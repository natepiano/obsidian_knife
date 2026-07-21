use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::PathBuf;

use chrono::NaiveDate;

use super::ObsidianRepository;
use crate::constants::FORMAT_DATE;
use crate::constants::FORWARD_SLASH;
use crate::constants::HASH;
use crate::constants::MARKDOWN_SUFFIX;
use crate::support;
use crate::validated_config::ValidatedConfig;

/// One wikilink whose target note does not exist and cannot be re-targeted automatically.
#[derive(Clone, Debug)]
pub(crate) struct UnresolvedLink {
    pub target:      String,
    pub file_path:   PathBuf,
    pub line_number: usize,
}

impl ObsidianRepository {
    /// Rewrites each `Wikilink.target` naming a real note to that note's file stem, when the
    /// stem names exactly one note in the vault. Obsidian path-qualifies links on file moves
    /// once two notes share a basename (`topics/service/LinkedIn`), and filenames contribute
    /// case variants (`Linkedin`); collapsing them lets `identify_ambiguous_matches` and the
    /// ambiguous-matches report count one candidate note instead of one per spelling, and
    /// back-populate replacements use the short form (`[[LinkedIn|linkedin]]`).
    ///
    /// Content links spelled with a rewritten target become `CanonicalLinkMatch` entries on
    /// their `MarkdownFile`, so `apply_replaceable_matches` also rewrites the existing links
    /// (`[[topics/service/LinkedIn|linkedin]]` to `[[LinkedIn|linkedin]]`).
    pub(crate) fn canonicalize_wikilink_targets(&mut self, validated_config: &ValidatedConfig) {
        // `stems_by_lower` maps a lowercased stem to the actual-case stem of every note
        // bearing it; only a stem naming a single note canonicalizes.
        let mut stems_by_lower: HashMap<String, Vec<String>> = HashMap::new();
        // `stems_by_relative_path` maps a note's lowercased vault-relative path (without
        // `MARKDOWN_SUFFIX`) to its stem, resolving path-qualified targets.
        let mut stems_by_relative_path: HashMap<String, String> = HashMap::new();

        for markdown_file in &self.markdown_files {
            let Some(stem) = markdown_file.path.file_stem().and_then(OsStr::to_str) else {
                continue;
            };

            stems_by_lower
                .entry(stem.to_lowercase())
                .or_default()
                .push(stem.to_string());

            let relative_path = support::format_relative_path(
                &markdown_file.path,
                validated_config.obsidian_path(),
            );
            let path_key = relative_path
                .strip_suffix(MARKDOWN_SUFFIX)
                .unwrap_or(&relative_path)
                .to_lowercase();
            stems_by_relative_path.insert(path_key, stem.to_string());
        }

        // `canonical_targets` maps each lowercased target naming a real note to that note's
        // stem; `find_canonical_link_matches` rewrites content links through it.
        let mut canonical_targets: HashMap<String, String> = HashMap::new();
        for wikilink in &mut self.wikilinks_sorted {
            if let Some(canonical_target) =
                canonical_note_target(&wikilink.target, &stems_by_lower, &stems_by_relative_path)
            {
                canonical_targets.insert(wikilink.target.to_lowercase(), canonical_target.clone());
                wikilink.target = canonical_target;
            }
        }

        for markdown_file in &mut self.markdown_files {
            markdown_file.canonical_link_matches =
                markdown_file.find_canonical_link_matches(&canonical_targets, validated_config);
        }
    }

    /// A phantom wikilink names a note that does not exist. When its target text matches the
    /// display text of exactly one `Wikilink` backed by a real note (an alias or a filename),
    /// this re-targets the phantom entries in `wikilinks_sorted` at that note and records a
    /// `PhantomLinkMatch` for every content link to convert.
    ///
    /// Running before `find_all_back_populate_matches` lets replacements build against the
    /// real note and lets `identify_ambiguous_matches` see a single target.
    pub(crate) fn resolve_phantom_wikilinks(&mut self, validated_config: &ValidatedConfig) {
        let note_stems = self.markdown_note_stems();

        // `real_targets_by_display` maps lowercased display text to targets naming real notes.
        let mut real_targets_by_display: HashMap<String, HashSet<String>> = HashMap::new();
        for wikilink in &self.wikilinks_sorted {
            if target_resolves(&note_stems, &wikilink.target) {
                real_targets_by_display
                    .entry(wikilink.display_text.to_lowercase())
                    .or_default()
                    .insert(wikilink.target.clone());
            }
        }

        // `resolutions` maps each phantom target to the single real target its text matches.
        let mut resolutions: HashMap<String, String> = HashMap::new();
        for wikilink in &self.wikilinks_sorted {
            if target_resolves(&note_stems, &wikilink.target) {
                continue;
            }

            let phantom_key = wikilink.target.to_lowercase();
            if let Some(real_targets) = real_targets_by_display.get(&phantom_key)
                && real_targets.len() == 1
                && let Some(real_target) = real_targets.iter().next()
            {
                resolutions.insert(phantom_key, real_target.clone());
            }
        }

        if resolutions.is_empty() {
            return;
        }

        for wikilink in &mut self.wikilinks_sorted {
            if let Some(real_target) = resolutions.get(&wikilink.target.to_lowercase()) {
                wikilink.target.clone_from(real_target);
            }
        }

        for markdown_file in &mut self.markdown_files {
            markdown_file.phantom_link_matches =
                markdown_file.find_phantom_link_matches(&resolutions, validated_config);
        }
    }

    /// Collects every content wikilink still pointing at a note that does not exist, one
    /// `UnresolvedLink` per occurrence. Date targets are daily-note placeholders and are
    /// excluded.
    pub(crate) fn collect_unresolved_links(&self) -> Vec<UnresolvedLink> {
        let note_stems = self.markdown_note_stems();

        let unresolved_targets: HashSet<String> = self
            .wikilinks_sorted
            .iter()
            .filter(|wikilink| {
                !target_resolves(&note_stems, &wikilink.target) && !is_date_target(&wikilink.target)
            })
            .map(|wikilink| wikilink.target.to_lowercase())
            .collect();

        if unresolved_targets.is_empty() {
            return Vec::new();
        }

        let mut unresolved_links = Vec::new();
        for markdown_file in &self.markdown_files {
            markdown_file.for_each_content_wikilink(|line_number, _, spanned_wikilink| {
                if unresolved_targets.contains(&spanned_wikilink.wikilink.target.to_lowercase()) {
                    unresolved_links.push(UnresolvedLink {
                        target: spanned_wikilink.wikilink.target,
                        file_path: markdown_file.path.clone(),
                        line_number,
                    });
                }
            });
        }

        unresolved_links.sort_by(|a, b| {
            a.target
                .to_lowercase()
                .cmp(&b.target.to_lowercase())
                .then_with(|| a.file_path.cmp(&b.file_path))
                .then_with(|| a.line_number.cmp(&b.line_number))
        });

        unresolved_links
    }

    fn markdown_note_stems(&self) -> HashSet<String> {
        self.markdown_files
            .iter()
            .filter_map(|markdown_file| markdown_file.path.file_stem().and_then(OsStr::to_str))
            .map(str::to_lowercase)
            .collect()
    }
}

/// Returns the note stem a wikilink `target` names: the last path segment with any heading
/// suffix and `MARKDOWN_SUFFIX` removed, lowercased.
fn target_note_stem(target: &str) -> String {
    let without_heading = target.split(HASH).next().unwrap_or(target);
    let last_segment = without_heading
        .rsplit(FORWARD_SLASH)
        .next()
        .unwrap_or(without_heading)
        .trim();

    last_segment
        .strip_suffix(MARKDOWN_SUFFIX)
        .unwrap_or(last_segment)
        .to_lowercase()
}

fn target_resolves(note_stems: &HashSet<String>, target: &str) -> bool {
    note_stems.contains(&target_note_stem(target))
}

/// Returns the file stem to use as a canonical `Wikilink.target` when `target` names exactly
/// one note whose stem no other note shares: `topics/service/LinkedIn` and `linkedin` both
/// canonicalize to `LinkedIn`. Heading targets, paths naming no note, and stems naming
/// several notes return `None`.
fn canonical_note_target(
    target: &str,
    stems_by_lower: &HashMap<String, Vec<String>>,
    stems_by_relative_path: &HashMap<String, String>,
) -> Option<String> {
    if target.contains(HASH) {
        return None;
    }

    let stem_key = if target.contains(FORWARD_SLASH) {
        let trimmed = target.trim();
        let path_key = trimmed
            .strip_suffix(MARKDOWN_SUFFIX)
            .unwrap_or(trimmed)
            .to_lowercase();
        stems_by_relative_path.get(&path_key)?.to_lowercase()
    } else {
        target_note_stem(target)
    };

    match stems_by_lower.get(&stem_key)?.as_slice() {
        [only_note_stem] => Some(only_note_stem.clone()),
        _ => None,
    }
}

/// Date targets are daily-note links; a missing daily note is a placeholder, not a phantom.
fn is_date_target(target: &str) -> bool {
    NaiveDate::parse_from_str(&target_note_stem(target), FORMAT_DATE).is_ok()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::collections::HashMap;

    use super::canonical_note_target;
    use super::target_note_stem;
    use crate::obsidian_repository::ObsidianRepository;
    use crate::test_support;
    use crate::test_support::TestFileBuilder;
    use crate::validated_config::ChangeMode;

    #[test]
    fn test_phantom_link_converted_when_alias_matches() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        TestFileBuilder::new()
            .with_content("# Kali Amen's Page")
            .with_title("kali amen".to_string())
            .with_aliases(vec!["Kali".to_string()])
            .create(&temp_dir, "Kali Amen.md");

        TestFileBuilder::new()
            .with_content("[[Kali]] was here\nKali again")
            .with_title("diary".to_string())
            .create(&temp_dir, "diary.md");

        let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        let diary = obsidian_repository
            .markdown_files
            .iter()
            .find(|file| file.path.ends_with("diary.md"))
            .expect("Should find diary.md");

        assert_eq!(diary.phantom_link_matches.len(), 1);
        assert_eq!(diary.phantom_link_matches[0].found_text, "[[Kali]]");
        assert_eq!(
            diary.phantom_link_matches[0].replacement,
            "[[Kali Amen|Kali]]"
        );

        // The plaintext mention is unambiguous and back-populates against the real note.
        assert!(!diary.has_ambiguous_matches());
        assert_eq!(diary.back_populate_matches.unambiguous.len(), 1);
        assert_eq!(
            diary.back_populate_matches.unambiguous[0].replacement,
            "[[Kali Amen|Kali]]"
        );

        assert_eq!(
            diary.content,
            "[[Kali Amen|Kali]] was here\n[[Kali Amen|Kali]] again"
        );
    }

    #[test]
    fn test_phantom_link_with_two_candidates_stays_ambiguous() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        TestFileBuilder::new()
            .with_content("# Kali Amen's Page")
            .with_title("kali amen".to_string())
            .with_aliases(vec!["Kali".to_string()])
            .create(&temp_dir, "Kali Amen.md");

        TestFileBuilder::new()
            .with_content("# Kali Brubaker's Page")
            .with_title("kali brubaker".to_string())
            .with_aliases(vec!["Kali".to_string()])
            .create(&temp_dir, "Kali Brubaker.md");

        TestFileBuilder::new()
            .with_content("[[Kali]] and Kali in plain text")
            .with_title("diary".to_string())
            .create(&temp_dir, "diary.md");

        let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        let diary = obsidian_repository
            .markdown_files
            .iter()
            .find(|file| file.path.ends_with("diary.md"))
            .expect("Should find diary.md");

        assert!(
            diary.phantom_link_matches.is_empty(),
            "two real candidates leave the phantom link alone"
        );
        assert!(diary.has_ambiguous_matches());
        assert_eq!(diary.content.trim_end(), "[[Kali]] and Kali in plain text");
    }

    #[test]
    fn test_collect_unresolved_links_excludes_dates_and_resolved() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        TestFileBuilder::new()
            .with_content("# Kali Amen's Page")
            .with_title("kali amen".to_string())
            .with_aliases(vec!["Kali".to_string()])
            .create(&temp_dir, "Kali Amen.md");

        TestFileBuilder::new()
            .with_content("[[Kali]] converted\n[[Missing Note]] stays\n[[2026-01-01]] daily")
            .with_title("diary".to_string())
            .create(&temp_dir, "diary.md");

        let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        let unresolved_links = obsidian_repository.collect_unresolved_links();

        assert_eq!(
            unresolved_links.len(),
            1,
            "resolved phantoms and date placeholders are excluded"
        );
        assert_eq!(unresolved_links[0].target, "Missing Note");
        assert!(unresolved_links[0].file_path.ends_with("diary.md"));
    }

    #[test]
    fn test_path_qualified_targets_collapse_to_stem() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        std::fs::create_dir_all(temp_dir.path().join("topics/service")).unwrap();
        TestFileBuilder::new()
            .with_content("# LinkedIn")
            .with_title("linkedin".to_string())
            .create(&temp_dir, "topics/service/LinkedIn.md");

        TestFileBuilder::new()
            .with_content("[[topics/service/LinkedIn|LinkedIn]] profile\nrun a linkedin campaign")
            .with_title("diary".to_string())
            .create(&temp_dir, "diary.md");

        let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        let diary = obsidian_repository
            .markdown_files
            .iter()
            .find(|file| file.path.ends_with("diary.md"))
            .expect("Should find diary.md");

        assert!(
            !diary.has_ambiguous_matches(),
            "path-qualified and stem targets name the same note"
        );
        assert_eq!(diary.back_populate_matches.unambiguous.len(), 1);
        assert_eq!(
            diary.back_populate_matches.unambiguous[0].replacement,
            "[[LinkedIn|linkedin]]"
        );

        // The existing path-qualified link rewrites to the stem; its alias equals the stem, so
        // the aliased form collapses to the bare link.
        assert_eq!(diary.canonical_link_matches.len(), 1);
        assert_eq!(
            diary.canonical_link_matches[0].found_text,
            "[[topics/service/LinkedIn|LinkedIn]]"
        );
        assert_eq!(diary.canonical_link_matches[0].replacement, "[[LinkedIn]]");
        assert_eq!(
            diary.content,
            "[[LinkedIn]] profile\nrun a [[LinkedIn|linkedin]] campaign"
        );
    }

    #[test]
    fn test_shared_stem_targets_stay_ambiguous() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        std::fs::create_dir_all(temp_dir.path().join("topics/service")).unwrap();
        TestFileBuilder::new()
            .with_content("# LinkedIn")
            .with_title("linkedin".to_string())
            .create(&temp_dir, "topics/service/LinkedIn.md");

        TestFileBuilder::new()
            .with_content("a draft post")
            .with_title("linkedin".to_string())
            .create(&temp_dir, "Linkedin.md");

        TestFileBuilder::new()
            .with_content("[[topics/service/LinkedIn|LinkedIn]] profile\nrun a linkedin campaign")
            .with_title("diary".to_string())
            .create(&temp_dir, "diary.md");

        let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        let diary = obsidian_repository
            .markdown_files
            .iter()
            .find(|file| file.path.ends_with("diary.md"))
            .expect("Should find diary.md");

        assert!(
            diary.has_ambiguous_matches(),
            "two notes with the linkedin stem keep the match ambiguous"
        );
        assert!(!diary.has_unambiguous_matches());
    }

    #[test]
    fn test_canonical_note_target_variants() {
        let stems_by_lower = HashMap::from([
            ("linkedin".to_string(), vec!["LinkedIn".to_string()]),
            (
                "todo".to_string(),
                vec!["todo".to_string(), "Todo".to_string()],
            ),
        ]);
        let stems_by_relative_path = HashMap::from([(
            "topics/service/linkedin".to_string(),
            "LinkedIn".to_string(),
        )]);

        let test_cases = vec![
            ("topics/service/LinkedIn", Some("LinkedIn")),
            ("topics/service/LinkedIn.md", Some("LinkedIn")),
            ("linkedin", Some("LinkedIn")),
            ("LinkedIn#Jobs", None),
            ("topics/other/LinkedIn", None),
            ("todo", None),
        ];

        for (target, expected) in test_cases {
            assert_eq!(
                canonical_note_target(target, &stems_by_lower, &stems_by_relative_path).as_deref(),
                expected,
                "canonical target mismatch for: {target}"
            );
        }
    }

    #[test]
    fn test_target_note_stem_variants() {
        let test_cases = vec![
            ("diary/2023/09/2023-09-02", "2023-09-02"),
            ("Note.md", "note"),
            ("Note#Heading", "note"),
            (" Spaced ", "spaced"),
        ];

        for (target, expected_stem) in test_cases {
            assert_eq!(
                target_note_stem(target),
                expected_stem,
                "stem mismatch for target: {target}"
            );
        }
    }
}

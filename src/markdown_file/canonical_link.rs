use std::collections::HashMap;

use super::MarkdownFile;
use super::back_populate;
use super::replaceable_content::MatchType;
use super::replaceable_content::ReplaceableContent;
use crate::constants::ESCAPED_PIPE;
use crate::constants::FORWARD_SLASH;
use crate::constants::PIPE;
use crate::support;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::SpannedWikilink;
use crate::wikilink::ToWikilink;

/// One content wikilink whose target names a real note by something other than its file stem
/// (a vault path like `topics/service/LinkedIn`, or a case variant like `Linkedin`);
/// `replacement` rewrites the link to the stem.
#[derive(Clone, Debug)]
pub struct CanonicalLinkMatch {
    pub found_text:    String,
    pub line_number:   usize,
    pub position:      usize,
    pub relative_path: String,
    pub replacement:   String,
}

impl ReplaceableContent for CanonicalLinkMatch {
    fn line_number(&self) -> usize { self.line_number }

    fn position(&self) -> usize { self.position }

    fn get_replacement(&self) -> String { self.replacement.clone() }

    fn matched_text(&self) -> String { self.found_text.clone() }

    fn match_type(&self) -> MatchType { MatchType::CanonicalLink }
}

impl MarkdownFile {
    /// `canonical_targets` maps each lowercased `Wikilink.target` to the file stem of the note
    /// it names; every content wikilink spelled differently becomes a `CanonicalLinkMatch`.
    /// Bare path-qualified links take the stem as display text (`[[topics/service/LinkedIn]]`
    /// becomes `[[LinkedIn]]`). Every other link keeps its display text, so rendered prose
    /// never changes: `[[amazon]]` becomes `[[Amazon|amazon]]` — the same form back-populate
    /// gives a plaintext mention — and an alias equal to the stem collapses to the bare form.
    pub(crate) fn find_canonical_link_matches(
        &self,
        canonical_targets: &HashMap<String, String>,
        validated_config: &ValidatedConfig,
    ) -> Vec<CanonicalLinkMatch> {
        let mut matches = Vec::new();
        self.for_each_content_wikilink(|line_number, line, spanned_wikilink| {
            let SpannedWikilink { wikilink, span } = spanned_wikilink;
            let Some(canonical_target) = canonical_targets.get(&wikilink.target.to_lowercase())
            else {
                return;
            };

            let (start, end) = span;
            let found_text = line[start..end].to_string();

            // A bare path-qualified link displays its path; the stem is the readable form.
            let bare_path_link =
                wikilink.display_text == wikilink.target && wikilink.target.contains(FORWARD_SLASH);
            let mut replacement = if bare_path_link {
                canonical_target.to_wikilink()
            } else {
                canonical_target.to_aliased_wikilink(&wikilink.display_text)
            };
            if back_populate::is_in_markdown_table(line, &found_text) {
                replacement = replacement.replace(PIPE, ESCAPED_PIPE);
            }

            // A link already using the stem produces a replacement equal to its source text.
            if replacement == found_text {
                return;
            }

            matches.push(CanonicalLinkMatch {
                found_text,
                line_number,
                position: start,
                relative_path: support::format_relative_path(
                    &self.path,
                    validated_config.obsidian_path(),
                ),
                replacement,
            });
        });

        matches
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::test_support;
    use crate::test_support::TestFileBuilder;
    use crate::validated_config::ChangeMode;

    fn linkedin_canonical_targets() -> HashMap<String, String> {
        HashMap::from([
            (
                "topics/service/linkedin".to_string(),
                "LinkedIn".to_string(),
            ),
            ("linkedin".to_string(), "LinkedIn".to_string()),
        ])
    }

    #[test]
    fn test_find_canonical_link_matches_bare_and_aliased() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        let file_path = TestFileBuilder::new()
            .with_content(
                "[[topics/service/LinkedIn]] profile\n\
                 see [[topics/service/LinkedIn|linkedin]] ads\n\
                 also [[topics/service/LinkedIn|LinkedIn]] jobs\n\
                 and [[Linkedin]] history",
            )
            .create(&temp_dir, "diary.md");
        let markdown_file = test_support::get_test_markdown_file(file_path);

        let matches = markdown_file
            .find_canonical_link_matches(&linkedin_canonical_targets(), &validated_config);

        assert_eq!(matches.len(), 4);
        assert_eq!(matches[0].found_text, "[[topics/service/LinkedIn]]");
        assert_eq!(
            matches[0].replacement, "[[LinkedIn]]",
            "bare path links take the stem as display text"
        );
        assert_eq!(
            matches[1].found_text,
            "[[topics/service/LinkedIn|linkedin]]"
        );
        assert_eq!(
            matches[1].replacement, "[[LinkedIn|linkedin]]",
            "aliased links keep their display text"
        );
        assert_eq!(
            matches[2].replacement, "[[LinkedIn]]",
            "an alias equal to the stem collapses to the bare form"
        );
        assert_eq!(
            matches[3].replacement, "[[LinkedIn|Linkedin]]",
            "bare case-variant links keep their rendered text as the alias"
        );
    }

    #[test]
    fn test_find_canonical_link_matches_skips_canonical_links() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        let file_path = TestFileBuilder::new()
            .with_content("[[LinkedIn]] profile and [[LinkedIn|linkedin]] ads")
            .create(&temp_dir, "diary.md");
        let markdown_file = test_support::get_test_markdown_file(file_path);

        let matches = markdown_file
            .find_canonical_link_matches(&linkedin_canonical_targets(), &validated_config);

        assert!(
            matches.is_empty(),
            "links already using the stem are left alone"
        );
    }

    #[test]
    fn test_find_canonical_link_matches_escapes_table_pipes() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        let file_path = TestFileBuilder::new()
            .with_content("| [[topics/service/LinkedIn\\|li]] | note |")
            .create(&temp_dir, "diary.md");
        let markdown_file = test_support::get_test_markdown_file(file_path);

        let matches = markdown_file
            .find_canonical_link_matches(&linkedin_canonical_targets(), &validated_config);

        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].replacement, "[[LinkedIn\\|li]]",
            "markdown table replacements keep escaped pipes"
        );
    }
}

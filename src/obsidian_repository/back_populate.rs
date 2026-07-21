use std::cmp::Reverse;
use std::collections::HashMap;
use std::collections::HashSet;
use std::convert::AsRef;
use std::mem::take;
use std::path::Path;

use anyhow::Context as _;
use anyhow::Result as AnyhowResult;
use anyhow::bail;

use super::ObsidianRepository;
use super::constants::FIRST_CONTENT_LINE_NUMBER;
use super::constants::INVALID_UTF8_BOUNDARY_PREFIX;
use super::constants::MIN_AMBIGUOUS_TARGETS;
use super::constants::NESTED_PATTERN_WARNING;
use super::constants::TRIPLE_CLOSING_BRACKETS;
use super::constants::TRIPLE_OPENING_BRACKETS;
use super::constants::UNCLASSIFIED_MATCH_WARNING;
use super::constants::WIKILINKS_AUTOMATON_NOT_INITIALIZED;
use super::constants::WIKILINKS_AUTOMATON_NOT_INITIALIZED_DETAIL;
use crate::constants::NEWLINE;
use crate::markdown_file::BackPopulateMatch;
use crate::markdown_file::ImageLinkState;
use crate::markdown_file::MarkdownFile;
use crate::markdown_file::MatchType;
use crate::markdown_file::ReplaceableContent;
use crate::support::VecEnumFilter;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::Wikilink;

#[derive(Debug, Default)]
struct ChangeSet {
    match_types: Vec<MatchType>,
}

impl ChangeSet {
    fn merge(&mut self, match_type: MatchType) {
        if !self.contains(&match_type) {
            self.match_types.push(match_type);
        }
    }

    fn contains(&self, match_type: &MatchType) -> bool { self.match_types.contains(match_type) }
}

impl ObsidianRepository {
    pub fn identify_ambiguous_matches(&mut self) {
        // `target_map` records the canonical `Wikilink.target` for each lowercase target.
        let mut target_map: HashMap<String, String> = HashMap::new();
        for wikilink in &self.wikilinks_sorted {
            let lower_target = wikilink.target.to_lowercase();
            if !target_map.contains_key(&lower_target)
                || wikilink.target.to_lowercase() == wikilink.target
            {
                target_map.insert(lower_target.clone(), wikilink.target.clone());
            }
        }

        let mut display_text_map: HashMap<String, HashSet<String>> = HashMap::new();
        for wikilink in &self.wikilinks_sorted {
            let lower_display_text = wikilink.display_text.to_lowercase();
            let lower_target = wikilink.target.to_lowercase();
            if let Some(canonical_target) = target_map.get(&lower_target) {
                display_text_map
                    .entry(lower_display_text.clone())
                    .or_default()
                    .insert(canonical_target.clone());
            }
        }

        // `MarkdownFile.back_populate_matches.unambiguous` is split into ambiguous
        // and still-unambiguous matches.
        for markdown_file in &mut self.markdown_files {
            // `matches_by_text` groups matches by lowercased `BackPopulateMatch.found_text`.
            let mut matches_by_text: HashMap<String, Vec<BackPopulateMatch>> = HashMap::new();

            // `file_matches` takes ownership of `markdown_file.back_populate_matches.unambiguous`.
            let file_matches = take(&mut markdown_file.back_populate_matches.unambiguous);
            for match_info in file_matches {
                let lower_found_text = match_info.found_text.to_lowercase();
                matches_by_text
                    .entry(lower_found_text)
                    .or_default()
                    .push(match_info);
            }

            // `matches_by_text` entries are classified through the `display_text_map` lookup.
            for (found_text_lower, text_matches) in matches_by_text {
                if let Some(targets) = display_text_map.get(&found_text_lower) {
                    if targets.len() >= MIN_AMBIGUOUS_TARGETS {
                        // `BackPopulateMatch` values with multiple targets move into
                        // `markdown_file.back_populate_matches.ambiguous`.
                        markdown_file
                            .back_populate_matches
                            .ambiguous
                            .extend(text_matches.clone());
                    } else {
                        // `text_matches` remains in
                        // `markdown_file.back_populate_matches.unambiguous`.
                        markdown_file
                            .back_populate_matches
                            .unambiguous
                            .extend(text_matches);
                    }
                } else {
                    // Missing `display_text_map` entries keep the `BackPopulateMatch`
                    // values unambiguous and emit `UNCLASSIFIED_MATCH_WARNING`.
                    println!(
                        "{UNCLASSIFIED_MATCH_WARNING} '{found_text_lower}' in file '{}'",
                        markdown_file.path.display()
                    );
                    markdown_file
                        .back_populate_matches
                        .unambiguous
                        .extend(text_matches);
                }
            }
        }
    }

    pub fn find_all_back_populate_matches(
        &mut self,
        validated_config: &ValidatedConfig,
    ) -> AnyhowResult<()> {
        let automaton = self.wikilinks_automaton.as_ref().context(format!(
            "{WIKILINKS_AUTOMATON_NOT_INITIALIZED} — {WIKILINKS_AUTOMATON_NOT_INITIALIZED_DETAIL}"
        ))?;

        // AhoCorasick pattern indexes line up with `wikilinks_sorted` order.
        let sorted_wikilinks: Vec<&Wikilink> = self.wikilinks_sorted.iter().collect();

        self.markdown_files.process_files_for_back_populate_matches(
            validated_config,
            &sorted_wikilinks,
            automaton,
        );
        Ok(())
    }

    pub fn apply_replaceable_matches(&mut self, operational_timezone: &str) -> AnyhowResult<()> {
        for markdown_file in &mut self.markdown_files {
            let has_replaceable_image_links = markdown_file.image_links.iter().any(|link| {
                matches!(
                    link.state,
                    ImageLinkState::Missing
                        | ImageLinkState::Duplicate { .. }
                        | ImageLinkState::Incompatible { .. }
                )
            });

            if !markdown_file.has_unambiguous_matches()
                && !markdown_file.has_phantom_link_matches()
                && !has_replaceable_image_links
            {
                continue;
            }

            let sorted_replaceable_matches = Self::collect_replaceable_matches(markdown_file);

            if sorted_replaceable_matches.is_empty() {
                continue;
            }

            let mut updated_content = String::new();
            let mut content_line_number = FIRST_CONTENT_LINE_NUMBER;
            let mut change_set = ChangeSet::default();

            for (zero_based_idx, line) in markdown_file.content.lines().enumerate() {
                let current_content_line = zero_based_idx + 1;
                let absolute_line_number =
                    current_content_line + markdown_file.frontmatter_line_count;

                if content_line_number != current_content_line {
                    updated_content.push_str(line);
                    updated_content.push(NEWLINE);
                    continue;
                }

                let line_matches: Vec<&dyn ReplaceableContent> = sorted_replaceable_matches
                    .iter()
                    .filter(|m| m.line_number() == absolute_line_number)
                    .map(AsRef::as_ref)
                    .collect();

                if line_matches.is_empty() {
                    updated_content.push_str(line);
                    updated_content.push(NEWLINE);
                } else {
                    let updated_line =
                        apply_line_replacements(line, &line_matches, &markdown_file.path)?;

                    // ChangeSet records which MatchType values changed the MarkdownFile.
                    for line_match in &line_matches {
                        change_set.merge(line_match.match_type());
                    }

                    if !updated_line.is_empty() {
                        updated_content.push_str(&updated_line);
                        updated_content.push(NEWLINE);
                    }
                }
                content_line_number += 1;
            }

            markdown_file.content = updated_content.trim_end().to_string();

            if change_set.contains(&MatchType::BackPopulate) {
                markdown_file.mark_as_back_populated(operational_timezone)?;
            }
            if change_set.contains(&MatchType::ImageReference) {
                markdown_file.mark_image_reference_as_updated(operational_timezone)?;
            }
            if change_set.contains(&MatchType::PhantomLink) {
                markdown_file.mark_phantom_links_resolved(operational_timezone)?;
            }
        }
        Ok(())
    }

    fn collect_replaceable_matches(
        markdown_file: &MarkdownFile,
    ) -> Vec<Box<dyn ReplaceableContent>> {
        let mut matches = Vec::new();

        matches.extend(
            markdown_file
                .back_populate_matches
                .unambiguous
                .iter()
                .cloned()
                .map(|m| Box::new(m) as Box<dyn ReplaceableContent>),
        );

        matches.extend(
            markdown_file
                .phantom_link_matches
                .iter()
                .cloned()
                .map(|m| Box::new(m) as Box<dyn ReplaceableContent>),
        );

        matches.extend(
            markdown_file
                .image_links
                .filter_by_predicate(|state| {
                    matches!(
                        state,
                        ImageLinkState::Incompatible { .. }
                            | ImageLinkState::Duplicate { .. }
                            | ImageLinkState::Missing
                    )
                })
                .iter()
                .cloned()
                .map(|m| Box::new(m) as Box<dyn ReplaceableContent>),
        );

        // `apply_replaceable_matches` sorts replacement positions in descending byte order so
        // earlier `replace_range` calls cannot shift later `ReplaceableContent` spans.
        matches.sort_by_key(|m| (m.line_number(), Reverse(m.position())));

        matches
    }
}

fn apply_line_replacements(
    line: &str,
    line_matches: &[&dyn ReplaceableContent],
    file_path: &Path,
) -> AnyhowResult<String> {
    let mut updated_line = line.to_string();

    // `sorted_matches` orders `ReplaceableContent` by descending `position`.
    let mut sorted_matches = line_matches.to_vec();
    sorted_matches.sort_by_key(|m| Reverse(m.position()));

    let has_image_replacement = sorted_matches
        .iter()
        .any(|m| m.match_type() == MatchType::ImageReference);

    // `match_info` replacements use right-to-left order.
    for match_info in sorted_matches {
        let start = match_info.position();
        let end = start + match_info.matched_text().len();

        // `updated_line.is_char_boundary` guards `replace_range` byte indexes.
        if !updated_line.is_char_boundary(start) || !updated_line.is_char_boundary(end) {
            bail!(
                "{INVALID_UTF8_BOUNDARY_PREFIX}{}, line {}: match {start}..{end} in \
                 {updated_line:?}, found {:?}",
                file_path.display(),
                match_info.line_number(),
                match_info.matched_text(),
            );
        }

        // `updated_line.replace_range` writes the `ReplaceableContent` replacement.
        updated_line.replace_range(start..end, &match_info.get_replacement());

        // `TRIPLE_OPENING_BRACKETS` and `TRIPLE_CLOSING_BRACKETS` flag nested patterns.
        if updated_line.contains(TRIPLE_OPENING_BRACKETS)
            || updated_line.contains(TRIPLE_CLOSING_BRACKETS)
        {
            eprintln!(
                "\n{NESTED_PATTERN_WARNING} '{}', line {}.\nCurrent line:\n{updated_line}\n",
                file_path.display(),
                match_info.line_number(),
            );
        }
    }

    // `ImageReference` replacements use `normalize_spaces` after trimming.
    Ok(if has_image_replacement {
        let trimmed = updated_line.trim();
        if trimmed.is_empty() {
            String::new()
        } else {
            normalize_spaces(trimmed)
        }
    } else {
        updated_line
    })
}

fn normalize_spaces(text: &str) -> String { text.split_whitespace().collect::<Vec<_>>().join(" ") }

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use crate::markdown_file::BackPopulateMatch;
    use crate::markdown_file::MarkdownFile;
    use crate::markdown_file::MatchContext;
    use crate::markdown_files::MarkdownFiles;
    use crate::obsidian_repository::ObsidianRepository;
    use crate::support;
    use crate::test_support;
    use crate::test_support as test_utils;
    use crate::test_support::TestFileBuilder;
    use crate::validated_config::ChangeMode;
    use crate::wikilink::Wikilink;
    #[test]
    fn test_identify_ambiguous_matches() {
        let (temp_dir, validated_config, mut obsidian_repository) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        obsidian_repository.wikilinks_sorted = vec![
            Wikilink {
                display_text: "Ed".to_string(),
                target:       "Ed Barnes".to_string(),
            },
            Wikilink {
                display_text: "Ed".to_string(),
                target:       "Ed Stanfield".to_string(),
            },
            Wikilink {
                display_text: "Unique".to_string(),
                target:       "Unique Target".to_string(),
            },
        ];

        TestFileBuilder::new()
            .with_content("Ed wrote this")
            .create(&temp_dir, "test1.md");

        TestFileBuilder::new()
            .with_content("Unique wrote this")
            .create(&temp_dir, "test2.md");

        let mut test_file = MarkdownFile::new(
            temp_dir.path().join("test1.md"),
            validated_config.operational_timezone(),
        )
        .unwrap();
        test_file.back_populate_matches.unambiguous = vec![BackPopulateMatch {
            relative_path: "test1.md".to_string(),
            line_number:   1,
            line_text:     "Ed wrote this".to_string(),
            found_text:    "Ed".to_string(),
            replacement:   "[[Ed Barnes|Ed]]".to_string(),
            position:      0,
            match_context: MatchContext::Plaintext,
        }];

        let mut test_file2 = MarkdownFile::new(
            temp_dir.path().join("test2.md"),
            validated_config.operational_timezone(),
        )
        .unwrap();
        test_file2.back_populate_matches.unambiguous = vec![BackPopulateMatch {
            relative_path: "test2.md".to_string(),
            line_number:   1,
            line_text:     "Unique wrote this".to_string(),
            found_text:    "Unique".to_string(),
            replacement:   "[[Unique Target]]".to_string(),
            position:      0,
            match_context: MatchContext::Plaintext,
        }];

        obsidian_repository.markdown_files.push(test_file2);
        obsidian_repository.markdown_files.push(test_file);

        obsidian_repository.identify_ambiguous_matches();

        let test_file = obsidian_repository
            .markdown_files
            .iter()
            .find(|f| f.path.ends_with("test1.md"))
            .expect("Should find test1.md");

        assert!(
            !test_file.has_unambiguous_matches(),
            "Ed match should be removed from unambiguous"
        );
        assert_eq!(
            test_file.back_populate_matches.ambiguous.len(),
            1,
            "Ed match should be moved to ambiguous"
        );
        let ambiguous_match = &test_file.back_populate_matches.ambiguous[0];
        assert_eq!(ambiguous_match.found_text, "Ed");
        assert_eq!(ambiguous_match.line_text, "Ed wrote this");

        let test_file2 = obsidian_repository
            .markdown_files
            .iter()
            .find(|f| f.path.ends_with("test2.md"))
            .expect("Should find test2.md");
        assert_eq!(
            test_file2.back_populate_matches.unambiguous.len(),
            1,
            "Should have one unambiguous match"
        );
        assert_eq!(
            test_file2.back_populate_matches.unambiguous[0].found_text,
            "Unique"
        );
        assert!(
            !test_file2.has_ambiguous_matches(),
            "Should have no ambiguous matches"
        );
    }

    #[test]
    fn test_truly_ambiguous_targets() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        TestFileBuilder::new()
            .with_content("Amazon is huge")
            .create(&temp_dir, "test1.md");

        TestFileBuilder::new()
            .with_content("# Amazon (company)")
            .with_title("amazon (company)".to_string())
            .with_aliases(vec!["Amazon".to_string()])
            .create(&temp_dir, "Amazon (company).md");

        TestFileBuilder::new()
            .with_content("# Amazon (river)")
            .with_title("amazon (river)".to_string())
            .with_aliases(vec!["Amazon".to_string()])
            .create(&temp_dir, "Amazon (river).md");

        // Let `ObsidianRepository::new` find all the files and process them.
        let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        let test_file = obsidian_repository
            .markdown_files
            .iter()
            .find(|f| f.path.ends_with("test1.md"))
            .expect("Should find test1.md");

        assert!(
            !test_file.has_unambiguous_matches(),
            "All matches should be moved from unambiguous"
        );
        assert_eq!(
            test_file.back_populate_matches.ambiguous.len(),
            1,
            "Should have one match in ambiguous"
        );

        let ambiguous_match = &test_file.back_populate_matches.ambiguous[0];
        assert_eq!(ambiguous_match.found_text, "Amazon");
        assert_eq!(ambiguous_match.line_text, "Amazon is huge");
    }

    #[test]
    fn test_mixed_case_and_truly_ambiguous() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        TestFileBuilder::new()
            .with_content("# AWS")
            .with_title("aws".to_string())
            .create(&temp_dir, "AWS.md");

        TestFileBuilder::new()
            .with_content("# aws")
            .with_title("aws".to_string())
            .create(&temp_dir, "aws.md");

        TestFileBuilder::new()
            .with_content("# Amazon (company)")
            .with_title("amazon (company)".to_string())
            .with_aliases(vec!["Amazon".to_string()])
            .create(&temp_dir, "Amazon (company).md");

        TestFileBuilder::new()
            .with_content("# Amazon (river)")
            .with_title("amazon (river)".to_string())
            .with_aliases(vec!["Amazon".to_string()])
            .create(&temp_dir, "Amazon (river).md");

        TestFileBuilder::new()
            .with_content(
                r"AWS and aws are the same
Amazon is ambiguous",
            )
            .with_title("Test Document".to_string()) // This adds frontmatter with the title
            .create(&temp_dir, "test1.md");

        // Let `ObsidianRepository::new` find all the files and process them.
        let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        let test_file = obsidian_repository
            .markdown_files
            .iter()
            .find(|f| f.path.ends_with("test1.md"))
            .expect("Should find test1.md");

        assert_eq!(
            test_file.back_populate_matches.unambiguous.len(),
            2,
            "Both AWS case variations should remain as unambiguous"
        );

        let aws_match_count = test_file
            .back_populate_matches
            .unambiguous
            .iter()
            .filter(|m| m.found_text.to_lowercase() == "aws")
            .count();
        assert_eq!(
            aws_match_count, 2,
            "Should have both AWS case variations remaining"
        );

        assert_eq!(
            test_file.back_populate_matches.ambiguous.len(),
            1,
            "Should have one ambiguous match"
        );
        assert_eq!(
            test_file.back_populate_matches.ambiguous[0].found_text, "Amazon",
            "Amazon should be in ambiguous matches"
        );
    }

    // This test sets up an **ambiguous alias** (`"Nate"`) mapping to two different targets.
    // It ensures that the `identify_ambiguous_matches` function correctly **classifies** both
    // instances of `"Nate"` as **ambiguous**.
    //
    // `identify_ambiguous_matches` must handle **both unambiguous and ambiguous
    // matches simultaneously** without interference. Prior to this, the real-world failure was
    // that it would find `Karen` as an alias but not `karen` even though we have a
    // case-insensitive search. The problem with the old test is that when there were no
    // ambiguous matches, the lowercase `karen` was not getting stripped out and the
    // test would pass even though the real world failed. In this case we are creating a
    // more realistic test that has a mix of ambiguous and unambiguous matches.
    #[test]
    fn test_combined_ambiguous_and_unambiguous_matches() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        TestFileBuilder::new()
            .with_content(
                r"# Reference Page
Karen is here
karen is here too
Nate was here and so was Nate"
                    .to_string(),
            )
            .with_title("reference page".to_string())
            .create(&temp_dir, "other.md");

        TestFileBuilder::new()
            .with_content("# Karen McCoy's Page".to_string())
            .with_title("karen mccoy".to_string())
            .with_aliases(vec!["Karen".to_string()])
            .create(&temp_dir, "Karen McCoy.md");

        TestFileBuilder::new()
            .with_content("# Nate McCoy's Page".to_string())
            .with_title("nate mccoy".to_string())
            .with_aliases(vec!["Nate".to_string()])
            .create(&temp_dir, "Nate McCoy.md");

        TestFileBuilder::new()
            .with_content("# Nathan Dye's Page".to_string())
            .with_title("nathan dye".to_string())
            .with_aliases(vec!["Nate".to_string()])
            .create(&temp_dir, "Nathan Dye.md");

        // Let `ObsidianRepository::new` find all the files and process them.
        let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        let other_file = obsidian_repository
            .markdown_files
            .iter()
            .find(|f| f.path.ends_with("other.md"))
            .expect("Should find other.md");

        let karen_match_count = other_file
            .back_populate_matches
            .unambiguous
            .iter()
            .filter(|m| m.found_text.to_lowercase() == "karen")
            .count();
        assert_eq!(
            karen_match_count, 2,
            "Both Karen case variations should remain as unambiguous"
        );

        let nate_ambiguous_matches: Vec<_> = other_file
            .back_populate_matches
            .ambiguous
            .iter()
            .filter(|m| m.found_text == "Nate")
            .collect();
        assert_eq!(
            nate_ambiguous_matches.len(),
            2,
            "Should have both Nate matches in ambiguous"
        );

        assert!(
            nate_ambiguous_matches
                .iter()
                .any(|m| m.line_text == "Nate was here and so was Nate")
        );
    }

    #[test]
    fn test_find_matches_with_existing_wikilinks() {
        let content = "[[Some Link]] and Test Link in same line\n\
       Test Link [[Other Link]] Test Link mixed\n\
       This don't match\n\
       This don't match either\n\
       But this Test Link should match";

        let (_temp_dir, validated_config, mut obsidian_repository) =
            test_support::create_test_environment(ChangeMode::DryRun, None, None, Some(content));

        // `find_all_back_populate_matches` populates `ObsidianRepository.markdown_files`.
        obsidian_repository
            .find_all_back_populate_matches(&validated_config)
            .unwrap();

        // `matches` stores results from the first and only markdown file.
        let matches = &obsidian_repository.markdown_files[0].back_populate_matches;

        assert_eq!(
            matches.unambiguous.len(),
            4,
            "Mismatch in number of matches"
        );

        let expected_lines = vec![5, 6, 6, 9];
        let actual_lines: Vec<usize> = matches.unambiguous.iter().map(|m| m.line_number).collect();
        assert_eq!(
            actual_lines, expected_lines,
            "Mismatch in line numbers of matches"
        );
    }

    #[test]
    fn test_overlapping_wikilink_matches() {
        let content = "[[Kyriana McCoy|Kyriana]] - Kyri and [[Kalina McCoy|Kali]]";
        let wikilinks = vec![
            Wikilink {
                display_text: "Kyri".to_string(),
                target:       "Kyri".to_string(),
            },
            Wikilink {
                display_text: "Kyri".to_string(),
                target:       "Kyriana McCoy".to_string(),
            },
        ];

        let (_temp_dir, validated_config, mut obsidian_repository) =
            test_support::create_test_environment(
                ChangeMode::DryRun,
                None,
                Some(wikilinks),
                Some(content),
            );

        // `find_all_back_populate_matches` populates `ObsidianRepository.markdown_files`.
        obsidian_repository
            .find_all_back_populate_matches(&validated_config)
            .unwrap();

        // `matches` stores results from the first and only markdown file.
        let matches = &obsidian_repository.markdown_files[0].back_populate_matches;

        assert_eq!(matches.unambiguous.len(), 1, "Expected exactly one match");
        assert_eq!(
            matches.unambiguous[0].position, 28,
            "Expected match at position 28"
        );
    }

    #[test]
    fn test_alias_priority() {
        let wikilinks = vec![
            Wikilink {
                display_text: "tomatoes".to_string(),
                target:       "tomato".to_string(),
            },
            Wikilink {
                display_text: "tomatoes".to_string(),
                target:       "tomatoes".to_string(),
            },
        ];

        let (temp_dir, validated_config, mut obsidian_repository) =
            test_utils::create_test_environment(ChangeMode::DryRun, None, Some(wikilinks), None);

        let content = "I love tomatoes in my salad";
        test_utils::create_markdown_test_file(
            &temp_dir,
            "salad.md",
            content,
            &mut obsidian_repository,
        );

        obsidian_repository
            .find_all_back_populate_matches(&validated_config)
            .unwrap();

        // `total_matches` counts unambiguous matches across all files.
        let total_matches: usize = obsidian_repository
            .markdown_files
            .iter()
            .map(|file| file.back_populate_matches.unambiguous.len())
            .sum();

        assert_eq!(total_matches, 1, "Should find exactly one match");

        let file_with_matches = obsidian_repository
            .markdown_files
            .iter()
            .find(|file| file.has_unambiguous_matches())
            .expect("Should have a file with matches");

        let first_match = &file_with_matches.back_populate_matches.unambiguous[0];
        assert_eq!(first_match.found_text, "tomatoes");
        assert_eq!(
            first_match.replacement, "[[tomato|tomatoes]]",
            "Should use the alias form [[tomato|tomatoes]] instead of [[tomatoes]]"
        );
    }

    #[test]
    fn test_no_matches_for_frontmatter_aliases() {
        let (temp_dir, validated_config, mut obsidian_repository) =
            test_utils::create_test_environment(ChangeMode::DryRun, None, None, None);

        let wikilink = Wikilink {
            display_text: "Will".to_string(),
            target:       "William.md".to_string(),
        };

        obsidian_repository.wikilinks_sorted.clear();
        obsidian_repository.wikilinks_sorted.push(wikilink);
        obsidian_repository.wikilinks_automaton = Some(test_utils::build_aho_corasick(
            &obsidian_repository.wikilinks_sorted,
        ));

        let content = "Will is mentioned here but should not be replaced";
        let file_path = TestFileBuilder::new()
            .with_title("Will".to_string())
            .with_content(content.to_string())
            .create(&temp_dir, "Will.md");

        obsidian_repository
            .markdown_files
            .push(test_utils::get_test_markdown_file(file_path));

        obsidian_repository
            .find_all_back_populate_matches(&validated_config)
            .unwrap();

        // `total_matches` counts unambiguous matches in the single markdown file.
        let total_matches: usize = obsidian_repository
            .markdown_files
            .iter()
            .map(|file| file.back_populate_matches.unambiguous.len())
            .sum();

        assert_eq!(
            total_matches, 0,
            "Should not find matches on page's own name"
        );

        let other_file_path = TestFileBuilder::new()
            .with_title("Other".to_string())
            .with_content(content.to_string())
            .create(&temp_dir, "Other.md");

        obsidian_repository
            .markdown_files
            .push(test_utils::get_test_markdown_file(other_file_path));

        obsidian_repository
            .find_all_back_populate_matches(&validated_config)
            .unwrap();

        // `total_matches` includes the additional markdown file.
        let total_matches: usize = obsidian_repository
            .markdown_files
            .iter()
            .map(|file| file.back_populate_matches.unambiguous.len())
            .sum();

        assert_eq!(total_matches, 1, "Should find match on other pages");
    }

    #[test]
    fn test_no_self_referential_back_population() {
        let (temp_dir, validated_config, mut obsidian_repository) =
            test_utils::create_test_environment(ChangeMode::DryRun, None, None, None);

        let wikilink = Wikilink {
            display_text: "Will".to_string(),
            target:       "William.md".to_string(),
        };

        obsidian_repository.wikilinks_sorted.clear();
        obsidian_repository.wikilinks_sorted.push(wikilink);
        obsidian_repository.wikilinks_automaton = Some(test_utils::build_aho_corasick(
            &obsidian_repository.wikilinks_sorted,
        ));

        let content = "Will is mentioned here but should not be replaced";
        test_utils::create_markdown_test_file(
            &temp_dir,
            "Will.md",
            content,
            &mut obsidian_repository,
        );

        obsidian_repository
            .find_all_back_populate_matches(&validated_config)
            .unwrap();

        // `total_matches` counts unambiguous matches in the single markdown file.
        let total_matches: usize = obsidian_repository
            .markdown_files
            .iter()
            .map(|file| file.back_populate_matches.unambiguous.len())
            .sum();

        assert_eq!(
            total_matches, 0,
            "Should not find matches on page's own name"
        );

        let other_file_path = test_utils::create_markdown_test_file(
            &temp_dir,
            "Other.md",
            content,
            &mut obsidian_repository,
        );

        obsidian_repository
            .find_all_back_populate_matches(&validated_config)
            .unwrap();

        // `total_matches` includes the additional markdown file.
        let total_matches: usize = obsidian_repository
            .markdown_files
            .iter()
            .map(|file| file.back_populate_matches.unambiguous.len())
            .sum();

        assert_eq!(total_matches, 1, "Should find match on other pages");

        let file_with_matches = obsidian_repository
            .markdown_files
            .iter()
            .find(|file| file.has_unambiguous_matches())
            .expect("Should have a file with matches");

        assert_eq!(
            support::format_relative_path(
                &file_with_matches.path,
                validated_config.obsidian_path(),
            ),
            support::format_relative_path(&other_file_path, validated_config.obsidian_path(),),
            "Match should be in 'Other.md'"
        );
    }

    #[test]
    fn test_apply_changes() {
        let initial_content = "This is Test Link in a sentence.";
        let (_temp_dir, validated_config, mut obsidian_repository) =
            test_support::create_test_environment(
                ChangeMode::Apply,
                None,
                None,
                Some(initial_content),
            );

        obsidian_repository
            .find_all_back_populate_matches(&validated_config)
            .unwrap();

        obsidian_repository
            .apply_replaceable_matches(validated_config.operational_timezone())
            .unwrap();

        assert_eq!(
            obsidian_repository.markdown_files[0].content,
            "This is [[Test Link]] in a sentence."
        );
    }

    #[test]
    fn test_case_insensitive_targets() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        TestFileBuilder::new()
            .with_content("# Sample\nAmazon") // Changed to not use "Test" in content
            .with_title("Sample".to_string()) // Changed from "Test"
            .create(&temp_dir, "Amazon.md");

        TestFileBuilder::new()
            .with_content("# Sample Document\nAmazon is huge\namazon is also huge")
            .with_title("Test Document".to_string()) // This adds frontmatter with the title
            .create(&temp_dir, "test1.md");

        let mut obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        let test_file = obsidian_repository
            .markdown_files
            .iter()
            .find(|f| f.path.ends_with("test1.md"))
            .expect("Should find test1.md");

        assert_eq!(
            test_file.back_populate_matches.unambiguous.len(),
            2,
            "Should have matches for both case variations"
        );

        // `identify_ambiguous_matches` moves alias collisions into ambiguous matches.
        obsidian_repository.identify_ambiguous_matches();

        let test_file = obsidian_repository
            .markdown_files
            .iter()
            .find(|f| f.path.ends_with("test1.md"))
            .expect("Should find test1.md");

        assert_eq!(
            test_file.back_populate_matches.unambiguous.len(),
            2,
            "Both matches should be considered unambiguous"
        );
    }

    #[test]
    fn test_back_populate_content() {
        let (temp_dir, validated_config, mut obsidian_repository) =
            test_support::create_test_environment(ChangeMode::Apply, None, None, None);

        let test_cases = vec![(
            "# Test Table\n|Name|Description|\n|---|---|\n|Test Link|Sample text|\n",
            vec![BackPopulateMatch {
                relative_path: "test.md".into(),
                line_number:   4,
                line_text:     "|Test Link|Sample text|".into(),
                found_text:    "Test Link".into(),
                replacement:   "[[Test Link\\|Another Name]]".into(),
                position:      1,
                match_context: MatchContext::MarkdownTable,
            }],
            "Table content replacement",
        )];

        for (content, matches, description) in test_cases {
            let file = TestFileBuilder::new()
                .with_content(content.to_string())
                .with_title("test".to_string())
                .create(&temp_dir, "test.md");

            let markdown_file = {
                let mut markdown_file =
                    MarkdownFile::new(file.clone(), validated_config.operational_timezone())
                        .unwrap();
                markdown_file.content = content.to_string();
                markdown_file.back_populate_matches.unambiguous = matches.clone();
                markdown_file
            };

            obsidian_repository.markdown_files = MarkdownFiles::new(vec![markdown_file], None);

            obsidian_repository
                .apply_replaceable_matches(validated_config.operational_timezone())
                .unwrap();

            // `file.content` contains the back-populate replacements.
            if let Some(file) = obsidian_repository
                .markdown_files
                .iter()
                .find(|f| f.path == file)
            {
                for match_info in &matches {
                    assert!(
                        file.content.contains(&match_info.replacement),
                        "Failed for: {}\nReplacement '{}' not found in content:\n{}",
                        description,
                        match_info.replacement,
                        file.content
                    );
                }
            }
        }
    }
}

use std::ffi::OsStr;

use aho_corasick::AhoCorasick;

use super::MarkdownFile;
use super::matching;
use super::replaceable_content::MatchType;
use super::replaceable_content::ReplaceableContent;
use super::text_excluder::CodeBlockExcluder;
use super::text_excluder::InlineCodeExcluder;
use crate::constants::ESCAPED_PIPE;
use crate::constants::PIPE;
use crate::support;
use crate::support::MARKDOWN_REGEX;
use crate::validated_config::ValidatedConfig;
use crate::wikilink;
use crate::wikilink::ToWikilink;
use crate::wikilink::Wikilink;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MatchContext {
    #[default]
    Plaintext,
    MarkdownTable,
}

#[derive(Clone, Debug, Default)]
pub struct BackPopulateMatch {
    pub found_text:    String,
    pub match_context: MatchContext,
    pub line_number:   usize,
    pub line_text:     String,
    pub position:      usize,
    pub relative_path: String,
    pub replacement:   String,
}

impl ReplaceableContent for BackPopulateMatch {
    fn line_number(&self) -> usize { self.line_number }

    fn position(&self) -> usize { self.position }

    fn get_replacement(&self) -> String { self.replacement.clone() }

    fn matched_text(&self) -> String { self.found_text.clone() }

    fn match_type(&self) -> MatchType { MatchType::BackPopulate }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BackPopulateMatches {
    pub ambiguous:   Vec<BackPopulateMatch>,
    pub unambiguous: Vec<BackPopulateMatch>,
}

impl MarkdownFile {
    pub(super) fn process_file_for_back_populate_replacements_inner(
        &mut self,
        sorted_wikilinks: &[&Wikilink],
        validated_config: &ValidatedConfig,
        automaton: &AhoCorasick,
    ) {
        let content = self.content.clone();
        let mut code_block_excluder = CodeBlockExcluder::new();

        for (line_idx, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            code_block_excluder.update(line);
            if code_block_excluder.is_in_code_block() {
                continue;
            }

            let matches = self.process_line_for_back_populate_replacements(
                line,
                line_idx,
                automaton,
                sorted_wikilinks,
                validated_config,
            );

            self.back_populate_matches.unambiguous.extend(matches);
        }
    }

    pub(super) fn process_line_for_back_populate_replacements(
        &self,
        line: &str,
        line_idx: usize,
        automaton: &AhoCorasick,
        sorted_wikilinks: &[&Wikilink],
        validated_config: &ValidatedConfig,
    ) -> Vec<BackPopulateMatch> {
        let mut matches = Vec::new();
        let exclusion_zones = self.collect_exclusion_zones(line, validated_config);

        for match_result in automaton.find_iter(line) {
            let wikilink = sorted_wikilinks[match_result.pattern()];
            let starts_at = match_result.start();
            let ends_at = match_result.end();

            if matching::range_overlaps(&exclusion_zones, starts_at, ends_at) {
                continue;
            }

            let matched_text = &line[starts_at..ends_at];
            if !matching::is_word_boundary(line, starts_at, ends_at) {
                continue;
            }

            if self.should_create_match(line, starts_at, matched_text) {
                let mut replacement = if matched_text == wikilink.target {
                    wikilink.target.to_wikilink()
                } else {
                    wikilink.target.to_aliased_wikilink(matched_text)
                };

                let match_context = if matching::is_in_markdown_table(line, matched_text) {
                    MatchContext::MarkdownTable
                } else {
                    MatchContext::Plaintext
                };
                if match_context == MatchContext::MarkdownTable {
                    replacement = replacement.replace(PIPE, ESCAPED_PIPE);
                }

                let relative_path =
                    support::format_relative_path(&self.path, validated_config.obsidian_path());

                matches.push(BackPopulateMatch {
                    found_text: matched_text.to_string(),
                    line_number: self.get_real_line_number(line_idx),
                    line_text: line.to_string(),
                    position: starts_at,
                    match_context,
                    relative_path,
                    replacement,
                });
            }
        }

        matches
    }

    pub(super) fn collect_exclusion_zones(
        &self,
        line: &str,
        validated_config: &ValidatedConfig,
    ) -> Vec<(usize, usize)> {
        let mut exclusion_zones = Vec::new();

        // InvalidWikilink spans block back-populate matches.
        for invalid_wikilink in &self.wikilinks.invalid {
            // InvalidWikilink.line stores the original line text, not a line number.
            if invalid_wikilink.line == line {
                exclusion_zones.push(invalid_wikilink.span);
            }
        }

        let regex_sources = [
            validated_config.do_not_back_populate_regexes(),
            self.do_not_back_populate_regexes.as_deref(),
        ];

        for do_not_back_populate_regexes in regex_sources.iter().flatten() {
            for regex in *do_not_back_populate_regexes {
                for regex_match in regex.find_iter(line) {
                    exclusion_zones.push((regex_match.start(), regex_match.end()));
                }
            }
        }

        // InlineCodeExcluder spans block back-populate matches.
        let mut inline_code_excluder = InlineCodeExcluder::new();
        let mut span_start = None;
        for (byte_offset, ch) in line.char_indices() {
            let was_inside = inline_code_excluder.is_in_code_block();
            inline_code_excluder.update(ch);
            let is_inside = inline_code_excluder.is_in_code_block();

            if !was_inside && is_inside {
                span_start = Some(byte_offset);
            } else if was_inside
                && !is_inside
                && let Some(start) = span_start.take()
            {
                exclusion_zones.push((start, byte_offset + ch.len_utf8()));
            }
        }

        // Markdown link spans block back-populate matches.
        for markdown_link_match in MARKDOWN_REGEX.find_iter(line) {
            exclusion_zones.push((markdown_link_match.start(), markdown_link_match.end()));
        }

        // matching::range_overlaps expects spans ordered by start byte.
        exclusion_zones.sort_by_key(|&(start, _)| start);
        exclusion_zones
    }

    pub(super) fn should_create_match(
        &self,
        line: &str,
        absolute_start: usize,
        matched_text: &str,
    ) -> bool {
        // `matched_text` cannot target the current `MarkdownFile` stem.
        if let Some(stem) = self.path.file_stem().and_then(OsStr::to_str) {
            if stem.eq_ignore_ascii_case(matched_text) {
                return false;
            }

            // `matched_text` cannot target the current frontmatter aliases.
            if let Some(frontmatter) = &self.frontmatter
                && let Some(aliases) = frontmatter.aliases()
                && aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(matched_text))
            {
                return false;
            }
        }

        !wikilink::is_within_wikilink(line, absolute_start)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::slice;

    use tempfile::TempDir;

    use crate::constants::DEFAULT_TIMEZONE;
    use crate::markdown_file::BackPopulateMatch;
    use crate::markdown_file::MarkdownFile;
    use crate::markdown_file::MatchContext;
    use crate::obsidian_repository::ObsidianRepository;
    use crate::support;
    use crate::test_support;
    use crate::test_support as test_utils;
    use crate::test_support::TestFileBuilder;
    use crate::validated_config::ChangeMode;
    use crate::wikilink::Wikilink;

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

        // Verify we got exactly one match
        assert_eq!(total_matches, 1, "Should find exactly one match");

        // Find the file that has matches
        let file_with_matches = obsidian_repository
            .markdown_files
            .iter()
            .find(|file| file.has_unambiguous_matches())
            .expect("Should have a file with matches");

        // Verify the match uses the alias form
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

        // Test with different file using same text
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

        // Find the file with matches and check its path
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
    fn test_markdown_file_aliases_only() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = TestFileBuilder::new()
            .with_aliases(vec!["Only Alias".to_string()])
            .with_content("# Test Content".to_string())
            .create(&temp_dir, "test.md");

        let markdown_file = MarkdownFile::new(file_path, DEFAULT_TIMEZONE).unwrap();

        assert!(markdown_file.do_not_back_populate_regexes.is_some());
        let regexes = markdown_file.do_not_back_populate_regexes.unwrap();
        assert_eq!(regexes.len(), 1);

        let test_line = "Only Alias appears here";
        assert!(regexes[0].is_match(test_line));
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

        // First find the matches
        obsidian_repository
            .find_all_back_populate_matches(&validated_config)
            .unwrap();

        // Apply the changes
        obsidian_repository
            .apply_replaceable_matches(validated_config.operational_timezone())
            .unwrap();

        // Verify changes by checking `MarkdownFile` content
        assert_eq!(
            obsidian_repository.markdown_files[0].content,
            "This is [[Test Link]] in a sentence."
        );
    }

    #[test]
    fn test_config_creation() {
        // Basic usage with defaults
        let (_, basic_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, None, None);
        assert_eq!(basic_config.change_mode(), ChangeMode::DryRun);

        // With apply_changes set to true
        let (_, apply_config, _) =
            test_support::create_test_environment(ChangeMode::Apply, None, None, None);
        assert_eq!(apply_config.change_mode(), ChangeMode::Apply);

        // With do_not_back_populate patterns
        let patterns = vec!["pattern1".to_string(), "pattern2".to_string()];
        let (_, pattern_config, _) = test_support::create_test_environment(
            ChangeMode::DryRun,
            Some(patterns.clone()),
            None,
            None,
        );
        let Some(regexes) = pattern_config.do_not_back_populate_regexes() else {
            panic!("expected do-not-back-populate regexes")
        };
        assert_eq!(regexes.len(), patterns.len());
        for pattern in &patterns {
            assert!(
                regexes.iter().any(|regex| regex.is_match(pattern)),
                "missing regex for pattern {pattern}"
            );
        }

        // With both parameters
        let (_, full_config, _) = test_support::create_test_environment(
            ChangeMode::Apply,
            Some(vec!["pattern".to_string()]),
            None,
            None,
        );
        assert_eq!(full_config.change_mode(), ChangeMode::Apply);
        assert!(full_config.do_not_back_populate_regexes().is_some());
    }

    #[test]
    fn test_scan_markdown_file_with_do_not_back_populate() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = TestFileBuilder::new()
            .with_content("# Test Content".to_string())
            .with_custom_frontmatter(
                r#"do_not_back_populate:
- "test phrase"
- "another phrase"
"#
                .to_string(),
            )
            .create(&temp_dir, "test.md");

        let markdown_file = MarkdownFile::new(file_path, DEFAULT_TIMEZONE).unwrap();

        assert!(markdown_file.do_not_back_populate_regexes.is_some());
        let regexes = markdown_file.do_not_back_populate_regexes.unwrap();
        assert_eq!(regexes.len(), 2);

        let test_line = "here is a test phrase and another phrase";
        assert!(regexes[0].is_match(test_line));
        assert!(regexes[1].is_match(test_line));
    }

    #[test]
    fn test_scan_markdown_file_combines_aliases_with_do_not_back_populate() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = TestFileBuilder::new()
            .with_aliases(vec!["First Alias".to_string(), "Second Alias".to_string()])
            .with_custom_frontmatter(
                r#"do_not_back_populate:
- "exclude this"
"#
                .to_string(),
            )
            .with_content("# Test Content".to_string())
            .create(&temp_dir, "test.md");

        let markdown_file = MarkdownFile::new(file_path, DEFAULT_TIMEZONE).unwrap();

        assert!(markdown_file.do_not_back_populate_regexes.is_some());
        let regexes = markdown_file.do_not_back_populate_regexes.unwrap();
        assert_eq!(regexes.len(), 3);

        let test_line = "First Alias and Second Alias and exclude this";
        assert!(regexes[0].is_match(test_line));
        assert!(regexes[1].is_match(test_line));
        assert!(regexes[2].is_match(test_line));
    }

    // Helper struct for test cases
    struct TestCase {
        content:          &'static str,
        wikilink:         Wikilink,
        expected_matches: Vec<(&'static str, &'static str)>,
        description:      &'static str,
    }

    fn get_case_sensitivity_test_cases() -> Vec<TestCase> {
        vec![
            TestCase {
                content:          "test link TEST LINK Test Link",
                wikilink:         Wikilink {
                    display_text: "Test Link".to_string(),
                    target:       "Test Link".to_string(),
                },
                // careful - these must match the order returned by process_line
                expected_matches: vec![
                    ("test link", "[[Test Link|test link]]"),
                    ("TEST LINK", "[[Test Link|TEST LINK]]"),
                    ("Test Link", "[[Test Link]]"),
                ],
                description:      "Basic case-insensitive matching",
            },
            TestCase {
                content:          "josh likes apples",
                wikilink:         Wikilink {
                    display_text: "josh".to_string(),
                    target:       "Joshua Strayhorn".to_string(),
                },
                expected_matches: vec![("josh", "[[Joshua Strayhorn|josh]]")],
                description:      "Alias case preservation",
            },
            TestCase {
                content:          "karen likes math",
                wikilink:         Wikilink {
                    display_text: "Karen".to_string(),
                    target:       "Karen McCoy".to_string(),
                },
                expected_matches: vec![("karen", "[[Karen McCoy|karen]]")],
                description:      "Alias case preservation when display case differs from content",
            },
            TestCase {
                content:          "| Test Link | Another test link |",
                wikilink:         Wikilink {
                    display_text: "Test Link".to_string(),
                    target:       "Test Link".to_string(),
                },
                expected_matches: vec![
                    ("Test Link", "[[Test Link]]"),
                    ("test link", "[[Test Link|test link]]"),
                ],
                description:      "Case handling in tables",
            },
        ]
    }

    fn verify_match(
        actual_match: &BackPopulateMatch,
        expected_text: &str,
        expected_base_replacement: &str,
        case_description: &str,
    ) {
        assert_eq!(
            actual_match.found_text, expected_text,
            "Wrong matched text for case: {case_description}"
        );

        let expected_replacement = if actual_match.match_context == MatchContext::MarkdownTable {
            expected_base_replacement.replace('|', r"\|")
        } else {
            expected_base_replacement.to_string()
        };

        assert_eq!(
            actual_match.replacement,
            expected_replacement,
            "Wrong replacement for case: {}\nExpected: {}\nActual: {}\nIn table: {}",
            case_description,
            expected_replacement,
            actual_match.replacement,
            actual_match.match_context == MatchContext::MarkdownTable
        );
    }

    #[test]
    fn test_case_insensitive_targets() {
        // Create test environment
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        // Create test files with case variations using `TestFileBuilder`
        TestFileBuilder::new()
            .with_content("# Sample\nAmazon") // Changed to not use "Test" in content
            .with_title("Sample".to_string()) // Changed from "Test"
            .create(&temp_dir, "Amazon.md");

        TestFileBuilder::new()
            .with_content("# Sample Document\nAmazon is huge\namazon is also huge")
            .with_title("Test Document".to_string()) // This adds frontmatter with the title
            .create(&temp_dir, "test1.md");

        // Scan folders to populate repository
        let mut obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        // Find our test file
        let test_file = obsidian_repository
            .markdown_files
            .iter()
            .find(|f| f.path.ends_with("test1.md"))
            .expect("Should find test1.md");

        // Verify we found both case variations initially
        assert_eq!(
            test_file.back_populate_matches.unambiguous.len(),
            2,
            "Should have matches for both case variations"
        );

        // `identify_ambiguous_matches` moves alias collisions into ambiguous matches.
        obsidian_repository.identify_ambiguous_matches();

        // Find our test file again after ambiguous matching
        let test_file = obsidian_repository
            .markdown_files
            .iter()
            .find(|f| f.path.ends_with("test1.md"))
            .expect("Should find test1.md");

        // All matches should remain in the markdown file as unambiguous
        assert_eq!(
            test_file.back_populate_matches.unambiguous.len(),
            2,
            "Both matches should be considered unambiguous"
        );
    }

    #[test]
    fn test_case_sensitivity_behavior() {
        // Initialize test environment without specific wikilinks
        let (temp_dir, validated_config, mut obsidian_repository) =
            test_support::create_test_environment(ChangeMode::DryRun, None, None, None);

        for case in get_case_sensitivity_test_cases() {
            let file_path = test_support::create_markdown_test_file(
                &temp_dir,
                "test.md",
                case.content,
                &mut obsidian_repository,
            );

            // Create a custom wikilink and build AC automaton directly
            let wikilink = case.wikilink;
            let automaton = test_support::build_aho_corasick(slice::from_ref(&wikilink));

            let markdown_file =
                MarkdownFile::new(file_path.clone(), validated_config.operational_timezone())
                    .unwrap();

            let matches = markdown_file.process_line_for_back_populate_replacements(
                case.content,
                0,
                &automaton,
                &[&wikilink],
                &validated_config,
            );

            assert_eq!(
                matches.len(),
                case.expected_matches.len(),
                "Wrong number of matches for case: {}",
                case.description
            );

            for ((expected_text, expected_base_replacement), actual_match) in
                case.expected_matches.iter().zip(matches.iter())
            {
                verify_match(
                    actual_match,
                    expected_text,
                    expected_base_replacement,
                    case.description,
                );
            }
        }
    }
}

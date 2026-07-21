use std::ffi::OsStr;

use aho_corasick::AhoCorasick;

use super::MarkdownFile;
use super::constants::APOSTROPHE;
use super::constants::MAX_OBSIDIAN_LINK_PIPE_COUNT;
use super::constants::RIGHT_SINGLE_QUOTATION_MARK;
use super::constants::T_LOWER;
use super::constants::T_UPPER;
use super::constants::UNDERSCORE;
use super::replaceable_content::MatchType;
use super::replaceable_content::ReplaceableContent;
use super::text_excluder::CodeBlockExcluder;
use super::text_excluder::InlineCodeExcluder;
use crate::constants::ESCAPED_PIPE;
use crate::constants::PIPE;
use crate::constants::SPACE;
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

            if range_overlaps(&exclusion_zones, starts_at, ends_at) {
                continue;
            }

            let matched_text = &line[starts_at..ends_at];
            if !is_word_boundary(line, starts_at, ends_at) {
                continue;
            }

            if self.should_create_match(line, starts_at, matched_text) {
                let mut replacement = if matched_text == wikilink.target {
                    wikilink.target.to_wikilink()
                } else {
                    wikilink.target.to_aliased_wikilink(matched_text)
                };

                let match_context = if is_in_markdown_table(line, matched_text) {
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

        // `range_overlaps` expects spans ordered by start byte.
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
            if let Some(front_matter) = &self.front_matter
                && let Some(aliases) = front_matter.aliases()
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

fn is_word_boundary(line: &str, starts_at: usize, ends_at: usize) -> bool {
    // Word characters match Rust alphanumerics plus UNDERSCORE.
    fn is_word_char(ch: char) -> bool { ch.is_alphanumeric() || ch == UNDERSCORE }

    // T-contractions block a word boundary after apostrophe+t.
    fn is_t_contraction(chars: &str) -> bool {
        let mut chars = chars.chars();
        matches!(
            (chars.next(), chars.next()),
            (
                Some(APOSTROPHE | RIGHT_SINGLE_QUOTATION_MARK),
                Some(T_LOWER | T_UPPER)
            )
        )
    }

    // before and after_chars provide the neighbor characters for boundary checks.
    let before = line[..starts_at].chars().last();
    let after_chars = &line[ends_at..];

    let start_is_boundary = starts_at == 0 || before.is_none_or(|ch| !is_word_char(ch));

    // Possessive suffixes remain valid replacement candidates.
    let end_is_boundary = ends_at == line.len()
        || (!is_word_char(after_chars.chars().next().unwrap_or(SPACE))
            && !is_t_contraction(after_chars));

    start_is_boundary && end_is_boundary
}

fn range_overlaps(ranges: &[(usize, usize)], start: usize, end: usize) -> bool {
    ranges.iter().any(|&(r_start, r_end)| {
        (start >= r_start && start < r_end)
            || (end > r_start && end <= r_end)
            || (start <= r_start && end >= r_end)
    })
}

pub(super) fn is_in_markdown_table(line: &str, matched_text: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with(PIPE)
        && trimmed.ends_with(PIPE)
        && trimmed.matches(PIPE).count() > MAX_OBSIDIAN_LINK_PIPE_COUNT
        && trimmed.contains(matched_text)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::slice;

    use crate::markdown_file::BackPopulateMatch;
    use crate::markdown_file::MarkdownFile;
    use crate::markdown_file::MatchContext;
    use crate::test_support;
    use crate::test_support::TestFileBuilder;
    use crate::validated_config::ChangeMode;
    use crate::wikilink::InvalidWikilink;
    use crate::wikilink::InvalidWikilinkReason;
    use crate::wikilink::Wikilink;

    #[test]
    fn test_collect_exclusion_zones_with_invalid_wikilinks() {
        let (_, validated_config, mut obsidian_repository) = test_support::create_test_environment(
            ChangeMode::DryRun,
            None,
            None,
            Some("Text [[invalid|link|extra]] and more text"),
        );

        let markdown_file = obsidian_repository.markdown_files.first_mut().unwrap();

        markdown_file.wikilinks.invalid.push(InvalidWikilink {
            content:     "[[invalid|link|extra]]".to_string(),
            reason:      InvalidWikilinkReason::DoubleAlias,
            span:        (5, 27),
            line:        "Text [[invalid|link|extra]] and more text".to_string(),
            line_number: 1,
        });

        let zones = markdown_file.collect_exclusion_zones(
            "Text [[invalid|link|extra]] and more text",
            &validated_config,
        );

        assert!(!zones.is_empty(), "Should have at least one exclusion zone");
        assert!(
            zones.contains(&(5, 27)),
            "Should contain invalid wikilink span"
        );
    }

    #[test]
    fn test_exclusion_zones_with_multiple_invalid_wikilinks() {
        let (_, validated_config, mut obsidian_repository) =
            test_support::create_test_environment(ChangeMode::DryRun, None, None, None);

        let markdown_file = obsidian_repository.markdown_files.first_mut().unwrap();

        markdown_file.wikilinks.invalid.extend(vec![
            InvalidWikilink {
                content:     "[[test|one|two]]".to_string(),
                reason:      InvalidWikilinkReason::DoubleAlias,
                span:        (0, 16),
                line:        "[[test|one|two]] some text [[]]".to_string(),
                line_number: 1,
            },
            InvalidWikilink {
                content:     "[[]]".to_string(),
                reason:      InvalidWikilinkReason::Empty,
                span:        (27, 31),
                line:        "[[test|one|two]] some text [[]]".to_string(),
                line_number: 1,
            },
        ]);

        let zones = markdown_file
            .collect_exclusion_zones("[[test|one|two]] some text [[]]", &validated_config);

        assert_eq!(zones.len(), 2, "Should have two exclusion zones");
        assert!(
            zones.contains(&(0, 16)),
            "Should contain first invalid wikilink span"
        );
        assert!(
            zones.contains(&(27, 31)),
            "Should contain second invalid wikilink span"
        );
    }

    #[test]
    fn test_exclusion_zones_only_matches_current_line() {
        let (_, validated_config, mut obsidian_repository) = test_support::create_test_environment(
            ChangeMode::DryRun,
            None,
            None,
            Some("Line 1 with [[bad|link|here]]\nLine 2 with normal text"),
        );

        let markdown_file = obsidian_repository.markdown_files.first_mut().unwrap();

        markdown_file.wikilinks.invalid.push(InvalidWikilink {
            content:     "[[bad|link|here]]".to_string(),
            reason:      InvalidWikilinkReason::DoubleAlias,
            span:        (10, 26),
            line:        "Line 1 with [[bad|link|here]]".to_string(),
            line_number: 1,
        });

        // `zones` should not contain exclusions for the second line.
        let zones =
            markdown_file.collect_exclusion_zones("Line 2 with normal text", &validated_config);

        assert!(
            zones.is_empty(),
            "Should not have exclusion zones for different line"
        );
    }

    #[test]
    fn test_should_create_match_in_table() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, None, None);
        let file_path = temp_dir.path().join("test.md");

        let markdown_file =
            MarkdownFile::new(file_path, validated_config.operational_timezone()).unwrap();

        assert!(markdown_file.should_create_match("| Test Link | description |", 2, "Test Link",));

        assert!(markdown_file.should_create_match("| Test Link | [[Other]] |", 2, "Test Link",));
    }

    #[test]
    fn test_process_line_table_escaping_combined() {
        let wikilinks = vec![
            Wikilink {
                display_text: "Another Link".to_string(),
                target:       "Other Page".to_string(),
            },
            Wikilink {
                display_text: "Test Link".to_string(),
                target:       "Target Page".to_string(),
            },
        ];

        let (temp_dir, validated_config, obsidian_repository) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(wikilinks), None);

        // Compile the wikilinks
        let sorted_wikilinks = &obsidian_repository.wikilinks_sorted;

        let automaton = test_support::build_aho_corasick(sorted_wikilinks);

        let markdown_file = obsidian_repository.markdown_files.first().unwrap();

        let test_cases = vec![
            (
                "| Test Link | Another Link | description |",
                vec![
                    "[[Target Page\\|Test Link]]",
                    "[[Other Page\\|Another Link]]",
                ],
                "Multiple matches in one row",
            ),
            (
                "| prefix Test Link suffix | Another Link |",
                vec![
                    "[[Target Page\\|Test Link]]",
                    "[[Other Page\\|Another Link]]",
                ],
                "Table cells with surrounding text",
            ),
            (
                "| column1 | Test Link | Another Link |",
                vec![
                    "[[Target Page\\|Test Link]]",
                    "[[Other Page\\|Another Link]]",
                ],
                "Different column positions",
            ),
            (
                "| Test Link | description | Another Link |",
                vec![
                    "[[Target Page\\|Test Link]]",
                    "[[Other Page\\|Another Link]]",
                ],
                "Multiple replacements in different columns",
            ),
        ];

        let wikilink_refs: Vec<&Wikilink> = sorted_wikilinks.iter().collect();
        for (line, expected_replacements, description) in test_cases {
            let _ = TestFileBuilder::new()
                .with_title("test".to_string())
                .with_content(line.to_string())
                .create(&temp_dir, "test.md");

            let matches = markdown_file.process_line_for_back_populate_replacements(
                line,
                0,
                &automaton,
                &wikilink_refs,
                &validated_config,
            );

            assert_eq!(
                matches.len(),
                expected_replacements.len(),
                "Incorrect number of replacements for: {description}"
            );

            for (match_info, expected) in matches.iter().zip(expected_replacements.iter()) {
                assert_eq!(
                    match_info.replacement, *expected,
                    "Incorrect replacement for: {description}"
                );
                assert_eq!(
                    match_info.match_context,
                    MatchContext::MarkdownTable,
                    "Should be marked as in table for: {description}"
                );
            }
        }
    }

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
                // `TestCase::expected_matches` follows the order returned by `process_line`.
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
    fn test_case_sensitivity_behavior() {
        let (temp_dir, validated_config, mut obsidian_repository) =
            test_support::create_test_environment(ChangeMode::DryRun, None, None, None);

        for case in get_case_sensitivity_test_cases() {
            let file_path = test_support::create_markdown_test_file(
                &temp_dir,
                "test.md",
                case.content,
                &mut obsidian_repository,
            );

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

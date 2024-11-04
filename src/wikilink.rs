use crate::constants::*;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;
use std::error::Error;
use std::path::Path;
use crate::wikilink_types::{CompiledWikilink, ExtractedWikilinks, InvalidWikilink, InvalidWikilinkReason, Wikilink,WikilinkParseResult};

lazy_static! {
    pub static ref MARKDOWN_REGEX: Regex = Regex::new(r"\[.*?\]\(.*?\)").unwrap();
}

pub fn is_wikilink(potential_wikilink: Option<&String>) -> bool {
    if let Some(test_wikilink) = potential_wikilink {
        test_wikilink.starts_with(OPENING_WIKILINK) && test_wikilink.ends_with(CLOSING_WIKILINK)
    } else {
        false
    }
}

pub fn create_filename_wikilink(filename: &str) -> Wikilink {
    let display_text = filename.strip_suffix(".md").unwrap_or(filename).to_string();

    Wikilink {
        display_text: display_text.clone(),
        target: display_text,
        is_alias: false,
    }
}

pub fn format_wikilink(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| format!("[[{}]]", s))
        .unwrap_or_else(|| "[[]]".to_string())
}

pub fn collect_all_wikilinks(
    content: &str,
    aliases: &Option<Vec<String>>,
    file_path: &Path,
) -> Result<HashSet<CompiledWikilink>, Box<dyn Error + Send + Sync>> {
    let mut all_wikilinks = HashSet::new();

    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    // Add filename-based wikilink
    let filename_wikilink = create_filename_wikilink(filename);
    all_wikilinks.insert(CompiledWikilink::new(filename_wikilink.clone()));

    // Add aliases if present
    if let Some(alias_list) = aliases {
        for alias in alias_list {
            let wikilink = Wikilink {
                display_text: alias.clone(),
                target: filename_wikilink.target.clone(),
                is_alias: true,
            };
            all_wikilinks.insert(CompiledWikilink::new(wikilink));
        }
    }

    // Process content line by line
    for (line) in content.lines() {
        let extracted = extract_wikilinks_from_content(line);

        for wikilink in extracted.valid {
            all_wikilinks.insert(CompiledWikilink::new(wikilink));
        }
    }

    Ok(all_wikilinks)
}

pub fn extract_wikilinks_from_content(content: &str) -> ExtractedWikilinks {
    let mut result = ExtractedWikilinks::default();
    let mut chars = content.char_indices().peekable();
    let mut in_wikilink = false;

    while let Some((start_idx, ch)) = chars.next() {
        // Handle unmatched closing brackets when not in a wikilink
        if !in_wikilink && ch == ']' && is_next_char(&mut chars, ']') {
            result.invalid.push(InvalidWikilink {
                content: "]]".to_string(),
                reason: InvalidWikilinkReason::UnmatchedClosing,
                span: (start_idx, start_idx + 2),
            });
            continue;
        }

        if ch == '[' && is_next_char(&mut chars, '[') {
            // Check if the previous character was '!' (image link)
            if start_idx > 0 && is_previous_char(content, start_idx, '!') {
                continue; // Skip image links
            }

            // Parse the wikilink
            match parse_wikilink(&mut chars) {
                Some(WikilinkParseResult::Valid(wikilink)) => {
                    result.valid.push(wikilink);
                }
                Some(WikilinkParseResult::Invalid(invalid)) => {
                    result.invalid.push(invalid);
                }
                None => {
                    // Handle unclosed wikilink
                    // Position will be from [[ to end of parsed content
                    result.invalid.push(InvalidWikilink {
                        content: content[start_idx..].to_string(),
                        reason: InvalidWikilinkReason::Malformed,
                        span: (start_idx, content.len()),
                    });
                }
            }
            in_wikilink = false;
        }
    }

    result
}

#[derive(Debug)]
enum WikilinkState {
    Target {
        content: String,
        start_pos: usize,
    },
    Display {
        target: String,
        target_span: (usize, usize),
        content: String,
        start_pos: usize,
    },
}

impl WikilinkState {
    fn push_char(&mut self, c: char) {
        match self {
            WikilinkState::Target { content, .. } => content.push(c),
            WikilinkState::Display { content, .. } => content.push(c),
        }
    }

    fn transition_to_display(self, pipe_pos: usize) -> Self {
        match self {
            WikilinkState::Target { content, start_pos } => WikilinkState::Display {
                target: content,
                target_span: (start_pos, pipe_pos),
                content: String::new(),
                start_pos: pipe_pos + 1,
            },
            display_state => display_state,
        }
    }

    fn to_wikilink(self) -> WikilinkParseResult {
        match self {
            WikilinkState::Target { content, .. } => {
                let trimmed = content.trim().to_string();
                WikilinkParseResult::Valid(Wikilink {
                    display_text: trimmed.clone(),
                    target: trimmed,
                    is_alias: false,
                })
            }
            WikilinkState::Display { target, content, .. } => {
                let trimmed_target = target.trim().to_string();
                let trimmed_display = content.trim().to_string();
                WikilinkParseResult::Valid(Wikilink {
                    display_text: trimmed_display,
                    target: trimmed_target,
                    is_alias: true,
                })
            }
        }
    }
}

fn parse_wikilink(chars: &mut std::iter::Peekable<std::str::CharIndices>) -> Option<WikilinkParseResult> {
    let start_pos = chars.peek()?.0;
    let mut state = WikilinkState::Target {
        content: String::new(),
        start_pos,
    };
    let mut escape = false;

    while let Some((pos, c)) = chars.next() {
        match (escape, c) {
            (true, '|') => {
                state = state.transition_to_display(pos);
                escape = false;
            }
            (true, c) => {
                state.push_char(c);
                escape = false;
            }
            (false, '\\') => escape = true,
            (false, '[') => {
                // Check for nested [[
                if let Some(&(_, next_ch)) = chars.peek() {
                    if next_ch == '[' {
                        chars.next(); // Consume the second [
                        return Some(WikilinkParseResult::Invalid(InvalidWikilink {
                            content: match &state {
                                WikilinkState::Target { content, .. } => content.clone(),
                                WikilinkState::Display { target, content, .. } => {
                                    format!("{}|{}", target, content)
                                }
                            },
                            reason: InvalidWikilinkReason::NestedOpening,
                            span: (start_pos, pos + 2),
                        }));
                    }
                }
                state.push_char(c);
            }
            (false, '|') => state = state.transition_to_display(pos),
            (false, ']') if is_next_char(chars, ']') => return Some(state.to_wikilink()),
            (false, c) => state.push_char(c),
        }
    }

    None
}

/// Helper function to check if the next character matches the expected one
fn is_next_char(chars: &mut std::iter::Peekable<std::str::CharIndices>, expected: char) -> bool {
    if let Some(&(_, next_ch)) = chars.peek() {
        if next_ch == expected {
            chars.next(); // Consume the expected character
            return true;
        }
    }
    false
}

fn is_previous_char(content: &str, index: usize, expected: char) -> bool {
    content[..index].chars().rev().next() == Some(expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // Update helper function to use direct creation
    fn assert_contains_wikilink(
        wikilinks: &HashSet<CompiledWikilink>,
        target: &str,
        display: Option<&str>,
        is_alias: bool,
    ) {
        let exists = wikilinks.iter().any(|w| {
            w.wikilink.target == target
                && w.wikilink.display_text == display.unwrap_or(target)
                && w.wikilink.is_alias == is_alias
        });
        assert!(
            exists,
            "Expected wikilink with target '{}', display '{:?}', is_alias '{}'",
            target, display, is_alias
        );
    }

    // Submodule for collecting wikilinks
    mod collect_wikilinks {
        use super::*;
        use tempfile::TempDir;

        #[test]
        fn collect_all_wikilinks_with_aliases() {
            let content = "# Test\nHere's a [[Regular Link]] and [[Target|Display Text]]";
            let aliases = Some(vec!["Alias One".to_string(), "Alias Two".to_string()]);

            let temp_dir = TempDir::new().unwrap();
            let file_path = temp_dir.path().join("test file.md");
            std::fs::write(&file_path, content).unwrap();

            let wikilinks = collect_all_wikilinks(content, &aliases, &file_path).unwrap();

            // Verify expected wikilinks
            assert_contains_wikilink(&wikilinks, "test file", None, false);
            assert_contains_wikilink(&wikilinks, "test file", Some("Alias One"), true);
            assert_contains_wikilink(&wikilinks, "test file", Some("Alias Two"), true);
            assert_contains_wikilink(&wikilinks, "Regular Link", None, false);
            assert_contains_wikilink(&wikilinks, "Target", Some("Display Text"), true);
        }

        #[test]
        fn collect_wikilinks_with_context() {
            let content = "Some [[Link]] here.";
            let temp_dir = TempDir::new().unwrap();
            let file_path = temp_dir.path().join("file.md");
            std::fs::write(&file_path, content).unwrap();

            let wikilinks = collect_all_wikilinks(content, &None, &file_path).unwrap();

            assert_contains_wikilink(&wikilinks, "Link", None, false);
        }
    }

    mod extract_wikilinks {
        use super::*;

        #[test]
        fn test_unmatched_closing_brackets() {
            let content = "Some text here]] more text";
            let extracted = extract_wikilinks_from_content(content);

            assert!(extracted.valid.is_empty(), "Should not have any valid wikilinks");
            assert_eq!(extracted.invalid.len(), 1, "Should have one invalid wikilink");
            assert_eq!(
                extracted.invalid[0].reason,
                InvalidWikilinkReason::UnmatchedClosing,
                "Should identify unmatched closing brackets"
            );
            assert_eq!(
                extracted.invalid[0].span,
                (14, 16),
                "Should have correct span for unmatched brackets"
            );
        }

        #[test]
        fn test_mixed_valid_and_invalid() {
            let content = "[[Valid Link]] but here]] and [[Another]]";
            let extracted = extract_wikilinks_from_content(content);

            assert_eq!(extracted.valid.len(), 2, "Should have two valid wikilinks");
            assert_eq!(extracted.invalid.len(), 1, "Should have one invalid wikilink");

            // Check the valid wikilinks
            assert!(extracted.valid.iter().any(|w| w.display_text == "Valid Link"));
            assert!(extracted.valid.iter().any(|w| w.display_text == "Another"));

            // Check the invalid wikilink
            assert_eq!(
                extracted.invalid[0].reason,
                InvalidWikilinkReason::UnmatchedClosing
            );
        }

        #[test]
        fn test_unclosed_wikilink() {
            let content = "[[Unclosed wikilink";
            let extracted = extract_wikilinks_from_content(content);

            assert!(extracted.valid.is_empty(), "Should not have any valid wikilinks");
            assert_eq!(extracted.invalid.len(), 1, "Should have one invalid wikilink");
            assert_eq!(
                extracted.invalid[0].reason,
                InvalidWikilinkReason::Malformed,
                "Should identify malformed/unclosed wikilink"
            );
        }
    }

    #[cfg(test)]
    mod wikilink_creation {
        use crate::wikilink_types::ToWikilink;
        use super::*;
        use std::path::Path;

        // Macro to test simple wikilink creation
        macro_rules! test_wikilink {
        ($test_name:ident, $input:expr, $expected:expr) => {
            #[test]
            fn $test_name() {
                let formatted = format_wikilink(&Path::new($input));
                assert_eq!(formatted, $expected);
            }
        };
    }

        // Define simple wikilink tests using the macro
        test_wikilink!(wikilink_simple, "test", "[[test]]");
        test_wikilink!(wikilink_with_md, "test.md", "[[test]]");
        test_wikilink!(wikilink_empty, "", "[[]]");
        test_wikilink!(wikilink_unicode, "テスト.md", "[[テスト]]");
        test_wikilink!(wikilink_with_space, "test café.md", "[[test café]]");

        /// Helper function to parse a full wikilink string.
        /// It ensures the input starts with `[[` and ends with `]]`,
        /// extracts the inner content, and passes it to `parse_wikilink`.
        fn parse_full_wikilink(input: &str) -> Option<WikilinkParseResult> {
            if input.starts_with("[[") && input.ends_with("]]") {
                // Extract the substring after `[[` and include the closing `]]`
                let inner = &input[2..];
                let mut chars = inner.char_indices().peekable();
                parse_wikilink(&mut chars)
            } else {
                // Invalid format if it doesn't start and end with brackets
                None
            }
        }

        /// Asserts that a full wikilink string is parsed correctly as valid.
        fn assert_valid_wikilink(
            input: &str,
            expected_target: &str,
            expected_display: &str,
            expected_is_alias: bool,
        ) {
            let result = parse_full_wikilink(input).expect("Failed to parse wikilink");

            match result {
                WikilinkParseResult::Valid(wikilink) => {
                    assert_eq!(wikilink.target, expected_target, "Target mismatch for input: {}", input);
                    assert_eq!(wikilink.display_text, expected_display, "Display text mismatch for input: {}", input);
                    assert_eq!(wikilink.is_alias, expected_is_alias, "Alias flag mismatch for input: {}", input);
                }
                WikilinkParseResult::Invalid(invalid) => {
                    panic!(
                        "Expected valid wikilink for input: {}, but got invalid: {} ({:?})",
                        input, invalid.content, invalid.reason
                    );
                }
            }
        }

        /// Asserts that a full wikilink string fails to parse as expected.
        fn assert_invalid_wikilink(input: &str, expected_reason: InvalidWikilinkReason) {
            let result = parse_full_wikilink(input);

            match result {
                Some(WikilinkParseResult::Invalid(invalid)) => {
                    assert_eq!(
                        invalid.reason, expected_reason,
                        "Expected reason {:?} but got {:?} for input: {}",
                        expected_reason, invalid.reason, input
                    );
                }
                Some(WikilinkParseResult::Valid(_)) => {
                    panic!("Expected invalid wikilink for input: {}, but got valid.", input);
                }
                None => {
                    panic!("Expected invalid wikilink for input: {}, but got None.", input);
                }
            }
        }

        #[test]
        fn to_aliased_wikilink_variants() {
            let test_cases = vec![
                ("target", "target", "[[target]]"),
                ("Target", "target", "[[Target|target]]"),
                ("test link", "Test Link", "[[test link|Test Link]]"),
                ("Apple", "fruit", "[[Apple|fruit]]"),
                ("Home", "主页", "[[Home|主页]]"),
                ("page.md", "Page", "[[page|Page]]"),
                ("café", "咖啡", "[[café|咖啡]]"),
                ("テスト", "Test", "[[テスト|Test]]"),
            ];

            for (target, display, expected) in test_cases {
                let result = target.to_aliased_wikilink(display);
                assert_eq!(
                    result, expected,
                    "Failed for target '{}', display '{}'",
                    target, display
                );
            }

            // Testing with String type
            let string_target = String::from("Target");
            assert_eq!(
                string_target.to_aliased_wikilink("target"),
                "[[Target|target]]"
            );
            assert_eq!(string_target.to_aliased_wikilink("Target"), "[[Target]]");
        }

        #[test]
        fn test_parse_wikilink_basic_and_aliased() {
            let test_cases = vec![
                // Basic cases
                ("[[test]]", "test", "test", false),
                ("[[simple link]]", "simple link", "simple link", false),
                ("[[  spaced  ]]", "spaced", "spaced", false),
                ("[[测试]]", "测试", "测试", false),
                // Aliased cases
                ("[[target|display]]", "target", "display", true),
                ("[[  target  |  display  ]]", "target", "display", true),
                ("[[测试|test]]", "测试", "test", true),
                ("[[test|测试]]", "test", "测试", true),
                ("[[a/b/c|display]]", "a/b/c", "display", true),
            ];

            for (input, target, display, is_alias) in test_cases {
                assert_valid_wikilink(input, target, display, is_alias);
            }
        }

        #[test]
        fn test_parse_wikilink_escaped_chars() {
            let test_cases = vec![
                // Regular escape in target
                ("[[test\\]text]]", "test]text", "test]text", false),
                // Escaped characters in aliased link
                ("[[target|display\\]text]]", "target", "display]text", true),
                // Multiple escaped characters
                ("[[test\\]with\\[brackets]]", "test]with[brackets", "test]with[brackets", false),
            ];

            for (input, target, display, is_alias) in test_cases {
                assert_valid_wikilink(input, target, display, is_alias);
            }
        }

        #[test]
        fn test_parse_wikilink_special_chars() {
            let test_cases = vec![
                ("[[!@#$%^&*()]]", "!@#$%^&*()", "!@#$%^&*()", false),
                ("[[../path/to/file]]", "../path/to/file", "../path/to/file", false),
                ("[[file (1)]]", "file (1)", "file (1)", false),
                ("[[file (1)|version 1]]", "file (1)", "version 1", true),
                ("[[outer [inner] text]]", "outer [inner] text", "outer [inner] text", false),
                ("[[target|(text)]]", "target", "(text)", true),
            ];

            for (input, target, display, is_alias) in test_cases {
                assert_valid_wikilink(input, target, display, is_alias);
            }
        }

        #[test]
        fn test_nested_opening_brackets() {
            let test_cases = vec![
                // Nested opening brackets should be invalid
                ("[[blah [[]]]", InvalidWikilinkReason::NestedOpening),
                ("[[blah [[blah]]]]", InvalidWikilinkReason::NestedOpening),
                // Additional nested cases
                ("[[outer [[inner]]]]", InvalidWikilinkReason::NestedOpening),
                ("[[level1 [[level2 [[level3]]]]]]", InvalidWikilinkReason::NestedOpening),
            ];

            for (input, expected_reason) in test_cases {
                assert_invalid_wikilink(input, expected_reason);
            }
        }
    }

        // Submodule for Markdown link tests
    mod markdown_links {
        use super::*;

        #[test]
        fn test_markdown_regex_matches() {
            let regex = MARKDOWN_REGEX.clone();

            let matching_cases = vec![
                "[text](https://example.com)",
                "[link](https://test.com)",
                "[page](folder/page.md)",
                "[img](../images/test.png)",
                "[text](path 'title')",
                "[text](path \"title\")",
                "[](path)",
                "[text]()",
                "[]()",
            ];

            for case in matching_cases {
                assert!(regex.is_match(case), "Regex should match '{}'", case);
            }

            let non_matching_cases = vec![
                "plain text",
                "[[wikilink]]",
                "![[imagelink]]",
                "[incomplete",
            ];

            for case in non_matching_cases {
                assert!(!regex.is_match(case), "Regex should not match '{}'", case);
            }
        }

        #[test]
        fn test_markdown_link_extraction() {
            let regex = MARKDOWN_REGEX.clone();
            let text = "Here is [one](link1) and [two](link2) and normal text";

            let links: Vec<_> = regex.find_iter(text).map(|m| m.as_str()).collect();
            assert_eq!(links.len(), 2);
            assert_eq!(links[0], "[one](link1)");
            assert_eq!(links[1], "[two](link2)");
        }
    }
}

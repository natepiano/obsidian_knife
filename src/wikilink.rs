use crate::constants::*;
use crate::wikilink_types::{
    ExtractedWikilinks, InvalidWikilink, InvalidWikilinkReason, ParsedExtractedWikilinks,
    ParsedInvalidWikilink, Wikilink, WikilinkParseResult,
};
use lazy_static::lazy_static;
use regex::Regex;
use std::error::Error;
use std::iter::Peekable;
use std::path::Path;
use std::str::CharIndices;

lazy_static! {
    pub static ref MARKDOWN_REGEX: Regex = Regex::new(r"\[.*?\]\(.*?\)").unwrap();
    static ref EMAIL_REGEX: Regex =
        Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
    static ref TAG_REGEX: Regex = Regex::new(r"(?:^|\s)#[a-zA-Z0-9_-]+").unwrap();
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

pub fn collect_file_wikilinks(
    content: &str,
    aliases: &Option<Vec<String>>,
    file_path: &Path,
) -> Result<ExtractedWikilinks, Box<dyn Error + Send + Sync>> {
    let mut result = ExtractedWikilinks::default();

    // Add filename-based wikilink
    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    let filename_wikilink = create_filename_wikilink(filename);
    result.valid.push(filename_wikilink.clone());

    // Add aliases if present
    if let Some(alias_list) = aliases {
        for alias in alias_list {
            let wikilink = Wikilink {
                display_text: alias.clone(),
                target: filename_wikilink.target.clone(),
                is_alias: true,
            };
            result.valid.push(wikilink);
        }
    }

    // Process content line by line and collect both valid and invalid wikilinks
    for (line_idx, line) in content.lines().enumerate() {
        let extracted = extract_wikilinks(line);
        result.valid.extend(extracted.valid);

        // Convert ParsedInvalidWikilink to InvalidWikilink with line information
        let invalid_with_lines: Vec<InvalidWikilink> = extracted
            .invalid
            .into_iter()
            .map(|parsed| parsed.into_invalid_wikilink(line.to_string(), line_idx + 1))
            .collect();

        result.invalid.extend(invalid_with_lines);
    }

    Ok(result)
}

fn extract_wikilinks(line: &str) -> ParsedExtractedWikilinks {
    let mut result = ParsedExtractedWikilinks::default();

    parse_special_patterns(line, &mut result);

    let mut chars = line.char_indices().peekable();
    let mut markdown_opening: Option<usize> = None;
    let mut last_position: usize = 0;

    while let Some((start_idx, ch)) = chars.next() {
        // Handle escaped characters
        if ch == '\\' {
            chars.next(); // Skip next character
            continue;
        }

        // Handle unmatched closing brackets when not in a wikilink
        if ch == ']' && is_next_char(&mut chars, ']') {
            let content = line[last_position..=start_idx + 1].to_string();
            result.invalid.push(ParsedInvalidWikilink {
                content,
                reason: InvalidWikilinkReason::UnmatchedClosing,
                span: (last_position, start_idx + 2),
            });
            markdown_opening = None;
            last_position = start_idx + 2;
            continue;
        }

        // Handle regular closing bracket - could close a markdown link
        if ch == ']' {
            markdown_opening = None;
        }

        if ch == '[' {
            if is_next_char(&mut chars, '[') {
                // If we had an unclosed markdown link before this wikilink, add it as invalid
                if let Some(start_pos) = markdown_opening {
                    let content_slice = line[start_pos..start_idx].trim();
                    result.invalid.push(ParsedInvalidWikilink {
                        content: content_slice.to_string(),
                        reason: InvalidWikilinkReason::UnmatchedMarkdownOpening,
                        span: (start_pos, start_pos + content_slice.len()),
                    });
                    markdown_opening = None;
                }

                // Check if this is an image reference
                let is_image = start_idx > 0 && is_previous_char(line, start_idx, '!');

                // Still parse the wikilink normally
                if let Some(wikilink_result) = parse_wikilink(&mut chars) {
                    match wikilink_result {
                        WikilinkParseResult::Valid(wikilink) => {
                            // Only add non-image wikilinks to the result
                            if !is_image {
                                result.valid.push(wikilink);
                            }
                            if let Some((pos, _)) = chars.peek() {
                                last_position = *pos;
                            }
                        }
                        WikilinkParseResult::Invalid(invalid) => {
                            result.invalid.push(invalid);
                        }
                    }
                }
            } else {
                // Handle markdown link opening as before...
                if let Some(start_pos) = markdown_opening {
                    let content_slice = line[start_pos..start_idx].trim();
                    result.invalid.push(ParsedInvalidWikilink {
                        content: content_slice.to_string(),
                        reason: InvalidWikilinkReason::UnmatchedMarkdownOpening,
                        span: (start_pos, start_pos + content_slice.len()),
                    });
                }
                markdown_opening = Some(start_idx);
            }
        }
    }

    // Handle unclosed markdown link at end of line
    if let Some(start_pos) = markdown_opening {
        let content_slice = line[start_pos..].trim();
        result.invalid.push(ParsedInvalidWikilink {
            content: content_slice.to_string(),
            reason: InvalidWikilinkReason::UnmatchedMarkdownOpening,
            span: (start_pos, start_pos + content_slice.len()),
        });
    }

    result
}

// Replace parse_email_addresses with this more generic function
fn parse_special_patterns(line: &str, result: &mut ParsedExtractedWikilinks) {
    // Add email addresses as invalid wikilinks
    for email_match in EMAIL_REGEX.find_iter(line) {
        result.invalid.push(ParsedInvalidWikilink {
            content: email_match.as_str().to_string(),
            reason: InvalidWikilinkReason::EmailAddress,
            span: (email_match.start(), email_match.end()),
        });
    }

    // Add tags as invalid wikilinks
    for tag_match in TAG_REGEX.find_iter(line) {
        let tag = tag_match.as_str().trim();
        result.invalid.push(ParsedInvalidWikilink {
            content: tag.to_string(),
            reason: InvalidWikilinkReason::Tag,
            span: (
                tag_match.start() + tag_match.as_str().find(tag).unwrap_or(0),
                tag_match.start() + tag_match.as_str().find(tag).unwrap_or(0) + tag.len(),
            ),
        });
    }
}

#[derive(Debug)]
enum WikilinkState {
    Target {
        content: String,
        start_pos: usize,
    },
    Display {
        target: String,
        _target_span: (usize, usize),
        content: String,
        _start_pos: usize,
    },
    Invalid {
        reason: InvalidWikilinkReason,
        content: String,
        start_pos: usize,
    },
}

impl WikilinkState {
    fn formatted_content(&self) -> String {
        match self {
            WikilinkState::Target { content, .. } => content.to_string(),
            WikilinkState::Display {
                target, content, ..
            } => format!("{}|{}", target, content),
            WikilinkState::Invalid { content, .. } => content.to_string(),
        }
    }

    fn push_char(&mut self, c: char) {
        match self {
            WikilinkState::Target { content, .. } => content.push(c),
            WikilinkState::Display { content, .. } => content.push(c),
            WikilinkState::Invalid { content, .. } => content.push(c),
        }
    }

    fn transition_to_display(&mut self, pipe_pos: usize) {
        if let WikilinkState::Target { content, start_pos } = self {
            *self = WikilinkState::Display {
                target: content.clone(),
                _target_span: (*start_pos, pipe_pos),
                content: String::new(),
                _start_pos: pipe_pos + 1,
            };
        }
    }

    fn transition_to_invalid(&mut self, reason: InvalidWikilinkReason) {
        let content = self.formatted_content();
        let start_pos = match self {
            WikilinkState::Target { start_pos, .. } => *start_pos,
            WikilinkState::Display {
                _target_span: (start, _),
                ..
            } => *start,
            WikilinkState::Invalid { start_pos, .. } => *start_pos,
        };
        *self = WikilinkState::Invalid {
            content,
            reason,
            start_pos,
        };
    }

    fn to_wikilink(self, end_pos: usize) -> WikilinkParseResult {
        match self {
            WikilinkState::Target { content, start_pos } => {
                let trimmed = content.trim().to_string();
                if trimmed.is_empty() {
                    WikilinkParseResult::Invalid(ParsedInvalidWikilink {
                        content: "[[]]".to_string(),
                        reason: InvalidWikilinkReason::EmptyWikilink,
                        span: (start_pos.checked_sub(2).unwrap_or(0), end_pos),
                    })
                } else {
                    WikilinkParseResult::Valid(Wikilink {
                        display_text: trimmed.clone(),
                        target: trimmed,
                        is_alias: false,
                    })
                }
            }
            WikilinkState::Display {
                target,
                content,
                _target_span: (start_pos, _),
                ..
            } => {
                let trimmed_target = target.trim().to_string();
                let trimmed_display = content.trim().to_string();
                if trimmed_target.is_empty() || trimmed_display.is_empty() {
                    WikilinkParseResult::Invalid(ParsedInvalidWikilink {
                        content: format!("[[{}|{}]]", target, content),
                        reason: InvalidWikilinkReason::EmptyWikilink,
                        span: (start_pos.checked_sub(2).unwrap_or(0), end_pos),
                    })
                } else {
                    WikilinkParseResult::Valid(Wikilink {
                        display_text: trimmed_display,
                        target: trimmed_target,
                        is_alias: true,
                    })
                }
            }
            WikilinkState::Invalid {
                content,
                reason,
                start_pos,
            } => {
                let formatted = match reason {
                    InvalidWikilinkReason::UnmatchedOpening => format!("[[{}", content),
                    _ => format!("[[{}]]", content),
                };
                WikilinkParseResult::Invalid(ParsedInvalidWikilink {
                    content: formatted,
                    reason,
                    span: (start_pos, end_pos),
                })
            }
        }
    }
}

fn parse_wikilink(chars: &mut Peekable<CharIndices>) -> Option<WikilinkParseResult> {
    let initial_pos = chars.peek()?.0;
    let start_pos = initial_pos.saturating_sub(2);

    let mut state = WikilinkState::Target {
        content: String::new(),
        start_pos,
    };

    while let Some((pos, c)) = chars.next() {
        if matches!(state, WikilinkState::Invalid { .. }) {
            if c == ']' && is_next_char(chars, ']') {
                return Some(state.to_wikilink(pos + 2));
            }
            state.push_char(c);
            continue;
        }

        match c {
            '\\' => {
                // Handle escaped characters
                if let Some((_, next_c)) = chars.next() {
                    if next_c == '|' {
                        // Treat escaped pipe same as regular pipe
                        match state {
                            WikilinkState::Target { .. } => state.transition_to_display(pos),
                            WikilinkState::Display { .. } => {
                                state.transition_to_invalid(InvalidWikilinkReason::DoubleAlias);
                                state.push_char('\\');
                                state.push_char('|');
                            }
                            _ => unreachable!(),
                        }
                    } else {
                        state.push_char(next_c);
                    }
                }
            }
            '|' => match state {
                WikilinkState::Target { .. } => state.transition_to_display(pos),
                WikilinkState::Display { .. } => {
                    state.transition_to_invalid(InvalidWikilinkReason::DoubleAlias);
                    state.push_char(c);
                }
                _ => unreachable!(),
            },
            ']' => {
                if is_next_char(chars, ']') {
                    return Some(state.to_wikilink(pos + 2));
                } else {
                    state.transition_to_invalid(InvalidWikilinkReason::UnmatchedSingleInWikilink);
                    state.push_char(c);
                }
            }
            '[' => {
                if is_next_char(chars, '[') {
                    state.transition_to_invalid(InvalidWikilinkReason::NestedOpening);
                    state.push_char(c); // push first '['
                    state.push_char('['); // push second '['
                } else {
                    state.transition_to_invalid(InvalidWikilinkReason::UnmatchedSingleInWikilink);
                    state.push_char(c);
                }
            }
            _ => state.push_char(c),
        }
    }

    state.transition_to_invalid(InvalidWikilinkReason::UnmatchedOpening);
    let content_len = state.formatted_content().len();
    Some(state.to_wikilink(start_pos + content_len + 2))
}

/// Helper function to check if the next character matches the expected one
fn is_next_char(chars: &mut Peekable<CharIndices>, expected: char) -> bool {
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
mod extract_wikilinks_tests {
    use super::*;

    struct WikilinkTestCase {
        description: &'static str,
        input: &'static str,
        expected_valid: Vec<(&'static str, &'static str, bool)>, // (target, display, is_alias)
        expected_invalid: Vec<(&'static str, InvalidWikilinkReason, (usize, usize))>, // (content, reason, span)
    }

    fn assert_wikilink_extraction(test_case: WikilinkTestCase) {
        let extracted = extract_wikilinks(test_case.input);

        // Verify valid wikilinks
        assert_eq!(
            extracted.valid.len(),
            test_case.expected_valid.len(),
            "Mismatch in number of valid wikilinks for: {}",
            test_case.description
        );

        for ((target, display, is_alias), wikilink) in
            test_case.expected_valid.iter().zip(extracted.valid.iter())
        {
            assert_eq!(
                wikilink.target, *target,
                "Target mismatch in {}",
                test_case.description
            );
            assert_eq!(
                wikilink.display_text, *display,
                "Display text mismatch in {}",
                test_case.description
            );
            assert_eq!(
                wikilink.is_alias, *is_alias,
                "Alias flag mismatch in {}",
                test_case.description
            );
        }

        // Verify invalid wikilinks
        assert_eq!(
            extracted.invalid.len(),
            test_case.expected_invalid.len(),
            "Mismatch in number of invalid wikilinks for: {}",
            test_case.description
        );

        for ((content, reason, span), invalid) in test_case
            .expected_invalid
            .iter()
            .zip(extracted.invalid.iter())
        {
            assert_eq!(
                invalid.content, *content,
                "Content mismatch in {}",
                test_case.description
            );
            assert_eq!(
                invalid.reason, *reason,
                "Reason mismatch in {}",
                test_case.description
            );
            assert_eq!(
                invalid.span, *span,
                "Span mismatch in {}",
                test_case.description
            );
        }
    }
    #[test]
    fn test_various_extractions() {
        let test_cases = vec![
            WikilinkTestCase {
                description: "Double alias with closing brackets",
                input: "Text with [[target|alias|extra]] here",
                expected_valid: vec![],
                expected_invalid: vec![(
                    "[[target|alias|extra]]",
                    InvalidWikilinkReason::DoubleAlias,
                    (10, 32),
                )],
            },
            WikilinkTestCase {
                description: "Double alias without closing",
                input: "Text with [[target|alias|extra",
                expected_valid: vec![],
                expected_invalid: vec![(
                    "[[target|alias|extra",
                    InvalidWikilinkReason::UnmatchedOpening,
                    (10, 30),
                )],
            },
            WikilinkTestCase {
                description: "Unmatched closing bracket within wikilink",
                input: "Text with [[test]text]] here",
                expected_valid: vec![],
                expected_invalid: vec![(
                    "[[test]text]]",
                    InvalidWikilinkReason::UnmatchedSingleInWikilink,
                    (10, 23),
                )],
            },
            WikilinkTestCase {
                description: "Unmatched opening bracket within wikilink",
                input: "Text with [[test[text]] here",
                expected_valid: vec![],
                expected_invalid: vec![(
                    "[[test[text]]",
                    InvalidWikilinkReason::UnmatchedSingleInWikilink,
                    (10, 23),
                )],
            },
            WikilinkTestCase {
                description: "Nested wikilink",
                input: "Text with [[target[[inner]] here",
                expected_valid: vec![],
                expected_invalid: vec![(
                    "[[target[[inner]]",
                    InvalidWikilinkReason::NestedOpening,
                    (10, 27),
                )],
            },
        ];

        for test_case in test_cases {
            assert_wikilink_extraction(test_case);
        }
    }

    #[test]
    fn test_unmatched_brackets() {
        let test_cases = vec![
            WikilinkTestCase {
                description: "Single unmatched closing brackets",
                input: "Some text here]] more text",
                expected_valid: vec![],
                expected_invalid: vec![(
                    "Some text here]]",
                    InvalidWikilinkReason::UnmatchedClosing,
                    (0, 16),
                )],
            },
            WikilinkTestCase {
                description: "Multiple unmatched closings",
                input: "Text]] more]] text",
                expected_valid: vec![],
                expected_invalid: vec![
                    ("Text]]", InvalidWikilinkReason::UnmatchedClosing, (0, 6)),
                    (" more]]", InvalidWikilinkReason::UnmatchedClosing, (6, 13)),
                ],
            },
            WikilinkTestCase {
                description: "Mixed valid and invalid brackets",
                input: "[[Valid Link]] but here]] and [[Another]]",
                expected_valid: vec![
                    ("Valid Link", "Valid Link", false),
                    ("Another", "Another", false),
                ],
                expected_invalid: vec![(
                    " but here]]",
                    InvalidWikilinkReason::UnmatchedClosing,
                    (14, 25),
                )],
            },
            // New Test Case for Unmatched Opening
            WikilinkTestCase {
                description: "Unmatched opening brackets at the end",
                input: "Here is an [[unmatched link",
                expected_valid: vec![],
                expected_invalid: vec![(
                    "[[unmatched link",
                    InvalidWikilinkReason::UnmatchedOpening,
                    (11, 27),
                )],
            },
            WikilinkTestCase {
                description: "No wikilinks",
                input: "This is a plain text without any wikilinks.",
                expected_valid: vec![],
                expected_invalid: vec![],
            },
        ];

        for test_case in test_cases {
            assert_wikilink_extraction(test_case);
        }
    }

    #[test]
    fn test_unclosed_markdown_links() {
        let test_cases = vec![
            WikilinkTestCase {
                description: "Basic unclosed markdown link",
                input: "[display",
                expected_valid: vec![],
                expected_invalid: vec![(
                    "[display",
                    InvalidWikilinkReason::UnmatchedMarkdownOpening,
                    (0, 8),
                )],
            },
            WikilinkTestCase {
                description: "Unclosed link in context",
                input: "some text [link",
                expected_valid: vec![],
                expected_invalid: vec![(
                    "[link",
                    InvalidWikilinkReason::UnmatchedMarkdownOpening,
                    (10, 15),
                )],
            },
            WikilinkTestCase {
                description: "Mixed valid wikilink and unclosed markdown",
                input: "[[valid link]] [unclosed",
                expected_valid: vec![("valid link", "valid link", false)],
                expected_invalid: vec![(
                    "[unclosed",
                    InvalidWikilinkReason::UnmatchedMarkdownOpening,
                    (15, 24),
                )],
            },
            WikilinkTestCase {
                description: "Multiple unclosed markdown links",
                input: "[first [second",
                expected_valid: vec![],
                expected_invalid: vec![
                    (
                        "[first",
                        InvalidWikilinkReason::UnmatchedMarkdownOpening,
                        (0, 6),
                    ),
                    (
                        "[second",
                        InvalidWikilinkReason::UnmatchedMarkdownOpening,
                        (7, 14),
                    ),
                ],
            },
            WikilinkTestCase {
                description: "Escaped brackets should not trigger",
                input: "\\[not a link",
                expected_valid: vec![],
                expected_invalid: vec![],
            },
            WikilinkTestCase {
                description: "Valid markdown link followed by unclosed",
                input: "[valid](link) [unclosed",
                expected_valid: vec![],
                expected_invalid: vec![
                    (
                        "[unclosed",
                        InvalidWikilinkReason::UnmatchedMarkdownOpening,
                        (14, 23),
                    ), // Fixed span
                ],
            },
        ];

        for test_case in test_cases {
            assert_wikilink_extraction(test_case);
        }
    }

    #[test]
    fn test_email_detection() {
        let test_cases = vec![
            WikilinkTestCase {
                description: "Simple email address",
                input: "Contact bob@example.com for more info",
                expected_valid: vec![],
                expected_invalid: vec![(
                    "bob@example.com",
                    InvalidWikilinkReason::EmailAddress,
                    (8, 23),
                )],
            },
            WikilinkTestCase {
                description: "Email with wikilink",
                input: "[[Contact]] john.doe@company.org today",
                expected_valid: vec![("Contact", "Contact", false)],
                expected_invalid: vec![(
                    "john.doe@company.org",
                    InvalidWikilinkReason::EmailAddress,
                    (12, 32),
                )],
            },
            WikilinkTestCase {
                description: "Multiple emails",
                input: "Email user1@test.com or user2@test.com",
                expected_valid: vec![],
                expected_invalid: vec![
                    (
                        "user1@test.com",
                        InvalidWikilinkReason::EmailAddress,
                        (6, 20),
                    ),
                    (
                        "user2@test.com",
                        InvalidWikilinkReason::EmailAddress,
                        (24, 38),
                    ),
                ],
            },
        ];

        for test_case in test_cases {
            assert_wikilink_extraction(test_case);
        }
    }

    #[test]
    fn test_tag_detection() {
        let test_cases = vec![
            WikilinkTestCase {
                description: "Simple tag at start of line",
                input: "#obsidian_knife is great",
                expected_valid: vec![],
                expected_invalid: vec![("#obsidian_knife", InvalidWikilinkReason::Tag, (0, 15))],
            },
            WikilinkTestCase {
                description: "Tag after space",
                input: "Check out this #ka-fave tag",
                expected_valid: vec![],
                expected_invalid: vec![("#ka-fave", InvalidWikilinkReason::Tag, (15, 23))],
            },
            WikilinkTestCase {
                description: "Multiple tags",
                input: "#tag1 some text #tag2",
                expected_valid: vec![],
                expected_invalid: vec![
                    ("#tag1", InvalidWikilinkReason::Tag, (0, 5)),
                    ("#tag2", InvalidWikilinkReason::Tag, (16, 21)),
                ],
            },
            WikilinkTestCase {
                description: "Tag with wikilink",
                input: "[[Note]] #important reference",
                expected_valid: vec![("Note", "Note", false)],
                expected_invalid: vec![("#important", InvalidWikilinkReason::Tag, (9, 19))],
            },
            WikilinkTestCase {
                description: "Tag with underscore and numbers",
                input: "Task #todo_123 pending",
                expected_valid: vec![],
                expected_invalid: vec![("#todo_123", InvalidWikilinkReason::Tag, (5, 14))],
            },
        ];

        for test_case in test_cases {
            assert_wikilink_extraction(test_case);
        }
    }
}

#[cfg(test)]
mod wikilink_creation_tests {
    use super::*;
    use crate::wikilink_types::ToWikilink;
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
                assert_eq!(
                    wikilink.target, expected_target,
                    "Target mismatch for input: {}",
                    input
                );
                assert_eq!(
                    wikilink.display_text, expected_display,
                    "Display text mismatch for input: {}",
                    input
                );
                assert_eq!(
                    wikilink.is_alias, expected_is_alias,
                    "Alias flag mismatch for input: {}",
                    input
                );
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

        // println!("{:?}", result);

        match result {
            Some(WikilinkParseResult::Invalid(invalid)) => {
                assert_eq!(
                    invalid.reason, expected_reason,
                    "Expected reason {:?} but got {:?} for input: {}",
                    expected_reason, invalid.reason, input
                );
            }
            Some(WikilinkParseResult::Valid(_)) => {
                panic!(
                    "Expected invalid wikilink for input: {}, but got valid.",
                    input
                );
            }
            None => {
                panic!(
                    "Expected invalid wikilink for input: {}, but got None.",
                    input
                );
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
    fn test_empty_wikilink_variants() {
        let test_cases = vec![
            ("[[]]", InvalidWikilinkReason::EmptyWikilink),
            ("[[|]]", InvalidWikilinkReason::EmptyWikilink),
            ("[[display|]]", InvalidWikilinkReason::EmptyWikilink),
            ("[[|alias]]", InvalidWikilinkReason::EmptyWikilink),
            ("[[display\\|]]", InvalidWikilinkReason::EmptyWikilink),
        ];

        for (input, expected_reason) in test_cases {
            assert_invalid_wikilink(input, expected_reason);
        }
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
    fn test_parse_wikilink_double_alias() {
        // Invalid cases
        let invalid_cases = vec![
            // Basic double alias case
            (
                "[[target|alias|second]]",
                InvalidWikilinkReason::DoubleAlias,
            ),
            // Multiple pipes
            (
                "[[target|alias|second|third]]",
                InvalidWikilinkReason::DoubleAlias,
            ),
            // Consecutive pipes
            ("[[target||alias]]", InvalidWikilinkReason::DoubleAlias),
            // Mixed consecutive and separated pipes
            (
                "[[target||alias|another]]",
                InvalidWikilinkReason::DoubleAlias,
            ),
            // Complex case with double pipe
            (
                "[[target|display|another]]",
                InvalidWikilinkReason::DoubleAlias,
            ),
            // Multiple consecutive pipes
            ("[[target|||display]]", InvalidWikilinkReason::DoubleAlias),
            // Escaped pipe is still a pipe
            (
                "[[target\\|text|DoubleAlias]]",
                InvalidWikilinkReason::DoubleAlias,
            ),
        ];

        for (input, expected_reason) in invalid_cases {
            assert_invalid_wikilink(input, expected_reason);
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
            (
                "[[test\\]with\\[brackets]]",
                "test]with[brackets",
                "test]with[brackets",
                false,
            ),
            // Escaped single brackets
            (
                "[[text\\[in\\]brackets]]",
                "text[in]brackets",
                "text[in]brackets",
                false,
            ),
            (
                "[[target\\[x\\]|display\\[y\\]]]",
                "target[x]",
                "display[y]",
                true,
            ),
        ];

        for (input, target, display, is_alias) in test_cases {
            assert_valid_wikilink(input, target, display, is_alias);
        }
    }

    #[test]
    fn test_parse_wikilink_unmatched_brackets() {
        let test_cases = vec![
            // Basic unmatched brackets
            (
                "[[text]text]]",
                InvalidWikilinkReason::UnmatchedSingleInWikilink,
            ),
            (
                "[[text[text]]",
                InvalidWikilinkReason::UnmatchedSingleInWikilink,
            ),
            // Mixed escape scenarios - only flag when a bracket is actually unmatched
            (
                "[[text[\\]text]]",
                InvalidWikilinkReason::UnmatchedSingleInWikilink,
            ), // first [ is unmatched, second is escaped
            (
                "[[text\\[]text]]",
                InvalidWikilinkReason::UnmatchedSingleInWikilink,
            ), // ] is unmatched, [ is escaped
            // Complex cases with aliases
            (
                "[[target[x|display]]",
                InvalidWikilinkReason::UnmatchedSingleInWikilink,
            ),
            (
                "[[target|display]x]]",
                InvalidWikilinkReason::UnmatchedSingleInWikilink,
            ),
        ];

        for (input, expected_reason) in test_cases {
            assert_invalid_wikilink(input, expected_reason);
        }
    }

    #[test]
    fn test_parse_wikilink_special_chars() {
        let test_cases = vec![
            ("[[!@#$%^&*()]]", "!@#$%^&*()", "!@#$%^&*()", false),
            (
                "[[../path/to/file]]",
                "../path/to/file",
                "../path/to/file",
                false,
            ),
            ("[[file (1)]]", "file (1)", "file (1)", false),
            ("[[file (1)|version 1]]", "file (1)", "version 1", true),
            ("[[target|(text)]]", "target", "(text)", true),
        ];

        for (input, target, display, is_alias) in test_cases {
            assert_valid_wikilink(input, target, display, is_alias);
        }
    }
}

#[cfg(test)]
mod collect_wikilinks_tests {
    use super::*;
    use tempfile::TempDir;

    fn assert_contains_wikilink(
        wikilinks: &[Wikilink],
        target: &str,
        display: Option<&str>,
        is_alias: bool,
    ) {
        let exists = wikilinks.iter().any(|w| {
            w.target == target
                && w.display_text == display.unwrap_or(target)
                && w.is_alias == is_alias
        });
        assert!(
            exists,
            "Expected wikilink with target '{}', display '{:?}', is_alias '{}'",
            target, display, is_alias
        );
    }

    #[test]
    fn test_collect_file_wikilinks_with_aliases() {
        let content = "# Test\nHere's a [[Regular Link]] and [[Target|Display Text]]";
        let aliases = Some(vec!["Alias One".to_string(), "Alias Two".to_string()]);

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test file.md");
        std::fs::write(&file_path, content).unwrap();

        let extracted = collect_file_wikilinks(content, &aliases, &file_path).unwrap();

        // Verify expected wikilinks
        assert_contains_wikilink(&extracted.valid, "test file", None, false);
        assert_contains_wikilink(&extracted.valid, "test file", Some("Alias One"), true);
        assert_contains_wikilink(&extracted.valid, "test file", Some("Alias Two"), true);
        assert_contains_wikilink(&extracted.valid, "Regular Link", None, false);
        assert_contains_wikilink(&extracted.valid, "Target", Some("Display Text"), true);

        // Verify no invalid wikilinks in this case
        assert!(
            extracted.invalid.is_empty(),
            "Should not have invalid wikilinks"
        );
    }

    #[test]
    fn test_collect_file_wikilinks_with_invalid() {
        let content = "Some [[good link]] and [[bad|link|extra]] here\n[[unmatched";
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        std::fs::write(&file_path, content).unwrap();

        let extracted = collect_file_wikilinks(content, &None, &file_path).unwrap();

        // Check valid wikilinks
        assert_contains_wikilink(&extracted.valid, "test", None, false); // filename
        assert_contains_wikilink(&extracted.valid, "good link", None, false);

        // Verify invalid wikilinks with line information
        assert_eq!(
            extracted.invalid.len(),
            2,
            "Should have exactly two invalid wikilinks"
        );

        // Find and verify the double alias invalid wikilink
        let double_alias = extracted
            .invalid
            .iter()
            .find(|w| w.reason == InvalidWikilinkReason::DoubleAlias)
            .expect("Should have a double alias invalid wikilink");

        assert_eq!(double_alias.line_number, 1);
        assert_eq!(
            double_alias.line,
            "Some [[good link]] and [[bad|link|extra]] here"
        );
        assert_eq!(double_alias.content, "[[bad|link|extra]]");

        // Find and verify the unmatched opening invalid wikilink
        let unmatched = extracted
            .invalid
            .iter()
            .find(|w| w.reason == InvalidWikilinkReason::UnmatchedOpening)
            .expect("Should have an unmatched opening invalid wikilink");

        assert_eq!(unmatched.line_number, 2);
        assert_eq!(unmatched.line, "[[unmatched");
        assert_eq!(unmatched.content, "[[unmatched");
    }

    #[test]
    fn test_collect_wikilinks_with_empty() {
        let content = "Test [[]] here\nAnd [[|]] there";
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        std::fs::write(&file_path, content).unwrap();

        let extracted = collect_file_wikilinks(content, &None, &file_path).unwrap();

        assert_eq!(
            extracted.invalid.len(),
            2,
            "Should have two invalid empty wikilinks"
        );

        // Verify first empty wikilink
        let first_empty = &extracted.invalid[0];
        assert_eq!(first_empty.line_number, 1);
        assert_eq!(first_empty.line, "Test [[]] here");
        assert_eq!(first_empty.content, "[[]]");
        assert_eq!(first_empty.reason, InvalidWikilinkReason::EmptyWikilink);

        // Verify second empty wikilink
        let second_empty = &extracted.invalid[1];
        assert_eq!(second_empty.line_number, 2);
        assert_eq!(second_empty.line, "And [[|]] there");
        assert_eq!(second_empty.content, "[[|]]");
        assert_eq!(second_empty.reason, InvalidWikilinkReason::EmptyWikilink);
    }
}

#[cfg(test)]
mod markdown_links_tests {
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

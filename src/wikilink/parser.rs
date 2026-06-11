use std::iter::Peekable;
use std::str::CharIndices;
use std::sync::LazyLock;

use regex::Regex;

use super::constants::EMPTY_WIKILINK;
use super::constants::MARKDOWN_CLICKABLE_IMAGE_PREFIX;
use super::constants::WIKILINK_FINDER_PATTERN;
use super::link::InvalidWikilink;
use super::link::InvalidWikilinkReason;
use super::link::Wikilink;
use crate::constants::BACKSLASH;
use crate::constants::CLOSING_BRACKET;
use crate::constants::CLOSING_WIKILINK;
use crate::constants::ESCAPED_PIPE;
use crate::constants::IMAGE_EMBED_MARKER;
use crate::constants::MARKDOWN_SUFFIX;
use crate::constants::OPENING_BRACKET;
use crate::constants::OPENING_WIKILINK;
use crate::constants::PIPE;
use crate::markdown_file::InlineCodeExcluder;
use crate::support;
use crate::support::EMAIL_REGEX;
use crate::support::RAW_HTTP_REGEX;
use crate::support::TAG_REGEX;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum WikilinkParseResult {
    Valid(Wikilink),
    Invalid(ParsedInvalidWikilink),
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParsedInvalidWikilink {
    pub content: String,
    pub reason:  InvalidWikilinkReason,
    pub span:    (usize, usize),
}

#[derive(Debug, Default)]
pub struct ParsedExtractedWikilinks {
    pub valid:   Vec<Wikilink>,
    pub invalid: Vec<ParsedInvalidWikilink>,
}

pub fn is_wikilink(potential_wikilink: Option<&str>) -> bool {
    potential_wikilink.is_some_and(|test_wikilink| {
        test_wikilink.starts_with(OPENING_WIKILINK) && test_wikilink.ends_with(CLOSING_WIKILINK)
    })
}

pub fn create_filename_wikilink(filename: &str) -> Wikilink {
    let display_text = filename
        .strip_suffix(MARKDOWN_SUFFIX)
        .unwrap_or(filename)
        .to_string();

    Wikilink {
        display_text: display_text.clone(),
        target:       display_text,
    }
}

pub fn extract_wikilinks(line: &str) -> ParsedExtractedWikilinks {
    let mut extracted_wikilinks = ParsedExtractedWikilinks::default();
    let mut inline_code_excluder = InlineCodeExcluder::new();

    parse_special_patterns(line, &mut extracted_wikilinks);

    let mut chars = line.char_indices().peekable();
    let mut markdown_opening: Option<usize> = None;
    let mut last_position: usize = 0;

    while let Some((start_idx, ch)) = chars.next() {
        // InlineCodeExcluder suppresses wikilink parsing inside inline code.
        inline_code_excluder.update(ch);

        if inline_code_excluder.is_in_code_block() {
            continue;
        }

        // BACKSLASH consumes the next char before wikilink parsing.
        if ch == BACKSLASH {
            chars.next();
            continue;
        }

        // InvalidWikilinkReason::UnmatchedClosing records stray closing brackets.
        if ch == CLOSING_BRACKET && is_next_char(&mut chars, CLOSING_BRACKET) {
            let content = line[last_position..(start_idx + CLOSING_WIKILINK.len())].to_string();
            extracted_wikilinks.invalid.push(ParsedInvalidWikilink {
                content,
                reason: InvalidWikilinkReason::UnmatchedClosing,
                span: (last_position, start_idx + CLOSING_WIKILINK.len()),
            });
            markdown_opening = None;
            last_position = start_idx + CLOSING_WIKILINK.len();
            continue;
        }

        // A single CLOSING_BRACKET clears markdown-link tracking.
        if ch == CLOSING_BRACKET {
            markdown_opening = None;
        }

        if ch == OPENING_BRACKET {
            if is_next_char(&mut chars, OPENING_BRACKET) {
                // `markdown_opening` becomes a `ParsedInvalidWikilink` before the wikilink.
                if let Some(start_position) = markdown_opening {
                    let content_slice = line[start_position..start_idx].trim();
                    extracted_wikilinks.invalid.push(ParsedInvalidWikilink {
                        content: content_slice.to_string(),
                        reason:  InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
                        span:    (start_position, start_position + content_slice.len()),
                    });
                    markdown_opening = None;
                }

                // `IMAGE_EMBED_MARKER` sets the `is_image` flag.
                let is_image =
                    start_idx > 0 && is_previous_char(line, start_idx, IMAGE_EMBED_MARKER);

                // Still parse the wikilink normally
                if let Some(wikilink_result) = parse_wikilink(&mut chars) {
                    match wikilink_result {
                        WikilinkParseResult::Valid(wikilink) => {
                            // Non-image wikilinks are stored in `extracted_wikilinks.valid`.
                            if !is_image {
                                extracted_wikilinks.valid.push(wikilink);
                            }
                            if let Some((position, _)) = chars.peek() {
                                last_position = *position;
                            }
                        },
                        WikilinkParseResult::Invalid(invalid) => {
                            extracted_wikilinks.invalid.push(invalid);
                        },
                    }
                }
            } else {
                // `markdown_opening` records a possible markdown link opening bracket.
                if let Some(start_position) = markdown_opening {
                    let between = &line[start_position..start_idx];
                    // `[!` between brackets is a `[![` markdown clickable image, not invalid
                    if between != MARKDOWN_CLICKABLE_IMAGE_PREFIX {
                        let content_slice = between.trim();
                        extracted_wikilinks.invalid.push(ParsedInvalidWikilink {
                            content: content_slice.to_string(),
                            reason:  InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
                            span:    (start_position, start_position + content_slice.len()),
                        });
                    }
                }
                markdown_opening = Some(start_idx);
            }
        }
    }

    // `markdown_opening` reports `InvalidWikilinkReason::UnmatchedMarkdownLinkOpening`.
    if let Some(start_position) = markdown_opening {
        let content_slice = line[start_position..].trim();
        extracted_wikilinks.invalid.push(ParsedInvalidWikilink {
            content: content_slice.to_string(),
            reason:  InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
            span:    (start_position, start_position + content_slice.len()),
        });
    }

    // `InlineCodeExcluder::is_inside` reports `InvalidWikilinkReason::UnclosedInlineCode`.
    if inline_code_excluder.is_inside() {
        extracted_wikilinks.invalid.push(ParsedInvalidWikilink {
            content: line[last_position..].to_string(),
            reason:  InvalidWikilinkReason::UnclosedInlineCode,
            span:    (last_position, line.len()),
        });
    }

    extracted_wikilinks
}

impl ParsedInvalidWikilink {
    pub fn into_invalid_wikilink(self, line: String, line_number: usize) -> InvalidWikilink {
        InvalidWikilink {
            content: self.content,
            reason: self.reason,
            span: self.span,
            line,
            line_number,
        }
    }
}

fn parse_special_patterns(line: &str, result: &mut ParsedExtractedWikilinks) {
    // EMAIL_REGEX marks email addresses as InvalidWikilinkReason::EmailAddress.
    let reason = InvalidWikilinkReason::EmailAddress;
    let regex = &EMAIL_REGEX;

    add_special_patterns(line, result, reason, regex);

    let reason = InvalidWikilinkReason::RawHttpLink;
    let regex = &RAW_HTTP_REGEX;
    add_special_patterns(line, result, reason, regex);

    // TAG_REGEX marks tags as InvalidWikilinkReason::Tag.
    let reason = InvalidWikilinkReason::Tag;
    let regex = &TAG_REGEX;
    add_special_patterns(line, result, reason, regex);
}

fn add_special_patterns(
    line: &str,
    result: &mut ParsedExtractedWikilinks,
    reason: InvalidWikilinkReason,
    regex: &Regex,
) {
    for regex_match in regex.find_iter(line) {
        result.invalid.push(ParsedInvalidWikilink {
            content: regex_match.as_str().trim().to_string(),
            reason,
            span: (regex_match.start(), regex_match.end()),
        });
    }
}

#[derive(Debug)]
enum WikilinkState {
    Target {
        content:        String,
        start_position: usize,
    },
    Display {
        target:      String,
        target_span: (usize, usize),
        content:     String,
    },
    Invalid {
        reason:         InvalidWikilinkReason,
        content:        String,
        start_position: usize,
    },
}

impl WikilinkState {
    fn formatted_content(&self) -> String {
        match self {
            Self::Target { content, .. } | Self::Invalid { content, .. } => content.clone(),
            Self::Display {
                target, content, ..
            } => format!("{target}{PIPE}{content}"),
        }
    }

    fn push_char(&mut self, c: char) {
        match self {
            Self::Target { content, .. }
            | Self::Display { content, .. }
            | Self::Invalid { content, .. } => content.push(c),
        }
    }

    fn transition_to_display(&mut self, pipe_position: usize) {
        if let Self::Target {
            content,
            start_position,
        } = self
        {
            *self = Self::Display {
                target:      content.clone(),
                target_span: (*start_position, pipe_position),
                content:     String::new(),
            };
        }
    }

    fn transition_to_invalid(&mut self, reason: InvalidWikilinkReason) {
        let content = self.formatted_content();
        let start_position = match self {
            Self::Target { start_position, .. } | Self::Invalid { start_position, .. } => {
                *start_position
            },
            Self::Display {
                target_span: (start, _),
                ..
            } => *start,
        };
        *self = Self::Invalid {
            content,
            reason,
            start_position,
        };
    }

    fn to_wikilink(&self, end_position: usize) -> WikilinkParseResult {
        match self {
            Self::Target {
                content,
                start_position,
            } => {
                let trimmed = content.trim().to_string();
                if trimmed.is_empty() {
                    WikilinkParseResult::Invalid(ParsedInvalidWikilink {
                        content: EMPTY_WIKILINK.to_string(),
                        reason:  InvalidWikilinkReason::Empty,
                        span:    (
                            start_position.saturating_sub(OPENING_WIKILINK.len()),
                            end_position,
                        ),
                    })
                } else {
                    WikilinkParseResult::Valid(Wikilink {
                        display_text: trimmed.clone(),
                        target:       trimmed,
                    })
                }
            },
            Self::Display {
                target,
                content,
                target_span: (start_position, _),
                ..
            } => {
                let trimmed_target = target.trim().to_string();
                let trimmed_display = content.trim().to_string();
                if trimmed_target.is_empty() || trimmed_display.is_empty() {
                    WikilinkParseResult::Invalid(ParsedInvalidWikilink {
                        content: format!(
                            "{OPENING_WIKILINK}{target}{PIPE}{content}{CLOSING_WIKILINK}"
                        ),
                        reason:  InvalidWikilinkReason::Empty,
                        span:    (
                            start_position.saturating_sub(OPENING_WIKILINK.len()),
                            end_position,
                        ),
                    })
                } else {
                    WikilinkParseResult::Valid(Wikilink {
                        display_text: trimmed_display,
                        target:       trimmed_target,
                    })
                }
            },
            Self::Invalid {
                content,
                reason,
                start_position,
            } => {
                let formatted = match reason {
                    InvalidWikilinkReason::UnmatchedOpening => {
                        format!("{OPENING_WIKILINK}{content}")
                    },
                    _ => format!("{OPENING_WIKILINK}{content}{CLOSING_WIKILINK}"),
                };
                WikilinkParseResult::Invalid(ParsedInvalidWikilink {
                    content: formatted,
                    reason:  *reason,
                    span:    (*start_position, end_position),
                })
            },
        }
    }
}

pub(super) fn parse_wikilink(chars: &mut Peekable<CharIndices>) -> Option<WikilinkParseResult> {
    let initial_position = chars.peek()?.0;
    let start_position = initial_position.saturating_sub(OPENING_WIKILINK.len());

    let mut wikilink_state = WikilinkState::Target {
        content: String::new(),
        start_position,
    };

    while let Some((position, c)) = chars.next() {
        if matches!(wikilink_state, WikilinkState::Invalid { .. }) {
            if c == CLOSING_BRACKET && is_next_char(chars, CLOSING_BRACKET) {
                return Some(wikilink_state.to_wikilink(position + CLOSING_WIKILINK.len()));
            }
            wikilink_state.push_char(c);
            continue;
        }

        match c {
            BACKSLASH => {
                // BACKSLASH consumes escaped wikilink chars before pipe handling.
                if let Some((_, next_c)) = chars.next() {
                    if next_c == PIPE {
                        // Escaped PIPE remains a wikilink separator.
                        match wikilink_state {
                            WikilinkState::Target { .. } => {
                                wikilink_state.transition_to_display(position);
                            },
                            WikilinkState::Display { .. } => {
                                wikilink_state
                                    .transition_to_invalid(InvalidWikilinkReason::DoubleAlias);
                                for ch in ESCAPED_PIPE.chars() {
                                    wikilink_state.push_char(ch);
                                }
                            },
                            WikilinkState::Invalid { .. } => {}, // already invalid, nothing to do
                        }
                    } else {
                        wikilink_state.push_char(next_c);
                    }
                }
            },
            PIPE => match wikilink_state {
                WikilinkState::Target { .. } => {
                    wikilink_state.transition_to_display(position);
                },
                WikilinkState::Display { .. } => {
                    wikilink_state.transition_to_invalid(InvalidWikilinkReason::DoubleAlias);
                    wikilink_state.push_char(c);
                },
                WikilinkState::Invalid { .. } => {}, // already invalid, nothing to do
            },
            CLOSING_BRACKET => {
                if is_next_char(chars, CLOSING_BRACKET) {
                    return Some(wikilink_state.to_wikilink(position + CLOSING_WIKILINK.len()));
                }
                wikilink_state.transition_to_invalid(InvalidWikilinkReason::UnmatchedSingle);
                wikilink_state.push_char(c);
            },
            OPENING_BRACKET => {
                if is_next_char(chars, OPENING_BRACKET) {
                    wikilink_state.transition_to_invalid(InvalidWikilinkReason::NestedOpening);
                    wikilink_state.push_char(c); // push first '['
                    wikilink_state.push_char(OPENING_BRACKET); // push second '['
                } else {
                    wikilink_state.transition_to_invalid(InvalidWikilinkReason::UnmatchedSingle);
                    wikilink_state.push_char(c);
                }
            },
            _ => wikilink_state.push_char(c),
        }
    }

    wikilink_state.transition_to_invalid(InvalidWikilinkReason::UnmatchedOpening);
    let content_len = wikilink_state.formatted_content().len();
    Some(wikilink_state.to_wikilink(start_position + content_len + CLOSING_WIKILINK.len()))
}

/// Returns whether `chars.peek()` matches `expected`.
fn is_next_char(chars: &mut Peekable<CharIndices>, expected: char) -> bool {
    if let Some(&(_, next_ch)) = chars.peek()
        && next_ch == expected
    {
        chars.next(); // Consume `expected`.
        return true;
    }
    false
}

fn is_previous_char(content: &str, index: usize, expected: char) -> bool {
    if index == 0 {
        return false; // No previous character if index is 0
    }

    content[..index].ends_with(expected)
}

pub fn is_within_wikilink(line: &str, byte_position: usize) -> bool {
    static WIKILINK_FINDER: LazyLock<Regex> =
        LazyLock::new(|| support::compile_regex(WIKILINK_FINDER_PATTERN));

    for mat in WIKILINK_FINDER.find_iter(line) {
        let content_start = mat.start() + OPENING_WIKILINK.len();
        let content_end = mat.end() - CLOSING_WIKILINK.len();

        // `byte_position` must be inside the wikilink content, excluding brackets.
        if byte_position >= content_start && byte_position < content_end {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::AliasExpectation;
    use crate::wikilink::InvalidWikilinkReason;

    struct WikilinkTestCase {
        description: &'static str,
        input:       &'static str,
        valid:       Vec<(&'static str, &'static str, AliasExpectation)>,
        invalid:     Vec<(&'static str, InvalidWikilinkReason, (usize, usize))>,
    }

    fn assert_wikilink_extraction(test_case: &WikilinkTestCase) {
        let extracted = extract_wikilinks(test_case.input);

        // Verify valid wikilinks
        assert_eq!(
            extracted.valid.len(),
            test_case.valid.len(),
            "Mismatch in number of valid wikilinks for: {}",
            test_case.description
        );

        for ((target, display, alias_expectation), wikilink) in
            test_case.valid.iter().zip(extracted.valid.iter())
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
                wikilink.is_alias(),
                alias_expectation.is_alias(),
                "Alias flag mismatch in {}",
                test_case.description
            );
        }

        // Verify invalid wikilinks
        assert_eq!(
            extracted.invalid.len(),
            test_case.invalid.len(),
            "Mismatch in number of invalid wikilinks for: {}",
            test_case.description
        );

        for ((content, reason, span), invalid) in
            test_case.invalid.iter().zip(extracted.invalid.iter())
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
                input:       "Text with [[target|alias|extra]] here",
                valid:       vec![],
                invalid:     vec![(
                    "[[target|alias|extra]]",
                    InvalidWikilinkReason::DoubleAlias,
                    (10, 32),
                )],
            },
            WikilinkTestCase {
                description: "Double alias without closing",
                input:       "Text with [[target|alias|extra",
                valid:       vec![],
                invalid:     vec![(
                    "[[target|alias|extra",
                    InvalidWikilinkReason::UnmatchedOpening,
                    (10, 30),
                )],
            },
            WikilinkTestCase {
                description: "Unmatched closing bracket within wikilink",
                input:       "Text with [[test]text]] here",
                valid:       vec![],
                invalid:     vec![(
                    "[[test]text]]",
                    InvalidWikilinkReason::UnmatchedSingle,
                    (10, 23),
                )],
            },
            WikilinkTestCase {
                description: "Unmatched opening bracket within wikilink",
                input:       "Text with [[test[text]] here",
                valid:       vec![],
                invalid:     vec![(
                    "[[test[text]]",
                    InvalidWikilinkReason::UnmatchedSingle,
                    (10, 23),
                )],
            },
            WikilinkTestCase {
                description: "Nested wikilink",
                input:       "Text with [[target[[inner]] here",
                valid:       vec![],
                invalid:     vec![(
                    "[[target[[inner]]",
                    InvalidWikilinkReason::NestedOpening,
                    (10, 27),
                )],
            },
            WikilinkTestCase {
                description: "Aliased wikilink",
                input:       "Text with [[target|alias]] here",
                valid:       vec![("target", "alias", AliasExpectation::Aliased)],
                invalid:     vec![],
            },
        ];

        for test_case in test_cases {
            assert_wikilink_extraction(&test_case);
        }
    }

    #[test]
    fn test_unmatched_brackets() {
        let test_cases = vec![
            WikilinkTestCase {
                description: "Single unmatched closing brackets",
                input:       "Some text here]] more text",
                valid:       vec![],
                invalid:     vec![(
                    "Some text here]]",
                    InvalidWikilinkReason::UnmatchedClosing,
                    (0, 16),
                )],
            },
            WikilinkTestCase {
                description: "Multiple unmatched closings",
                input:       "Text]] more]] text",
                valid:       vec![],
                invalid:     vec![
                    ("Text]]", InvalidWikilinkReason::UnmatchedClosing, (0, 6)),
                    (" more]]", InvalidWikilinkReason::UnmatchedClosing, (6, 13)),
                ],
            },
            WikilinkTestCase {
                description: "Mixed valid and invalid brackets",
                input:       "[[Valid Link]] but here]] and [[Another]]",
                valid:       vec![
                    ("Valid Link", "Valid Link", AliasExpectation::DirectLink),
                    ("Another", "Another", AliasExpectation::DirectLink),
                ],
                invalid:     vec![(
                    " but here]]",
                    InvalidWikilinkReason::UnmatchedClosing,
                    (14, 25),
                )],
            },
            // New Test Case for Unmatched Opening
            WikilinkTestCase {
                description: "Unmatched opening brackets at the end",
                input:       "Here is an [[unmatched link",
                valid:       vec![],
                invalid:     vec![(
                    "[[unmatched link",
                    InvalidWikilinkReason::UnmatchedOpening,
                    (11, 27),
                )],
            },
            WikilinkTestCase {
                description: "No wikilinks",
                input:       "This is a plain text without any wikilinks.",
                valid:       vec![],
                invalid:     vec![],
            },
        ];

        for test_case in test_cases {
            assert_wikilink_extraction(&test_case);
        }
    }

    #[test]
    fn test_unclosed_markdown_links() {
        let test_cases = vec![
            WikilinkTestCase {
                description: "Basic unclosed markdown link",
                input:       "[display",
                valid:       vec![],
                invalid:     vec![(
                    "[display",
                    InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
                    (0, 8),
                )],
            },
            WikilinkTestCase {
                description: "Unclosed link in context",
                input:       "some text [link",
                valid:       vec![],
                invalid:     vec![(
                    "[link",
                    InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
                    (10, 15),
                )],
            },
            WikilinkTestCase {
                description: "Mixed valid wikilink and unclosed markdown",
                input:       "[[valid link]] [unclosed",
                valid:       vec![("valid link", "valid link", AliasExpectation::DirectLink)],
                invalid:     vec![(
                    "[unclosed",
                    InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
                    (15, 24),
                )],
            },
            WikilinkTestCase {
                description: "Multiple unclosed markdown links",
                input:       "[first [second",
                valid:       vec![],
                invalid:     vec![
                    (
                        "[first",
                        InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
                        (0, 6),
                    ),
                    (
                        "[second",
                        InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
                        (7, 14),
                    ),
                ],
            },
            WikilinkTestCase {
                description: "Escaped brackets should not trigger",
                input:       "\\[not a link",
                valid:       vec![],
                invalid:     vec![],
            },
            WikilinkTestCase {
                description: "Valid markdown link followed by unclosed",
                input:       "[valid](link) [unclosed",
                valid:       vec![],
                invalid:     vec![
                    (
                        "[unclosed",
                        InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
                        (14, 23),
                    ), // Fixed span
                ],
            },
        ];

        for test_case in test_cases {
            assert_wikilink_extraction(&test_case);
        }
    }

    #[test]
    fn test_markdown_clickable_image_not_flagged() {
        // CI badge: [![alt](image-url)](link-url) is valid markdown, not an invalid wikilink
        let test_cases = vec![
            WikilinkTestCase {
                description: "CI badge should not produce UnmatchedMarkdownLinkOpening",
                input:       "[![CI](https://example.com/badge.svg)](https://example.com/ci)",
                valid:       vec![],
                invalid:     vec![(
                    "https://example.com/badge.svg)](https://example.com/ci)",
                    InvalidWikilinkReason::RawHttpLink,
                    (7, 62),
                )],
            },
            WikilinkTestCase {
                description: "Badge with surrounding text",
                input:       "Check out [![Build](https://example.com/b.svg)](https://example.com) the project",
                valid:       vec![],
                invalid:     vec![(
                    "https://example.com/b.svg)](https://example.com)",
                    InvalidWikilinkReason::RawHttpLink,
                    (20, 68),
                )],
            },
        ];

        for test_case in test_cases {
            assert_wikilink_extraction(&test_case);
        }
    }

    #[test]
    fn test_email_detection() {
        let test_cases = vec![
            WikilinkTestCase {
                description: "Simple email address",
                input:       "Contact bob@example.com for more info",
                valid:       vec![],
                invalid:     vec![(
                    "bob@example.com",
                    InvalidWikilinkReason::EmailAddress,
                    (8, 23),
                )],
            },
            WikilinkTestCase {
                description: "Email with wikilink",
                input:       "[[Contact]] john.doe@company.org today",
                valid:       vec![("Contact", "Contact", AliasExpectation::DirectLink)],
                invalid:     vec![(
                    "john.doe@company.org",
                    InvalidWikilinkReason::EmailAddress,
                    (12, 32),
                )],
            },
            WikilinkTestCase {
                description: "Multiple emails",
                input:       "Email user1@test.com or user2@test.com",
                valid:       vec![],
                invalid:     vec![
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
            assert_wikilink_extraction(&test_case);
        }
    }

    #[test]
    fn test_tag_detection() {
        let test_cases = vec![
            WikilinkTestCase {
                description: "Simple tag at start of line",
                input:       "#obsidian_knife is great",
                valid:       vec![],
                invalid:     vec![("#obsidian_knife", InvalidWikilinkReason::Tag, (0, 15))],
            },
            WikilinkTestCase {
                description: "Tag after space",
                input:       "Check out this #ka-fave tag",
                valid:       vec![],
                invalid:     vec![("#ka-fave", InvalidWikilinkReason::Tag, (14, 23))],
            },
            WikilinkTestCase {
                description: "Multiple tags",
                input:       "#tag1 some text #tag2",
                valid:       vec![],
                invalid:     vec![
                    ("#tag1", InvalidWikilinkReason::Tag, (0, 5)),
                    ("#tag2", InvalidWikilinkReason::Tag, (15, 21)),
                ],
            },
            WikilinkTestCase {
                description: "Tag with wikilink",
                input:       "[[Note]] #important reference",
                valid:       vec![("Note", "Note", AliasExpectation::DirectLink)],
                invalid:     vec![("#important", InvalidWikilinkReason::Tag, (8, 19))],
            },
            WikilinkTestCase {
                description: "Tag with underscore and numbers",
                input:       "Task #two_do_123 pending",
                valid:       vec![],
                invalid:     vec![("#two_do_123", InvalidWikilinkReason::Tag, (4, 16))],
            },
        ];

        for test_case in test_cases {
            assert_wikilink_extraction(&test_case);
        }
    }

    #[test]
    fn test_raw_http_detection() {
        let test_cases = vec![
            WikilinkTestCase {
                description: "link at start of line",
                input:       "https://google.com/ is blah",
                valid:       vec![],
                invalid:     vec![(
                    "https://google.com/",
                    InvalidWikilinkReason::RawHttpLink,
                    (0, 19),
                )],
            },
            WikilinkTestCase {
                description: "link after space",
                input:       "Check out this https://google.com/ link",
                valid:       vec![],
                invalid:     vec![(
                    "https://google.com/",
                    InvalidWikilinkReason::RawHttpLink,
                    (15, 34),
                )],
            },
            WikilinkTestCase {
                description: "Multiple links",
                input:       "http://this.com/ some text http://that.com/",
                valid:       vec![],
                invalid:     vec![
                    (
                        "http://this.com/",
                        InvalidWikilinkReason::RawHttpLink,
                        (0, 16),
                    ),
                    (
                        "http://that.com/",
                        InvalidWikilinkReason::RawHttpLink,
                        (27, 43),
                    ),
                ],
            },
        ];

        for test_case in test_cases {
            assert_wikilink_extraction(&test_case);
        }
    }
}

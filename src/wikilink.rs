#[cfg(test)]
mod extract_wikilink_tests;
#[cfg(test)]
mod markdown_link_tests;
#[cfg(test)]
mod wikilink_creation_tests;

mod wikilink_types;
pub use wikilink_types::*;

use crate::{
    constants::*,
    utils::{EMAIL_REGEX, TAG_REGEX},
};

use crate::utils::RAW_HTTP_REGEX;
use lazy_static::lazy_static;
use regex::Regex;
use std::iter::Peekable;
use std::str::CharIndices;

pub fn is_wikilink(potential_wikilink: Option<&str>) -> bool {
    if let Some(test_wikilink) = potential_wikilink {
        test_wikilink.starts_with(OPENING_WIKILINK) && test_wikilink.ends_with(CLOSING_WIKILINK)
    } else {
        false
    }
}

pub fn create_filename_wikilink(filename: &str) -> Wikilink {
    let display_text = filename
        .strip_suffix(MARKDOWN_SUFFIX)
        .unwrap_or(filename)
        .to_string();

    Wikilink {
        display_text: display_text.clone(),
        target: display_text,
        is_alias: false,
    }
}

pub fn extract_wikilinks(line: &str) -> ParsedExtractedWikilinks {
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

fn parse_special_patterns(line: &str, result: &mut ParsedExtractedWikilinks) {
    // Add email addresses as invalid wikilinks
    let reason = InvalidWikilinkReason::EmailAddress;
    let regex = &EMAIL_REGEX;

    add_special_patterns(line, result, reason, regex);

    let reason = InvalidWikilinkReason::RawHttpLink;
    let regex = &RAW_HTTP_REGEX;
    add_special_patterns(line, result, reason, regex);

    // Add tags as invalid wikilinks
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

    fn to_wikilink(&self, end_pos: usize) -> WikilinkParseResult {
        match self {
            WikilinkState::Target { content, start_pos } => {
                let trimmed = content.trim().to_string();
                if trimmed.is_empty() {
                    WikilinkParseResult::Invalid(ParsedInvalidWikilink {
                        content: "[[]]".to_string(),
                        reason: InvalidWikilinkReason::EmptyWikilink,
                        // span: (start_pos.checked_sub(2).unwrap_or(0), end_pos),
                        span: (start_pos.saturating_sub(2), end_pos),
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
                        span: (start_pos.saturating_sub(2), end_pos),
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
                    reason: *reason,
                    span: (*start_pos, end_pos),
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
    if index == 0 {
        return false; // No previous character if index is 0
    }

    match content[..index].chars().next_back() {
        Some(c) => c == expected,
        None => false,
    }
}

pub(crate) fn is_within_wikilink(line: &str, byte_position: usize) -> bool {
    lazy_static! {
        static ref WIKILINK_FINDER: Regex = Regex::new(r"\[\[.*?\]\]").unwrap();
    }

    for mat in WIKILINK_FINDER.find_iter(line) {
        let content_start = mat.start() + 2; // Start of link content, after "[["
        let content_end = mat.end() - 2; // End of link content, before "\]\]"

        // Return true only if the byte_position falls within the link content
        if byte_position >= content_start && byte_position < content_end {
            return true;
        }
    }
    false
}

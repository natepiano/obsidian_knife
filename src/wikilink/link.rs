use std::cmp::Ordering;
use std::cmp::PartialEq;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;

use serde::Deserialize;
use serde::Serialize;

use super::constants::INVALID_WIKILINK_DOUBLE_ALIAS;
use super::constants::INVALID_WIKILINK_EMAIL_ADDRESS;
use super::constants::INVALID_WIKILINK_EMPTY;
use super::constants::INVALID_WIKILINK_NESTED_OPENING;
use super::constants::INVALID_WIKILINK_PREFIX;
use super::constants::INVALID_WIKILINK_RAW_HTTP_LINK;
use super::constants::INVALID_WIKILINK_TAG;
use super::constants::INVALID_WIKILINK_UNCLOSED_INLINE_CODE;
use super::constants::INVALID_WIKILINK_UNMATCHED_CLOSING;
use super::constants::INVALID_WIKILINK_UNMATCHED_MARKDOWN_LINK_OPENING;
use super::constants::INVALID_WIKILINK_UNMATCHED_OPENING;
use super::constants::INVALID_WIKILINK_UNMATCHED_SINGLE;
use crate::constants::CLOSING_WIKILINK;
use crate::constants::MARKDOWN_SUFFIX;
use crate::constants::OPENING_WIKILINK;
use crate::constants::PIPE;

/// Trait to convert strings to wikilink format
pub trait ToWikilink {
    /// Converts the string to a wikilink format by surrounding it with [[]]
    fn to_wikilink(&self) -> String;

    /// Creates an aliased wikilink using the target (`self`) and display text
    /// If the texts match (case-sensitive), returns a simple wikilink
    /// Otherwise returns an aliased wikilink in the format [[target|display]]
    fn to_aliased_wikilink(&self, display_text: &str) -> String
    where
        Self: AsRef<str>,
    {
        let target_without_md = strip_md_extension(self.as_ref());

        if target_without_md == display_text {
            target_without_md.to_wikilink()
        } else {
            format!("{OPENING_WIKILINK}{target_without_md}{PIPE}{display_text}{CLOSING_WIKILINK}")
        }
    }
}

impl ToWikilink for str {
    fn to_wikilink(&self) -> String {
        format!(
            "{OPENING_WIKILINK}{}{CLOSING_WIKILINK}",
            strip_md_extension(self)
        )
    }
}

impl ToWikilink for String {
    fn to_wikilink(&self) -> String { self.as_str().to_wikilink() }
}

/// Helper function to strip .md extension if present
fn strip_md_extension(text: &str) -> &str { text.strip_suffix(MARKDOWN_SUFFIX).unwrap_or(text) }

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Wikilink {
    pub display_text: String,
    pub target:       String,
}

impl Wikilink {
    pub fn is_alias(&self) -> bool { self.display_text != self.target }
}

impl PartialOrd for Wikilink {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

impl Ord for Wikilink {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .display_text
            .len()
            .cmp(&self.display_text.len())
            .then(self.display_text.cmp(&other.display_text))
            .then_with(|| match (self.is_alias(), other.is_alias()) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => self.target.cmp(&other.target),
            })
    }
}

impl Display for Wikilink {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.is_alias() {
            write!(f, "{}{PIPE}{}", self.target, self.display_text)
        } else {
            f.write_str(&self.target)
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InvalidWikilinkReason {
    DoubleAlias,                  // e.g. [[A|B|C]]
    Empty,                        // [[]] or [[|]]
    EmailAddress,                 // bob@rock.com
    NestedOpening,                // [[blah [[blah]]
    RawHttpLink,                  // http://somelink.com/
    Tag,                          // #tags should be ignored
    UnclosedInlineCode,           // ` without closing `
    UnmatchedClosing,             // ]] without matching [[
    UnmatchedMarkdownLinkOpening, // [ without following ]
    UnmatchedOpening,             // [[ without closing ]]
    UnmatchedSingle,              // ] without [ or [ without ]
}

impl Display for InvalidWikilinkReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DoubleAlias => f.write_str(INVALID_WIKILINK_DOUBLE_ALIAS),
            Self::EmailAddress => f.write_str(INVALID_WIKILINK_EMAIL_ADDRESS),
            Self::Empty => f.write_str(INVALID_WIKILINK_EMPTY),
            Self::NestedOpening => f.write_str(INVALID_WIKILINK_NESTED_OPENING),
            Self::RawHttpLink => f.write_str(INVALID_WIKILINK_RAW_HTTP_LINK),
            Self::Tag => f.write_str(INVALID_WIKILINK_TAG),
            Self::UnclosedInlineCode => f.write_str(INVALID_WIKILINK_UNCLOSED_INLINE_CODE),
            Self::UnmatchedClosing => f.write_str(INVALID_WIKILINK_UNMATCHED_CLOSING),
            Self::UnmatchedMarkdownLinkOpening => {
                f.write_str(INVALID_WIKILINK_UNMATCHED_MARKDOWN_LINK_OPENING)
            },
            Self::UnmatchedOpening => f.write_str(INVALID_WIKILINK_UNMATCHED_OPENING),
            Self::UnmatchedSingle => f.write_str(INVALID_WIKILINK_UNMATCHED_SINGLE),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidWikilink {
    pub content:     String, // The actual problematic wikilink text
    pub reason:      InvalidWikilinkReason,
    pub span:        (usize, usize), // Start and end positions in the original text
    pub line:        String,         // The full line containing the invalid wikilink
    pub line_number: usize,          // The line number where the invalid wikilink appears
}

impl Display for InvalidWikilink {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{INVALID_WIKILINK_PREFIX} {}, position {}-{}: '{}' {}",
            self.line_number, self.span.0, self.span.1, self.content, self.reason
        )
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use super::super::*;
    use crate::support::MARKDOWN_REGEX;
    use crate::test_support::AliasExpectation;
    use crate::wikilink::ToWikilink;
    use crate::wikilink::parser;
    use crate::wikilink::parser::WikilinkParseResult;

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
            assert!(regex.is_match(case), "Regex should match '{case}'");
        }

        let non_matching_cases = vec![
            "plain text",
            "[[wikilink]]",
            "![[imagelink]]",
            "[incomplete",
        ];

        for case in non_matching_cases {
            assert!(!regex.is_match(case), "Regex should not match '{case}'");
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

    /// Helper function to parse a full wikilink string.
    /// It ensures the input starts with `[[` and ends with `]]`,
    /// extracts the inner content, and passes it to `parse_wikilink`.
    fn parse_full_wikilink(input: &str) -> Option<WikilinkParseResult> {
        if input.starts_with("[[") && input.ends_with("]]") {
            // Extract the substring after `[[` and include the closing `]]`
            let inner = &input[2..];
            let mut chars = inner.char_indices().peekable();
            parser::parse_wikilink(&mut chars)
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
        alias_expectation: AliasExpectation,
    ) {
        let result = parse_full_wikilink(input).expect("Failed to parse wikilink");

        match result {
            WikilinkParseResult::Valid(wikilink) => {
                assert_eq!(
                    wikilink.target, expected_target,
                    "Target mismatch for input: {input}"
                );
                assert_eq!(
                    wikilink.display_text, expected_display,
                    "Display text mismatch for input: {input}"
                );
                assert_eq!(
                    wikilink.is_alias(),
                    alias_expectation.is_alias(),
                    "Alias flag mismatch for input: {input}"
                );
            },
            WikilinkParseResult::Invalid(invalid) => {
                panic!(
                    "Expected valid wikilink for input: {}, but got invalid: {} ({:?})",
                    input, invalid.content, invalid.reason
                );
            },
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
            },
            Some(WikilinkParseResult::Valid(_)) => {
                panic!("Expected invalid wikilink for input: {input}, but got valid.");
            },
            None => {
                panic!("Expected invalid wikilink for input: {input}, but got None.");
            },
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
                "Failed for target '{target}', display '{display}'"
            );
        }

        // Testing with `String` type
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
            ("[[]]", InvalidWikilinkReason::Empty),
            ("[[|]]", InvalidWikilinkReason::Empty),
            ("[[display|]]", InvalidWikilinkReason::Empty),
            ("[[|alias]]", InvalidWikilinkReason::Empty),
            ("[[display\\|]]", InvalidWikilinkReason::Empty),
        ];

        for (input, expected_reason) in test_cases {
            assert_invalid_wikilink(input, expected_reason);
        }
    }

    #[test]
    fn test_parse_wikilink_basic_and_aliased() {
        let test_cases = vec![
            // Basic cases
            ("[[test]]", "test", "test", AliasExpectation::DirectLink),
            (
                "[[simple link]]",
                "simple link",
                "simple link",
                AliasExpectation::DirectLink,
            ),
            (
                "[[  spaced  ]]",
                "spaced",
                "spaced",
                AliasExpectation::DirectLink,
            ),
            ("[[测试]]", "测试", "测试", AliasExpectation::DirectLink),
            // Aliased cases
            (
                "[[target|display]]",
                "target",
                "display",
                AliasExpectation::Aliased,
            ),
            (
                "[[  target  |  display  ]]",
                "target",
                "display",
                AliasExpectation::Aliased,
            ),
            ("[[测试|test]]", "测试", "test", AliasExpectation::Aliased),
            ("[[test|测试]]", "test", "测试", AliasExpectation::Aliased),
            (
                "[[a/b/c|display]]",
                "a/b/c",
                "display",
                AliasExpectation::Aliased,
            ),
        ];

        for (input, target, display, alias_expectation) in test_cases {
            assert_valid_wikilink(input, target, display, alias_expectation);
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
            (
                "[[test\\]text]]",
                "test]text",
                "test]text",
                AliasExpectation::DirectLink,
            ),
            // Escaped characters in aliased link
            (
                "[[target|display\\]text]]",
                "target",
                "display]text",
                AliasExpectation::Aliased,
            ),
            // Multiple escaped characters
            (
                "[[test\\]with\\[brackets]]",
                "test]with[brackets",
                "test]with[brackets",
                AliasExpectation::DirectLink,
            ),
            // Escaped single brackets
            (
                "[[text\\[in\\]brackets]]",
                "text[in]brackets",
                "text[in]brackets",
                AliasExpectation::DirectLink,
            ),
            (
                "[[target\\[x\\]|display\\[y\\]]]",
                "target[x]",
                "display[y]",
                AliasExpectation::Aliased,
            ),
        ];

        for (input, target, display, alias_expectation) in test_cases {
            assert_valid_wikilink(input, target, display, alias_expectation);
        }
    }

    #[test]
    fn test_parse_wikilink_unmatched_brackets() {
        let test_cases = vec![
            // Basic unmatched brackets
            ("[[text]text]]", InvalidWikilinkReason::UnmatchedSingle),
            ("[[text[text]]", InvalidWikilinkReason::UnmatchedSingle),
            // Mixed escape scenarios - only flag when a bracket is actually unmatched
            ("[[text[\\]text]]", InvalidWikilinkReason::UnmatchedSingle), /* first [ is
                                                                           * unmatched,
                                                                           * second is escaped */
            ("[[text\\[]text]]", InvalidWikilinkReason::UnmatchedSingle), /* ] is unmatched,
                                                                           * [ is
                                                                           * escaped */
            // Complex cases with aliases
            (
                "[[target[x|display]]",
                InvalidWikilinkReason::UnmatchedSingle,
            ),
            (
                "[[target|display]x]]",
                InvalidWikilinkReason::UnmatchedSingle,
            ),
        ];

        for (input, expected_reason) in test_cases {
            assert_invalid_wikilink(input, expected_reason);
        }
    }

    #[test]
    fn test_parse_wikilink_special_chars() {
        let test_cases = vec![
            (
                "[[!@#$%^&*()]]",
                "!@#$%^&*()",
                "!@#$%^&*()",
                AliasExpectation::DirectLink,
            ),
            (
                "[[../path/to/file]]",
                "../path/to/file",
                "../path/to/file",
                AliasExpectation::DirectLink,
            ),
            (
                "[[file (1)]]",
                "file (1)",
                "file (1)",
                AliasExpectation::DirectLink,
            ),
            (
                "[[file (1)|version 1]]",
                "file (1)",
                "version 1",
                AliasExpectation::Aliased,
            ),
            (
                "[[target|(text)]]",
                "target",
                "(text)",
                AliasExpectation::Aliased,
            ),
        ];

        for (input, target, display, alias_expectation) in test_cases {
            assert_valid_wikilink(input, target, display, alias_expectation);
        }
    }
}

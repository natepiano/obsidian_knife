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

/// `ToWikilink` converts strings to wikilink text.
pub trait ToWikilink {
    /// Returns `self` as `[[target]]` wikilink text.
    fn to_wikilink(&self) -> String;

    /// Builds an aliased wikilink from `self` and `display_text`.
    /// Matching target and display text return `[[target]]`; differing values return
    /// `[[target|display]]`.
    fn to_aliased_wikilink(&self, display_text: &str) -> String
    where
        Self: AsRef<str>,
    {
        let target_without_markdown = strip_markdown_extension(self.as_ref());

        if target_without_markdown == display_text {
            target_without_markdown.to_wikilink()
        } else {
            format!(
                "{OPENING_WIKILINK}{target_without_markdown}{PIPE}{display_text}{CLOSING_WIKILINK}"
            )
        }
    }
}

impl ToWikilink for str {
    fn to_wikilink(&self) -> String {
        format!(
            "{OPENING_WIKILINK}{}{CLOSING_WIKILINK}",
            strip_markdown_extension(self)
        )
    }
}

impl ToWikilink for String {
    fn to_wikilink(&self) -> String { self.as_str().to_wikilink() }
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

/// Removes `MARKDOWN_SUFFIX` from `text` when it is present.
fn strip_markdown_extension(text: &str) -> &str {
    text.strip_suffix(MARKDOWN_SUFFIX).unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use crate::wikilink::ToWikilink;

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
}

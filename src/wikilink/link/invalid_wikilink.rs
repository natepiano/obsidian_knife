use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;

use crate::wikilink::constants::INVALID_WIKILINK_DOUBLE_ALIAS;
use crate::wikilink::constants::INVALID_WIKILINK_EMAIL_ADDRESS;
use crate::wikilink::constants::INVALID_WIKILINK_EMPTY;
use crate::wikilink::constants::INVALID_WIKILINK_NESTED_OPENING;
use crate::wikilink::constants::INVALID_WIKILINK_PREFIX;
use crate::wikilink::constants::INVALID_WIKILINK_RAW_HTTP_LINK;
use crate::wikilink::constants::INVALID_WIKILINK_TAG;
use crate::wikilink::constants::INVALID_WIKILINK_UNCLOSED_INLINE_CODE;
use crate::wikilink::constants::INVALID_WIKILINK_UNMATCHED_CLOSING;
use crate::wikilink::constants::INVALID_WIKILINK_UNMATCHED_MARKDOWN_LINK_OPENING;
use crate::wikilink::constants::INVALID_WIKILINK_UNMATCHED_OPENING;
use crate::wikilink::constants::INVALID_WIKILINK_UNMATCHED_SINGLE;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidWikilink {
    pub content:     String, // The actual problematic wikilink text
    pub reason:      InvalidWikilinkReason,
    pub span:        (usize, usize), // Start and end positions in the original text
    pub line:        String,         // The full line containing the invalid wikilink
    pub line_number: usize,          // The line number where the invalid wikilink appears
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

impl Display for InvalidWikilink {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{INVALID_WIKILINK_PREFIX} {}, position {}-{}: '{}' {}",
            self.line_number, self.span.0, self.span.1, self.content, self.reason
        )
    }
}

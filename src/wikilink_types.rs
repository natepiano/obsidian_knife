use serde::{Deserialize, Serialize};
use std::cmp::PartialEq;
use std::fmt;

/// Trait to convert strings to wikilink format
pub trait ToWikilink {
    /// Converts the string to a wikilink format by surrounding it with [[]]
    fn to_wikilink(&self) -> String;

    /// Creates an aliased wikilink using the target (self) and display text
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
            format!("[[{}|{}]]", target_without_md, display_text)
        }
    }
}

impl ToWikilink for str {
    fn to_wikilink(&self) -> String {
        format!("[[{}]]", strip_md_extension(self))
    }
}

impl ToWikilink for String {
    fn to_wikilink(&self) -> String {
        self.as_str().to_wikilink()
    }
}

/// Helper function to strip .md extension if present
fn strip_md_extension(text: &str) -> &str {
    text.strip_suffix(".md").unwrap_or(text)
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Wikilink {
    pub display_text: String,
    pub target: String,
    pub is_alias: bool,
}

impl fmt::Display for Wikilink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}{}",
            self.target,
            if self.is_alias { "|" } else { "" },
            if self.is_alias {
                &self.display_text
            } else {
                ""
            }
        )
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum InvalidWikilinkReason {
    DoubleAlias,               // e.g. [[A|B|C]]
    EmptyWikilink,             // [[]] or [[|]]
    EmailAddress,              // bob@rock.com
    NestedOpening,             // [[blah [[blah]]
    Tag,                       // #tags should be ignored
    UnmatchedClosing,          // ]] without matching [[
    UnmatchedMarkdownOpening,  // [ without following ]
    UnmatchedOpening,          // [[ without closing ]]
    UnmatchedSingleInWikilink, // ] without [ or [ without ]
}

impl fmt::Display for InvalidWikilinkReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DoubleAlias => write!(f, "contains multiple alias separators"),
            Self::EmailAddress => write!(f, "ignore email addresses for back population"),
            Self::EmptyWikilink => write!(f, "contains empty wikilink"),
            Self::NestedOpening => write!(f, "contains a nested opening"),
            Self::Tag => write!(f, "ignore tags for back population"),
            Self::UnmatchedClosing => write!(f, "contains unmatched closing brackets ']]'"),
            Self::UnmatchedMarkdownOpening => write!(f, "'[' without following match"),
            Self::UnmatchedOpening => write!(f, "contains unmatched opening brackets '[['"),
            Self::UnmatchedSingleInWikilink => write!(f, "contains unmatched bracket '[' or ']'"),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct InvalidWikilink {
    pub content: String, // The actual problematic wikilink text
    pub reason: InvalidWikilinkReason,
    pub span: (usize, usize), // Start and end positions in the original text
    pub line: String,         // The full line containing the invalid wikilink
    pub line_number: usize,   // The line number where the invalid wikilink appears
}

impl fmt::Display for InvalidWikilink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Invalid wikilink at line {}, position {}-{}: '{}' {}",
            self.line_number, self.span.0, self.span.1, self.content, self.reason
        )
    }
}

#[derive(Debug, PartialEq)]
pub enum WikilinkParseResult {
    Valid(Wikilink),
    Invalid(ParsedInvalidWikilink),
}

#[derive(Debug, PartialEq)]
pub struct ParsedInvalidWikilink {
    pub content: String,
    pub reason: InvalidWikilinkReason,
    pub span: (usize, usize),
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

#[derive(Debug, Default)]
pub struct ExtractedWikilinks {
    pub valid: Vec<Wikilink>,
    pub invalid: Vec<InvalidWikilink>,
}

#[derive(Debug, Default)]
pub struct ParsedExtractedWikilinks {
    pub valid: Vec<Wikilink>,
    pub invalid: Vec<ParsedInvalidWikilink>,
}

use std::cmp::PartialEq;
use std::fmt;
use serde::{Deserialize, Serialize};

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
    if text.ends_with(".md") {
        &text[..text.len() - 3]
    } else {
        text
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Wikilink {
    pub display_text: String,
    pub target: String,
    pub is_alias: bool,
}

#[derive(Debug, Default)]
pub struct ExtractedWikilinks {
    pub valid: Vec<Wikilink>,
    pub invalid: Vec<InvalidWikilink>,
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

#[derive(Debug, PartialEq)]
pub enum InvalidWikilinkReason {
    NestedOpening,     // Contains [[ inside
    UnmatchedClosing,  // ]] without matching [[
    UnmatchedOpening, // [[ without closing ]]
    UnmatchedSingleOpening, // [ without ]
    ImproperlyNested,  // e.g. [[A|B]]C]]
    UnmatchedPipe,     // More pipes than expected or unescaped
    DoubleAlias,       // e.g. [[A|B|C]]
    Malformed         // Catch-all for other malformed cases
}

// Update Display implementation for InvalidWikilinkReason to handle the new variant
impl fmt::Display for InvalidWikilinkReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NestedOpening => write!(f, "contains nested opening brackets '[['"),
            Self::UnmatchedClosing => write!(f, "contains unmatched closing brackets ']]'"),
            Self::UnmatchedOpening => write!(f, "contains unmatched opening brackets '[['"),
            Self::UnmatchedSingleOpening => write!(f, "contains unmatched opening bracket '['"),
            Self::ImproperlyNested => write!(f, "contains improperly nested brackets"),
            Self::UnmatchedPipe => write!(f, "contains unmatched pipe character '|'"),
            Self::DoubleAlias => write!(f, "contains multiple alias separators"),
            Self::Malformed => write!(f, "has malformed wikilink structure")
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct InvalidWikilink {
    pub content: String,            // The actual problematic wikilink text
    pub reason: InvalidWikilinkReason,
    pub span: (usize, usize)        // Start and end positions in the original text
}

impl fmt::Display for InvalidWikilink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid wikilink at position {}-{}: '{}' {}",
               self.span.0,
               self.span.1,
               self.content,
               self.reason
        )
    }
}

#[derive(Debug, PartialEq)]
pub enum WikilinkParseResult {
    Valid(Wikilink),
    Invalid(InvalidWikilink)
}

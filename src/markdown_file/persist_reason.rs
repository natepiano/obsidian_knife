use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;

/// Why a `MarkdownFile` needs writing back to disk. Every variant records a change the tool
/// made to the file's content; frontmatter dates are owned by the vault, not by this tool,
/// so no variant describes a date the tool decided to repair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistReason {
    BackPopulated,
    ImageReferencesModified,
    LinksCanonicalized,
    PhantomLinksResolved,
}

impl Display for PersistReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::BackPopulated => write!(f, "back populated"),
            Self::ImageReferencesModified => write!(f, "image references updated"),
            Self::LinksCanonicalized => write!(f, "links canonicalized"),
            Self::PhantomLinksResolved => write!(f, "phantom links resolved"),
        }
    }
}

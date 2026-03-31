#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions use unwrap/expect/panic for clarity"
)]
mod extract_wikilink_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions use unwrap/expect/panic for clarity"
)]
mod markdown_link_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions use unwrap/expect/panic for clarity"
)]
mod wikilink_creation_tests;

mod wikilink_parser;
mod wikilink_types;

pub use wikilink_parser::create_filename_wikilink;
pub use wikilink_parser::extract_wikilinks;
pub use wikilink_parser::is_wikilink;
pub use wikilink_parser::is_within_wikilink;
#[allow(
    unused_imports,
    reason = "facade re-export for test modules via super::*"
)]
use wikilink_parser::parse_wikilink;
pub use wikilink_types::ExtractedWikilinks;
pub use wikilink_types::InvalidWikilink;
pub use wikilink_types::InvalidWikilinkReason;
pub use wikilink_types::ToWikilink;
pub use wikilink_types::Wikilink;

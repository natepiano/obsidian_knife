#[cfg(test)]
mod extract_wikilink_tests;
#[cfg(test)]
mod markdown_link_tests;
#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]
mod wikilink_creation_tests;

mod wikilink_parser;
mod wikilink_types;

pub use wikilink_parser::create_filename_wikilink;
pub use wikilink_parser::extract_wikilinks;
pub use wikilink_parser::is_wikilink;
pub use wikilink_parser::is_within_wikilink;
pub use wikilink_types::ExtractedWikilinks;
pub use wikilink_types::InvalidWikilink;
pub use wikilink_types::InvalidWikilinkReason;
pub use wikilink_types::ToWikilink;
pub use wikilink_types::Wikilink;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod alias_handling_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod back_populate_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod case_sensitivity_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod date_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod exclusion_zone_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod matching_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod parse_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod persist_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]
mod process_content_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod table_handling_tests;

mod back_populate;
mod date_validation;
mod markdown_file_core;
mod markdown_file_types;
mod match_helpers;
mod text_excluder;

pub use markdown_file_core::MarkdownFile;
pub use markdown_file_types::BackPopulateMatch;
pub use markdown_file_types::DateValidation;
#[cfg(test)]
pub use markdown_file_types::ImageLink;
pub use markdown_file_types::ImageLinkState;
pub use markdown_file_types::MatchContext;
pub use markdown_file_types::MatchType;
pub use markdown_file_types::PersistReason;
pub use markdown_file_types::ReplaceableContent;
pub use text_excluder::InlineCodeExcluder;

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

// Re-imports for child test modules that use `super::*`
#[cfg(test)]
#[allow(unused_imports, reason = "re-imported for test modules via super::*")]
use date_validation::extract_date;
#[cfg(test)]
#[allow(unused_imports, reason = "re-imported for test modules via super::*")]
use date_validation::get_date_validation_issue;
#[cfg(test)]
#[allow(unused_imports, reason = "re-imported for test modules via super::*")]
use date_validation::get_date_validations;
#[cfg(test)]
#[allow(unused_imports, reason = "re-imported for test modules via super::*")]
use date_validation::is_valid_date;
#[cfg(test)]
#[allow(unused_imports, reason = "re-imported for test modules via super::*")]
use date_validation::process_date_validations;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_core::MarkdownFile;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::BackPopulateMatch;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::BackPopulateMatches;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::DateCreatedFixValidation;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::DateValidation;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::DateValidationIssue;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::ImageLink;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::ImageLinkState;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::ImageLinkTarget;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::ImageLinkType;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::ImageLinks;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::MatchContext;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::MatchType;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::PersistReason;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::ReplaceableContent;
#[allow(
    unused_imports,
    reason = "facade re-exports used by other modules via crate::markdown_file::Type"
)]
pub use markdown_file_types::Wikilinks;
#[cfg(test)]
#[allow(unused_imports, reason = "re-imported for test modules via super::*")]
use match_helpers::is_in_markdown_table;
#[cfg(test)]
#[allow(unused_imports, reason = "re-imported for test modules via super::*")]
use match_helpers::is_word_boundary;
#[cfg(test)]
#[allow(unused_imports, reason = "re-imported for test modules via super::*")]
use match_helpers::range_overlaps;
pub use text_excluder::InlineCodeExcluder;

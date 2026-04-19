#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]

mod alias_handling_tests;
mod back_populate_tests;
mod case_sensitivity_tests;
mod date_tests;
mod exclusion_zone_tests;
mod matching_tests;
mod parse_tests;
mod persist_tests;
mod process_content_tests;
mod table_handling_tests;

pub(super) use super::BackPopulateMatch;
pub(super) use super::DateValidation;
pub(super) use super::ImageLink;
pub(super) use super::MarkdownFile;
pub(super) use super::MatchContext;
pub(super) use super::PersistReason;
pub(super) use super::date_validation;
pub(super) use super::date_validation::DateCreatedFixValidation;
pub(super) use super::date_validation::DateValidationIssue;
pub(super) use super::image_link::ImageLinkRendering;
pub(super) use super::image_link::ImageLinkTarget;
pub(super) use super::image_link::ImageLinkType;

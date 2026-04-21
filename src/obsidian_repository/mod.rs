#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod ambiguous_matches_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod file_limit_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod image_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]
mod obsidian_repository_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]
mod persist_file_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod scan_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod update_modified_tests;

mod back_populate;
mod image_processing;
mod repository;

pub use repository::ObsidianRepository;
pub use repository::format_relative_path;

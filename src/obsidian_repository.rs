#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions use unwrap/expect/panic for clarity"
)]
mod ambiguous_matches_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions use unwrap/expect/panic for clarity"
)]
mod file_limit_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions use unwrap/expect/panic for clarity"
)]
mod image_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions use unwrap/expect/panic for clarity"
)]
mod obsidian_repository_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions use unwrap/expect/panic for clarity"
)]
mod persist_file_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions use unwrap/expect/panic for clarity"
)]
mod scan_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions use unwrap/expect/panic for clarity"
)]
mod update_modified_tests;

mod back_populate;
mod image_processing;
mod repository_core;

pub use repository_core::ObsidianRepository;
pub use repository_core::format_relative_path;

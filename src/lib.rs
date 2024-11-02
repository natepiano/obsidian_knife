mod back_populate;
mod cleanup_images;
mod config;
mod constants;
mod deterministic_file_search;
mod file_utils;
mod frontmatter;
mod scan;
mod sha256_cache;
mod thread_safe_writer;
mod update_dates;
mod validated_config;
mod wikilink;
mod yaml_utils;

#[cfg(test)]
pub(crate) mod test_utils;

use std::error::Error;

// Re-export types for main
pub use config::Config;
pub use constants::*;
pub use thread_safe_writer::ThreadSafeWriter;
pub use validated_config::ValidatedConfig;

// lib was separated from main so it could be incorporated into integration tests
// such as config_tests.rs

pub fn process_config(
    config: ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let obsidian_repository_info = scan::scan_obsidian_folder(&config, writer)?;
    cleanup_images::cleanup_images(&config, &obsidian_repository_info, writer)?;
    update_dates::process_dates(&config, &obsidian_repository_info.markdown_files, writer)?;
    back_populate::process_back_populate(&config, &obsidian_repository_info, &writer)?;
    Ok(())
}

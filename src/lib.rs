// lib.rs
mod cleanup_images;
mod config;
mod constants;
mod file_utils;
mod frontmatter;
mod scan;
mod sha256_cache;
mod simplify_wikilinks;
mod thread_safe_writer;
mod update_dates;
mod validated_config;
mod wikilink;
mod yaml_utils;

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
    let collected_files = scan::scan_obsidian_folder(&config, writer)?;
    cleanup_images::cleanup_images(&config, &collected_files, writer)?;
    simplify_wikilinks::process_simplify_wikilinks(
        &config,
        &collected_files.markdown_files,
        writer,
    )?;
    update_dates::process_dates(&config, &collected_files.markdown_files, writer)?;
    Ok(())
}

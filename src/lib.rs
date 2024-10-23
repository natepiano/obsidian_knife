// lib.rs
pub mod cleanup_images;
pub mod config;
pub mod constants;
pub mod file_utils;
pub mod scan;
pub mod sha256_cache;
pub mod simplify_wikilinks;
pub mod thread_safe_writer;
pub mod validated_config;

use chrono::Local;
use std::error::Error;

// Re-export commonly used types
pub use config::Config;
pub use thread_safe_writer::ThreadSafeWriter;
pub use validated_config::ValidatedConfig;

pub fn process_config(
    config: ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let collected_files = scan::scan_obsidian_folder(&config, writer)?;
    cleanup_images::cleanup_images(&config, &collected_files, writer)?;
    simplify_wikilinks::process_simplify_wikilinks(&config, &collected_files.markdown_files, writer)?;
    Ok(())
}

pub fn write_execution_start(
    validated_config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let properties = format!(
        "time_stamp: {}\napply_changes: {}\n",
        timestamp,
        validated_config.apply_changes(),
    );

    writer.write_properties(&properties)?;
    writer.writeln("# ", "starting obsidian_knife")?;

    if validated_config.apply_changes() {
        writer.writeln("", "apply_changes enabled: changes will be applied")?;
    } else {
        writer.writeln("", "apply_changes disabled: no changes will be applied")?;
    }
    println!();
    Ok(())
}

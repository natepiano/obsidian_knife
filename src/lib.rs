#[cfg(test)]
pub(crate) mod test_utils;

mod back_populate;
// mod cleanup_dates;
mod cleanup_images;
mod config;
mod constants;
mod frontmatter;
mod markdown_file_info;
mod markdown_files;
mod obsidian_repository_info;
mod scan;
mod utils;
mod wikilink;
mod wikilink_types;
mod yaml_frontmatter;

// Re-export types for main
pub use constants::*;
pub use utils::Timer;

use crate::frontmatter::FrontMatter;
use crate::markdown_file_info::MarkdownFileInfo;
use crate::yaml_frontmatter::YamlFrontMatter;
use crate::{config::Config, config::ValidatedConfig};
use chrono::Utc;
use std::error::Error;
use std::path::PathBuf;
use utils::expand_tilde;
use utils::ThreadSafeWriter;

// lib was separated from main so it could be incorporated into integration tests
// such as config_tests.rs - but that's not happening so...
pub fn process_config(config_path: PathBuf) -> Result<(), Box<dyn Error + Send + Sync>> {
    let expanded_path = expand_tilde(config_path);

    let mut markdown_file = MarkdownFileInfo::new(expanded_path)?;
    let mut config = if let Some(frontmatter) = &markdown_file.frontmatter {
        Config::from_frontmatter(frontmatter.clone())?
    } else {
        return Err("Config file must have frontmatter".into());
    };

    let validated_config = config.validate()?;
    let writer = ThreadSafeWriter::new(validated_config.output_folder())?;

    write_execution_start(&validated_config, &writer)?;

    let mut obsidian_repository_info = scan::scan_obsidian_folder(&validated_config)?;

   // frontmatter::report_frontmatter_issues(&obsidian_repository_info.markdown_files, &writer)?;
    obsidian_repository_info.markdown_files.report_frontmatter_issues(&writer)?;
    cleanup_images::cleanup_images(&validated_config, &mut obsidian_repository_info, &writer)?;

    // cleanup_dates::process_dates(
    //     &validated_config,
    //     &mut obsidian_repository_info.markdown_files,
    //     &writer,
    // )?;

    back_populate::process_back_populate(
        &validated_config,
        &mut obsidian_repository_info,
        &writer,
    )?;

   // write_date_validation_table(&writer, &obsidian_repository_info.markdown_files)?;
    obsidian_repository_info.markdown_files.write_date_validation_table(&writer)?;
    obsidian_repository_info.markdown_files.write_persist_reasons_table(&writer)?;


    if config.apply_changes == Some(true) {
        // this whole thing is a bit of a code smell
        // converting from frontmatter to config
        // making sure to update modified date so we can re-use markdown_file persist
        // which in this case doesn't actually matter but does matter for frontmatter...
        config.apply_changes = Some(false);
        let config_yaml = config.to_yaml_str()?;
        let updated_frontmatter = FrontMatter::from_yaml_str(&config_yaml)?;
        markdown_file.frontmatter = Some(updated_frontmatter);
        markdown_file
            .frontmatter
            .as_mut()
            .unwrap()
            .set_date_modified_now();
        markdown_file.persist()?;
    }

    Ok(())
}

pub fn write_execution_start(
    validated_config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let timestamp = Utc::now().format(FORMAT_TIME_STAMP);
    let properties = format!(
        "{}{}\n{}{}\n",
        YAML_TIMESTAMP,
        timestamp,
        YAML_APPLY_CHANGES,
        validated_config.apply_changes(),
    );

    writer.write_properties(&properties)?;

    if validated_config.apply_changes() {
        writer.writeln("", MODE_APPLY_CHANGES)?;
    } else {
        writer.writeln("", MODE_DRY_RUN)?;
    }
    Ok(())
}

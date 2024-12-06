#[cfg(test)]
pub mod test_utils;

// mod cleanup_dates;
mod config;
mod constants;
mod frontmatter;
mod image_file_info;
mod image_files;
mod markdown_file_info;
mod markdown_files;
mod obsidian_repository_info;
mod report;
mod scan;
mod utils;
mod validated_config;
mod wikilink;
mod yaml_frontmatter;

// Re-export types for main
pub use constants::*;
pub use utils::Timer;

use crate::config::Config;
use crate::frontmatter::FrontMatter;
use crate::markdown_file_info::MarkdownFileInfo;
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::validated_config::ValidatedConfig;
use crate::yaml_frontmatter::YamlFrontMatter;
use std::error::Error;
use std::path::PathBuf;
use utils::expand_tilde;

// lib was separated from main so it could be incorporated into integration tests
// such as config_tests.rs - but that's not happening so...
pub fn process_config(config_path: PathBuf) -> Result<(), Box<dyn Error + Send + Sync>> {
    let expanded_path = expand_tilde(config_path);

    let mut markdown_file = MarkdownFileInfo::new(expanded_path, DEFAULT_TIMEZONE)?;
    let mut config = if let Some(frontmatter) = &markdown_file.frontmatter {
        Config::from_frontmatter(frontmatter.clone())?
    } else {
        return Err("Config file must have frontmatter".into());
    };

    let validated_config = config.validate()?;

    // ANALYSIS PHASE
    let mut obsidian_repository_info = ObsidianRepositoryInfo::new(&validated_config)?;
    let (grouped_images, image_operations) =
        obsidian_repository_info.analyze_repository(&validated_config)?;

    // REPORTING PHASE
    obsidian_repository_info.write_reports(&validated_config, &grouped_images)?;

    if config.apply_changes == Some(true) {
        obsidian_repository_info.persist(image_operations)?;
        reset_apply_changes(&mut markdown_file, &mut config)?;
    }

    Ok(())
}

fn reset_apply_changes(
    markdown_file: &mut MarkdownFileInfo,
    config: &mut Config,
) -> Result<(), Box<dyn Error + Send + Sync>> {
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
    Ok(())
}

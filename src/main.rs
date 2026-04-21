#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]
mod test_support;

mod config;
mod constants;
mod description_builder;
mod frontmatter;
mod image_file;
mod markdown_file;
mod markdown_files;
mod obsidian_repository;
mod report;
mod utils;
mod validated_config;
mod wikilink;
mod yaml_frontmatter;

use std::error::Error;
use std::path::PathBuf;

use crate::config::Config;
use crate::config::ConfiguredChanges;
use crate::constants::DEFAULT_TIMEZONE;
#[cfg(debug_assertions)]
use crate::constants::DEV;
use crate::constants::ERROR_DETAILS;
use crate::constants::ERROR_OCCURRED;
use crate::constants::ERROR_SOURCE;
use crate::constants::ERROR_TYPE;
use crate::constants::OBSIDIAN_KNIFE;
#[cfg(not(debug_assertions))]
use crate::constants::RELEASE;
use crate::constants::TOTAL_TIME;
use crate::constants::USAGE;
use crate::frontmatter::FrontMatter;
use crate::markdown_file::MarkdownFile;
use crate::obsidian_repository::ObsidianRepository;
use crate::utils::Timer;
use crate::validated_config::ChangeMode;
use crate::yaml_frontmatter::YamlFrontMatter;
use crate::yaml_frontmatter::YamlFrontMatterError;

// Custom error type for main specific errors
#[derive(Debug)]
enum MainError {
    Usage(String),
}

impl std::fmt::Display for MainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(msg) => write!(f, "{msg}"),
        }
    }
}

impl Error for MainError {}

fn process_obsidian_repository(config_path: PathBuf) -> Result<(), Box<dyn Error + Send + Sync>> {
    let expanded_path = utils::expand_tilde(config_path);

    let mut markdown_file = MarkdownFile::new(expanded_path, DEFAULT_TIMEZONE)?;
    let mut config = if let Some(frontmatter) = &markdown_file.frontmatter {
        Config::try_from(frontmatter)?
    } else {
        return Err(markdown_file
            .frontmatter_error
            .unwrap_or(YamlFrontMatterError::Missing)
            .into());
    };

    let validated_config = config.validate()?;

    let obsidian_repository = ObsidianRepository::new(&validated_config)?;
    obsidian_repository.write_reports(&validated_config)?;

    if matches!(config.change_mode(), ChangeMode::Apply) {
        obsidian_repository.persist()?;
        reset_change_mode(&mut markdown_file, &mut config)?;
    }

    Ok(())
}

fn reset_change_mode(
    markdown_file: &mut MarkdownFile,
    config: &mut Config,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    config.configured_changes = ConfiguredChanges::DryRun;
    let config_yaml = config.to_yaml_str()?;
    let updated_frontmatter = FrontMatter::from_yaml_str(&config_yaml)?;
    markdown_file.frontmatter = Some(updated_frontmatter);

    let operational_timezone = config
        .operational_timezone
        .as_ref()
        .map_or(DEFAULT_TIMEZONE, |time_zone| time_zone.as_str());

    if let Some(frontmatter) = markdown_file.frontmatter.as_mut() {
        frontmatter.set_date_modified_now(operational_timezone);
    }
    markdown_file.persist()?;
    Ok(())
}

// Separate error handling and reporting logic
fn handle_error(e: Box<dyn Error + Send + Sync>) -> Result<(), Box<dyn Error + Send + Sync>> {
    eprintln!("{ERROR_OCCURRED}");
    eprintln!("{ERROR_TYPE}");
    let error_type_name = std::any::type_name_of_val(&*e);
    eprintln!("{error_type_name}");
    eprintln!("{ERROR_DETAILS} {e}");

    if let Some(source) = e.source() {
        eprintln!("{ERROR_SOURCE} {source}");
    }
    Err(e)
}

// Get config file name with better error handling
fn get_config_file() -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 2 {
        return Err(Box::new(MainError::Usage(USAGE.into())));
    }

    Ok(PathBuf::from(&args[1]))
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let _timer = Timer::new(TOTAL_TIME);

    #[cfg(debug_assertions)]
    println!("{OBSIDIAN_KNIFE}\n{DEV} v.{}", env!("CARGO_PKG_VERSION"));

    #[cfg(not(debug_assertions))]
    println!(
        "{OBSIDIAN_KNIFE}\n{RELEASE} v.{}",
        env!("CARGO_PKG_VERSION")
    );

    let config_path = get_config_file()?;

    match process_obsidian_repository(config_path) {
        Ok(()) => Ok(()),
        Err(e) => handle_error(e), // Removed writer parameter
    }
}

#[cfg(test)]
#[allow(clippy::panic, reason = "tests should panic on unexpected values")]
mod main_tests {
    use super::*;

    #[test]
    fn test_get_config_file_no_args() {
        match get_config_file() {
            Ok(_) => panic!("Expected error for missing arguments"),
            Err(e) => assert!(e.to_string().contains("usage:")),
        }
    }
}

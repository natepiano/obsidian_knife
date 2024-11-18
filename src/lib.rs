#[cfg(test)]
pub(crate) mod test_utils;

mod back_populate;
// mod cleanup_dates;
mod cleanup_images;
mod config;
mod constants;
mod deterministic_file_search;
mod file_utils;
mod frontmatter;
mod markdown_file_info;
mod obsidian_repository_info;
mod scan;
mod utils;
mod wikilink;
mod wikilink_types;
mod yaml_frontmatter;

// Re-export types for main
pub use constants::*;
pub use utils::Timer;

use crate::markdown_file_info::write_date_validation_table;
use crate::{config::Config, config::ValidatedConfig};
use chrono::Utc;
use std::error::Error;
use std::path::PathBuf;
use utils::ThreadSafeWriter;

// lib was separated from main so it could be incorporated into integration tests
// such as config_tests.rs - but that's not happening so...
pub fn process_config(config_path: PathBuf) -> Result<(), Box<dyn Error + Send + Sync>> {
    let config = Config::from_obsidian_file(&config_path)?;

    let validated_config = config.validate()?;
    let writer = ThreadSafeWriter::new(validated_config.output_folder())?;

    write_execution_start(&validated_config, &writer)?;

    let mut obsidian_repository_info = scan::scan_obsidian_folder(&validated_config)?;

    frontmatter::report_frontmatter_issues(&obsidian_repository_info.markdown_files, &writer)?;
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

    write_date_validation_table(&writer, &obsidian_repository_info.markdown_files)?;

    config.reset_apply_changes()?;

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

#[cfg(test)]
mod lib_tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_environment() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();

        // Create Obsidian vault structure with absolute path
        let obsidian_path = temp_dir.path().join("vault");
        fs::create_dir(&obsidian_path).unwrap();
        fs::create_dir(obsidian_path.join(".obsidian")).unwrap();

        // Ensure path exists and get canonical path
        assert!(
            obsidian_path.exists(),
            "Obsidian path does not exist after creation"
        );
        let canonical_path = obsidian_path
            .canonicalize()
            .expect("Failed to get canonical path");

        // Create output directory
        fs::create_dir(canonical_path.join("output")).unwrap();

        // Create config file
        let config_path = temp_dir.path().join("config.md");
        let config_content = format!(
            r#"---
obsidian_path: {}
apply_changes: false
output_folder: output
---"#,
            canonical_path.to_string_lossy()
        );

        let mut file = File::create(&config_path).unwrap();
        write!(file, "{}", config_content).unwrap();

        assert!(
            config_path.exists(),
            "Config file does not exist after creation"
        );

        (temp_dir, config_path)
    }

    #[test]
    fn test_config_with_valid_setup() {
        let (_temp_dir, config_path) = create_test_environment();

        match Config::from_obsidian_file(&config_path) {
            Ok(config) => {
                let validated_config = config.validate().unwrap();
                assert!(
                    !validated_config.apply_changes(),
                    "apply_changes should be false"
                );
                assert!(
                    validated_config.obsidian_path().exists(),
                    "Obsidian path should exist"
                );
            }
            Err(e) => panic!(
                "Failed to initialize config: {} (Obsidian path exists: {})",
                e,
                _temp_dir.path().join("vault").exists()
            ),
        }
    }

    #[test]
    fn test_config_with_missing_obsidian_path() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.md");

        let config_content = r#"---
obsidian_path: /nonexistent/path
apply_changes: false
---"#;

        let mut file = File::create(&config_path).unwrap();
        write!(file, "{}", config_content).unwrap();

        let config = Config::from_obsidian_file(&config_path).unwrap();
        match config.validate() {
            Ok(_) => panic!("Expected error for missing Obsidian path"),
            Err(e) => assert!(e.to_string().contains("obsidian path does not exist")),
        }
    }

    #[test]
    fn test_config_with_invalid_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.md");

        let config_content = r#"---
invalid: yaml: content:
---"#;

        let mut file = File::create(&config_path).unwrap();
        write!(file, "{}", config_content).unwrap();

        match Config::from_obsidian_file(&config_path) {
            Ok(_) => panic!("Expected error for invalid YAML"),
            Err(_) => (), // Any error is fine here as we just want to ensure it fails
        }
    }
}

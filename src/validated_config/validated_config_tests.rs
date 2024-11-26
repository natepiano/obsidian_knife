use crate::validated_config::ValidatedConfigBuilder;
use std::fs;
use std::path::PathBuf;

use super::*;
use crate::config::Config;
use tempfile::TempDir;

pub fn get_test_validated_config_builder(temp_dir: &TempDir) -> ValidatedConfigBuilder {
    let mut builder = ValidatedConfigBuilder::default();
    builder.obsidian_path(temp_dir.path().to_path_buf());
    builder.output_folder(temp_dir.path().join("output"));
    builder
}

pub fn get_test_validated_config_result(
    temp_dir: &TempDir,
    modifier: impl FnOnce(&mut ValidatedConfigBuilder),
) -> Result<ValidatedConfig, ValidationError> {
    let mut builder = get_test_validated_config_builder(temp_dir);
    modifier(&mut builder);
    builder.build()
}

pub fn get_test_validated_config(
    temp_dir: &TempDir,
    back_populate_file_filter: Option<&str>,
) -> ValidatedConfig {
    get_test_validated_config_result(temp_dir, |builder| {
        if let Some(filter) = back_populate_file_filter {
            builder.back_populate_file_filter(Some(filter.to_string()));
        }
    })
    .unwrap()
}

#[test]
fn test_back_populate_file_filter() {
    let temp_dir = TempDir::new().unwrap();
    let config = get_test_validated_config(&temp_dir, Some("test_file"));

    assert_eq!(
        config.back_populate_file_filter(),
        Some("test_file.md".to_string())
    );

    // Test with wikilink format
    let config = get_test_validated_config(&temp_dir, Some("[[test_file]]"));
    assert_eq!(
        config.back_populate_file_filter(),
        Some("test_file.md".to_string())
    );

    // Test with existing .md extension
    let config = get_test_validated_config(&temp_dir, Some("test_file.md"));
    assert_eq!(
        config.back_populate_file_filter(),
        Some("test_file.md".to_string())
    );

    // Test with wikilink and .md extension
    let config = get_test_validated_config(&temp_dir, Some("[[test_file.md]]"));
    assert_eq!(
        config.back_populate_file_filter(),
        Some("test_file.md".to_string())
    );

    // Test with None
    let config = get_test_validated_config(&temp_dir, None);
    assert_eq!(config.back_populate_file_filter(), None);
}

#[test]
fn test_preserve_obsidian_in_ignore_folders() {
    let temp_dir = TempDir::new().unwrap();
    let obsidian_path = temp_dir.path().to_path_buf();

    // Create builder with initial ignore folders containing .obsidian
    let mut builder = ValidatedConfigBuilder::default();
    builder.obsidian_path(obsidian_path.clone());

    // First set ignore_folders with .obsidian
    builder.ignore_folders(Some(vec![PathBuf::from(".obsidian")]));

    // Then set output folder
    builder.output_folder(obsidian_path.join("custom_output"));

    // Build and verify both paths are in ignore folders
    let config = builder.build().unwrap();
    let ignore_folders = config.ignore_folders().unwrap();

    let obsidian_dir = obsidian_path.join(".obsidian");
    let output_dir = obsidian_path.join("custom_output");

    assert!(
        ignore_folders.contains(&obsidian_dir),
        "Should contain .obsidian directory"
    );
    assert!(
        ignore_folders.contains(&output_dir),
        "Should contain output directory"
    );
}

#[test]
fn test_timezone_validation() {
    let temp_dir = TempDir::new().unwrap();

    // Test valid timezone
    let yaml = format!(
        r#"
obsidian_path: {}
operational_timezone: "America/Los_Angeles""#,
        temp_dir.path().display()
    );

    let config: Config = serde_yaml::from_str(&yaml).unwrap();
    let result = config.validate();
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap().operational_timezone(),
        "America/Los_Angeles"
    );

    // Test invalid timezone
    let yaml = format!(
        r#"
obsidian_path: {}
operational_timezone: "Invalid/Timezone""#,
        temp_dir.path().display()
    );

    let config: Config = serde_yaml::from_str(&yaml).unwrap();
    let result = config.validate();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid timezone: Invalid/Timezone"));
}

#[test]
fn test_default_timezone() {
    let temp_dir = TempDir::new().unwrap();

    // Test default timezone when none specified
    let yaml = format!(
        r#"
obsidian_path: {}"#,
        temp_dir.path().display()
    );

    let config: Config = serde_yaml::from_str(&yaml).unwrap();
    let result = config.validate();
    assert!(result.is_ok());
    assert_eq!(result.unwrap().operational_timezone(), "America/New_York");
}

#[test]
fn test_default_output_folder() {
    // Create a temporary directory for the test
    let temp_dir = TempDir::new().unwrap();

    let yaml = format!(
        r#"
obsidian_path: {}"#,
        temp_dir.path().display()
    );

    let config: Config = serde_yaml::from_str(&yaml).unwrap();
    let validated = config.validate().unwrap();

    let expected_output = temp_dir.path().join("obsidian_knife");
    assert_eq!(validated.output_folder(), expected_output.as_path());
}

#[test]
fn test_output_folder_added_to_ignore() {
    // Create a temporary directory for the test
    let temp_dir = TempDir::new().unwrap();

    // Create the .obsidian directory
    let obsidian_dir = temp_dir.path().join(".obsidian");
    fs::create_dir(&obsidian_dir).unwrap();

    let yaml = format!(
        r#"
obsidian_path: {}
output_folder: custom_output
ignore_folders:
  - .obsidian"#,
        temp_dir.path().display()
    );

    let config: Config = serde_yaml::from_str(&yaml).unwrap();
    let validated = config.validate().unwrap();

    let ignore_folders = validated.ignore_folders().unwrap();
    let output_path = validated.output_folder();

    assert!(ignore_folders.contains(&output_path.to_path_buf()));
    assert!(ignore_folders.contains(&obsidian_dir));
}

#[test]
fn test_validate_empty_output_folder() {
    // Create a temporary directory for the test
    let temp_dir = TempDir::new().unwrap();

    let yaml = format!(
        r#"
obsidian_path: {}
output_folder: "  ""#,
        temp_dir.path().display()
    );

    let config: Config = serde_yaml::from_str(&yaml).unwrap();
    let result = config.validate();
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(matches!(
        *err.downcast_ref::<ValidationError>().unwrap(),
        ValidationError::EmptyOutputFolder
    ));
}

#[test]
fn test_invalid_back_populate_count() {
    let temp_dir = TempDir::new().unwrap();
    let result = get_test_validated_config_result(&temp_dir, |builder| {
        builder.file_process_limit(Some(0));
    });

    assert!(matches!(
        result.unwrap_err(),
        ValidationError::InvalidFileProcessLimit
    ));

    // Test that valid counts work
    let result = get_test_validated_config_result(&temp_dir, |builder| {
        builder.file_process_limit(Some(1));
    });
    assert!(result.is_ok());
}

#[test]
fn test_empty_back_populate_file_filter() {
    let temp_dir = TempDir::new().unwrap();
    let result = get_test_validated_config_result(&temp_dir, |builder| {
        builder.back_populate_file_filter(Some("   ".to_string()));
    });

    assert!(matches!(
        result.unwrap_err(),
        ValidationError::EmptyBackPopulateFileFilter
    ));

    // Test that non-empty filter works
    let result = get_test_validated_config_result(&temp_dir, |builder| {
        builder.back_populate_file_filter(Some("valid_filter".to_string()));
    });
    assert!(result.is_ok());
}

#[test]
fn test_invalid_obsidian_path() {
    let temp_dir = TempDir::new().unwrap();
    let nonexistent_path = temp_dir.path().join("nonexistent");

    let result = get_test_validated_config_result(&temp_dir, |builder| {
        builder.obsidian_path(nonexistent_path.clone());
    });

    assert!(matches!(
        result.unwrap_err(),
        ValidationError::InvalidObsidianPath(path) if path == nonexistent_path.display().to_string()
    ));
}

#[test]
fn test_missing_obsidian_path() {
    let temp_dir = TempDir::new().unwrap();
    let mut builder = ValidatedConfigBuilder::default();
    // Don't set obsidian_path at all
    builder.output_folder(temp_dir.path().join("output"));

    let result = builder.build();
    assert!(matches!(
        result.unwrap_err(),
        ValidationError::MissingObsidianPath
    ));
}

#[test]
fn test_uninitialized_fields() {
    let temp_dir = TempDir::new().unwrap();
    let mut builder = ValidatedConfigBuilder::default();

    // Set obsidian_path but not output_folder
    builder.obsidian_path(temp_dir.path().to_path_buf());
    let result = builder.build();

    assert!(matches!(
        result.unwrap_err(),
        ValidationError::UninitializedField(field) if field == "output_folder"
    ));

    // Now test with obsidian_path missing but output_folder set
    let mut builder = ValidatedConfigBuilder::default();
    builder.output_folder(temp_dir.path().join("output"));
    let result = builder.build();

    // This should fail with MissingObsidianPath first
    assert!(matches!(
        result.unwrap_err(),
        ValidationError::MissingObsidianPath
    ));
}

#[test]
fn test_multiple_validation_errors() {
    let temp_dir = TempDir::new().unwrap();
    let result = get_test_validated_config_result(&temp_dir, |builder| {
        builder
            .file_process_limit(Some(0))
            .back_populate_file_filter(Some("".to_string()));
    });

    // Should fail with the first error encountered
    assert!(matches!(
        result.unwrap_err(),
        ValidationError::InvalidFileProcessLimit
    ));
}

#[test]
fn test_all_validation_passes() {
    let temp_dir = TempDir::new().unwrap();
    let result = get_test_validated_config_result(&temp_dir, |builder| {
        builder
            .file_process_limit(Some(1))
            .back_populate_file_filter(Some("valid_filter".to_string()))
            .operational_timezone("America/New_York".to_string());
    });

    assert!(result.is_ok());
}

#[test]
fn test_timezone_edge_cases() {
    let temp_dir = TempDir::new().unwrap();

    // Test empty timezone
    let result = get_test_validated_config_result(&temp_dir, |builder| {
        builder.operational_timezone("".to_string());
    });
    assert!(matches!(
        result.unwrap_err(),
        ValidationError::InvalidTimezone(_)
    ));

    // Test timezone with invalid characters
    let result = get_test_validated_config_result(&temp_dir, |builder| {
        builder.operational_timezone("America/New@York".to_string());
    });
    assert!(matches!(
        result.unwrap_err(),
        ValidationError::InvalidTimezone(_)
    ));
}

#[test]
fn test_output_folder_edge_cases() {
    let temp_dir = TempDir::new().unwrap();

    // Test with absolute path
    let absolute_path = temp_dir.path().join("absolute_output");
    let result = get_test_validated_config_result(&temp_dir, |builder| {
        builder.output_folder(absolute_path.clone());
    });
    assert!(result.is_ok());
    let config = result.unwrap();
    assert!(config.ignore_folders().unwrap().contains(&absolute_path));

    // Test with relative path
    let result = get_test_validated_config_result(&temp_dir, |builder| {
        builder.output_folder(PathBuf::from("relative_output"));
    });
    assert!(result.is_ok());

    // Test that output folder is properly resolved and added to ignore folders
    let config = result.unwrap();
    let expected_path = temp_dir.path().join("relative_output");
    assert!(
        config.ignore_folders().unwrap().contains(&expected_path),
        "\nExpected path: {:?}\nIgnore folders: {:?}",
        expected_path,
        config.ignore_folders().unwrap()
    );
}

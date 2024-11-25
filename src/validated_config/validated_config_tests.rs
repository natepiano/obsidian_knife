use std::fs;
use crate::validated_config::ValidatedConfigBuilder;
use std::path::PathBuf;

use super::*;
use tempfile::TempDir;
use crate::config::Config;

#[test]
fn test_back_populate_file_filter() {
    let temp_dir = TempDir::new().unwrap();
    let config = ValidatedConfigBuilder::default()
        .obsidian_path(temp_dir.path().to_path_buf())
        .output_folder(temp_dir.path().join("output"))
        .back_populate_file_filter(Some("test_file".to_string()))
        .build()
        .unwrap();

    assert_eq!(
        config.back_populate_file_filter(),
        Some("test_file.md".to_string())
    );

    // Test with wikilink format
    let config = ValidatedConfigBuilder::default()
        .apply_changes(false)
        .back_populate_file_filter(Some("[[test_file]]".to_string()))
        .obsidian_path(temp_dir.path().to_path_buf())
        .output_folder(temp_dir.path().join("output"))
        .build()
        .unwrap();

    assert_eq!(
        config.back_populate_file_filter(),
        Some("test_file.md".to_string())
    );

    // Test with existing .md extension
    let config = ValidatedConfigBuilder::default()
        .apply_changes(false)
        .back_populate_file_filter(Some("test_file.md".to_string()))
        .obsidian_path(temp_dir.path().to_path_buf())
        .output_folder(temp_dir.path().join("output"))
        .build()
        .unwrap();

    assert_eq!(
        config.back_populate_file_filter(),
        Some("test_file.md".to_string())
    );

    // Test with wikilink and .md extension
    let config = ValidatedConfigBuilder::default()
        .apply_changes(false)
        .back_populate_file_filter(Some("[[test_file.md]]".to_string()))
        .obsidian_path(temp_dir.path().to_path_buf())
        .output_folder(temp_dir.path().join("output"))
        .build()
        .unwrap();

    assert_eq!(
        config.back_populate_file_filter(),
        Some("test_file.md".to_string())
    );

    // Test with None
    let config = ValidatedConfigBuilder::default()
        .apply_changes(false)
        .obsidian_path(temp_dir.path().to_path_buf())
        .output_folder(temp_dir.path().join("output"))
        .build()
        .unwrap();

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

    // Print folders for debugging if test fails
    println!("Ignore folders: {:?}", ignore_folders);
    println!("Looking for obsidian_dir: {:?}", obsidian_dir);
    println!("Looking for output_dir: {:?}", output_dir);
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

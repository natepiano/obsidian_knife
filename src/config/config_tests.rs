use crate::config::Config;
use crate::frontmatter::FrontMatter;
use crate::markdown_file_info::MarkdownFileInfo;
use crate::test_utils::{get_test_markdown_file_info, TestFileBuilder};
use crate::yaml_frontmatter::YamlFrontMatter;
use crate::{DEFAULT_TIMEZONE, ERROR_NOT_FOUND};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn create_test_environment() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().unwrap();

    // Create Obsidian vault structure
    let obsidian_path = temp_dir.path().join("vault");
    fs::create_dir(&obsidian_path).unwrap();
    fs::create_dir(obsidian_path.join(".obsidian")).unwrap();

    let canonical_path = obsidian_path
        .canonicalize()
        .expect("Failed to get canonical path");

    // Create output directory
    fs::create_dir(canonical_path.join("output")).unwrap();

    // Create config file using TestFileBuilder
    let config_yaml = format!(
        "obsidian_path: {}\napply_changes: false\noutput_folder: output",
        canonical_path.to_string_lossy()
    );

    let config_path = TestFileBuilder::new()
        .with_custom_frontmatter(config_yaml)
        .create(&temp_dir, "config.md");

    assert!(
        config_path.exists(),
        "Config file does not exist after creation"
    );

    (temp_dir, config_path)
}

#[test]
fn test_reset_apply_changes() {
    let temp_dir = TempDir::new().unwrap();
    let yaml = r#"
obsidian_path: /test/path
apply_changes: true
file_process_limit: 5
back_populate_file_filter: "*test*"
do_not_back_populate:
 - "*.png"
ignore_folders:
 - .git
output_folder: output"#;

    let config_path = TestFileBuilder::new()
        .with_custom_frontmatter(yaml.to_string())
        .create(&temp_dir, "config.md");

    let mut markdown_file = get_test_markdown_file_info(config_path.clone());
    let mut config = Config::from_frontmatter(markdown_file.frontmatter.clone().unwrap()).unwrap();

    // Validate initial values
    assert_eq!(config.apply_changes, Some(true));
    assert_eq!(config.file_process_limit, Some(5));
    assert_eq!(config.back_populate_file_filter, Some("*test*".to_string()));
    assert_eq!(config.do_not_back_populate, Some(vec!["*.png".to_string()]));
    assert_eq!(config.ignore_folders, Some(vec![PathBuf::from(".git")]));
    assert_eq!(config.output_folder, Some("output".to_string()));
    assert_eq!(config.obsidian_path, "/test/path".to_string());

    // Test apply_changes update
    config.apply_changes = Some(false);
    let config_yaml = config.to_yaml_str().unwrap();

    let updated_frontmatter = FrontMatter::from_yaml_str(&config_yaml).unwrap();
    markdown_file.frontmatter = Some(updated_frontmatter);
    markdown_file
        .frontmatter
        .as_mut()
        .unwrap()
        .set_date_modified_now();
    markdown_file.persist().unwrap();

    // Verify all fields after update
    let new_markdown_file = get_test_markdown_file_info(config_path.clone());
    let new_config = Config::from_frontmatter(new_markdown_file.frontmatter.unwrap()).unwrap();

    assert_eq!(new_config.apply_changes, Some(false));
    assert_eq!(new_config.file_process_limit, Some(5));
    assert_eq!(
        new_config.back_populate_file_filter,
        Some("*test*".to_string())
    );
    assert_eq!(
        new_config.do_not_back_populate,
        Some(vec!["*.png".to_string()])
    );
    assert_eq!(new_config.ignore_folders, Some(vec![PathBuf::from(".git")]));
    assert_eq!(new_config.output_folder, Some("output".to_string()));
    assert_eq!(new_config.obsidian_path, "/test/path".to_string());
}

#[test]
fn test_config_from_markdown() {
    let temp_dir = TempDir::new().unwrap();

    let yaml = r#"
obsidian_path: ~/Documents/brain
apply_changes: false
cleanup_image_files: true"#;

    let config_path = TestFileBuilder::new()
        .with_custom_frontmatter(yaml.to_string())
        .create(&temp_dir, "config.md");

    let markdown_file = get_test_markdown_file_info(config_path);
    let config = Config::from_frontmatter(markdown_file.frontmatter.unwrap()).unwrap();

    assert_eq!(config.obsidian_path, "~/Documents/brain");
    assert_eq!(config.apply_changes, Some(false));
}

#[test]
fn test_config_file_not_found() {
    let nonexistent_path = PathBuf::from("nonexistent/config.md");
    let result = MarkdownFileInfo::new(nonexistent_path.clone(), DEFAULT_TIMEZONE);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains(&format!(
        "{}{}",
        ERROR_NOT_FOUND,
        nonexistent_path.display()
    )));
}

#[test]
fn test_config_invalid_yaml() {
    let temp_dir = TempDir::new().unwrap();
    let invalid_yaml = r#"---
invalid: yaml: content:
---
"#;

    let config_path = TestFileBuilder::new()
        .with_content(invalid_yaml.to_string())
        .create(&temp_dir, "config.md");

    let markdown_file = get_test_markdown_file_info(config_path);
    let result = Config::from_frontmatter(markdown_file.frontmatter.unwrap_or_default());

    assert!(result.is_err());
}

#[test]
fn test_config_with_output_folder() {
    let yaml = r#"
obsidian_path: ~/Documents/brain
output_folder: custom_output
apply_changes: false"#;

    let config: Config = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.output_folder, Some("custom_output".to_string()));
}

#[test]
fn test_config_without_output_folder() {
    let yaml = r#"
obsidian_path: ~/Documents/brain
apply_changes: false"#;

    let config: Config = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.output_folder, None);
}

#[test]
fn test_process_config_with_valid_setup() {
    let (_temp_dir, config_path) = create_test_environment();

    let markdown_file = get_test_markdown_file_info(config_path);
    let config = Config::from_frontmatter(markdown_file.frontmatter.unwrap()).unwrap();

    let validated_config = config.validate().unwrap();
    assert!(!validated_config.apply_changes());
    assert!(validated_config.obsidian_path().exists());
}

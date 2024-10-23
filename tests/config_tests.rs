use obsidian_knife::Config;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn example_config() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("example_config.md")
}

fn create_test_folders(temp_dir: &TempDir) {
    for folder in [".idea", ".obsidian", "conf/templates"].iter() {
        fs::create_dir_all(temp_dir.path().join(folder)).unwrap();
    }
}

fn setup_test_config(temp_dir: &TempDir, obsidian_path: &Path) -> PathBuf {
    let config_content = fs::read_to_string(example_config()).unwrap();
    let temp_config_path = temp_dir.path().join("example_config.md");

    let modified_content = config_content.replace(
        "obsidian_path: ~/Documents/brain",
        &format!("obsidian_path: {}", obsidian_path.display()),
    );

    fs::write(&temp_config_path, modified_content).unwrap();
    temp_config_path
}

#[test]
fn test_validate_example_config() {
    let temp_dir = TempDir::new().unwrap();
    create_test_folders(&temp_dir);

    let config_path = setup_test_config(&temp_dir, temp_dir.path());
    let config = Config::from_obsidian_file(&config_path).unwrap();
    let validated_config = config.validate().unwrap();

    // Test the validated configuration
    assert!(!validated_config.apply_changes());

    // Check ignore folders
    let ignore_folders = validated_config.ignore_folders().unwrap();
    assert_eq!(ignore_folders.len(), 5); // 3 original + output folder + cache folder

    // Check each folder exists in ignore list
    for folder in [".idea", ".obsidian", "conf/templates"].iter() {
        let full_path = temp_dir.path().join(folder);
        assert!(ignore_folders.contains(&full_path));
    }

    // Verify output folder is in ignore list
    let output_path = validated_config.output_folder();
    assert!(ignore_folders.contains(&output_path.to_path_buf()));

    // Verify cache folder is in ignore list
    let cache_path = temp_dir.path().join(".obsidian_knife");
    assert!(ignore_folders.contains(&cache_path));

    // Test ignore_text patterns
    let ignore_text = validated_config.ignore_text().unwrap();
    assert_eq!(ignore_text.len(), 1);
    assert_eq!(ignore_text[0], "Ed: music reco:");

    // Test simplify_wikilinks patterns
    let simplify_patterns = validated_config.simplify_wikilinks().unwrap();
    assert_eq!(simplify_patterns.len(), 2);
    assert_eq!(simplify_patterns[0], "Ed:");
    assert_eq!(simplify_patterns[1], "Bob Rock");
}

#[test]
fn test_validate_example_config_without_folders() {
    let temp_dir = TempDir::new().unwrap();
    create_test_folders(&temp_dir);

    let non_existent_path = temp_dir.path().join("does_not_exist");
    let config_path = setup_test_config(&temp_dir, &non_existent_path);

    let config = Config::from_obsidian_file(&config_path).unwrap();
    let result = config.validate();

    assert!(result.is_err());
    let error = result.unwrap_err().to_string();

    assert!(error.contains("does_not_exist"));
}

#[test]
fn test_example_config_creation_date_property() {
    let temp_dir = TempDir::new().unwrap();
    create_test_folders(&temp_dir);
    let config_path = temp_dir.path().join("example_config.md");

    // Copy example config and modify the path to point to our temp directory
    let mut example_config = std::fs::read_to_string("tests/data/example_config.md").unwrap();
    example_config = example_config.replace(
        "obsidian_path: ~/Documents/brain",
        &format!("obsidian_path: {}", temp_dir.path().display()),
    );

    File::create(&config_path)
        .unwrap()
        .write_all(example_config.as_bytes())
        .unwrap();

    let config = Config::from_obsidian_file(&config_path).unwrap();
    let validated = config.validate();
    assert!(validated.is_ok());
    assert_eq!(
        validated.unwrap().creation_date_property(),
        Some("date_onenote")
    );
}

#[test]
fn test_config_integration_with_filesystem() {
    let temp_dir = TempDir::new().unwrap();

    // Create test directories
    fs::create_dir_all(temp_dir.path().join(".obsidian")).unwrap();

    // Test that output folder is properly added to ignore list
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

    // Verify both filesystem integration points
    assert!(ignore_folders.contains(&output_path.to_path_buf()));
    assert!(ignore_folders.contains(&temp_dir.path().join(".obsidian")));
}

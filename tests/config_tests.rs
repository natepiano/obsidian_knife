use obsidian_knife::Config;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn example_config() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("example_config.md")
}

#[test]
fn test_validate_example_config() {
    // Create a temporary directory for our test "obsidian vault"
    let temp_dir = TempDir::new().unwrap();

    // Create the required folders from our example config
    for folder in [".idea", ".obsidian", "conf/templates"].iter() {
        fs::create_dir_all(temp_dir.path().join(folder)).unwrap();
    }

    let config_content = fs::read_to_string(example_config()).unwrap();

    // Create a temporary config file with the content, replacing the obsidian_path
    let temp_config_path = temp_dir.path().join("config.md");
    let modified_content = config_content.replace(
        "obsidian_path: ~/Documents/brain",
        &format!("obsidian_path: {}", temp_dir.path().to_string_lossy()),
    );

    let mut temp_config_file = File::create(&temp_config_path).unwrap();
    temp_config_file.write_all(modified_content.as_bytes()).unwrap();

    // Parse and validate the config
    let config = Config::from_obsidian_file(&temp_config_path).unwrap();
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
    // Create a temporary directory for our test "obsidian vault"
    let temp_dir = TempDir::new().unwrap();

    // Create the required folders from our example config
    for folder in [".idea", ".obsidian", "conf/templates"].iter() {
        fs::create_dir_all(temp_dir.path().join(folder)).unwrap();
    }

    // Read the example config
    let config_content = fs::read_to_string(example_config()).unwrap();

    // Create a temporary config file, but point it to a non-existent directory
    let temp_config_path = temp_dir.path().join("example_config.md");
    let non_existent_path = temp_dir.path().join("does_not_exist");
    let modified_content = config_content.replace(
        "obsidian_path: ~/Documents/brain",
        &format!("obsidian_path: {}", non_existent_path.display()),
    );

    fs::write(&temp_config_path, modified_content).unwrap();

    // Parse and validate the config
    let config = Config::from_obsidian_file(&temp_config_path).unwrap();
    let result = config.validate();

    // Should fail because the Obsidian vault path doesn't exist
    assert!(result.is_err());
    let error = result.unwrap_err().to_string();

    println!("Actual error message: {}", error);
    assert!(error.contains("does_not_exist"));
}

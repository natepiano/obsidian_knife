use super::*;
use crate::test_utils::{eastern_midnight, get_test_markdown_file, TestFileBuilder};
use chrono::{Datelike, Utc};
use tempfile::TempDir;

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_update_modified_dates_changes_frontmatter() {
    let temp_dir = TempDir::new().unwrap();

    let base_date = eastern_midnight(2024, 1, 15);

    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-01-15]]".to_string()),
            Some("[[2024-01-15]]".to_string()),
        )
        .with_fs_dates(base_date, base_date)
        .create(&temp_dir, "test1.md");

    let mut repository = ObsidianRepository::default();
    let mut markdown_file = get_test_markdown_file(file_path.clone());
    markdown_file.mark_image_reference_as_updated();

    repository.markdown_files.push(markdown_file);

    let frontmatter = repository.markdown_files[0].frontmatter.as_ref().unwrap();

    // Get today's date for comparison
    let today = Utc::now();
    let expected_date = format!(
        "[[{}-{:02}-{:02}]]",
        today.year(),
        today.month(),
        today.day()
    );

    assert_eq!(
        frontmatter.date_modified(),
        Some(&expected_date),
        "Modified date should be today's date"
    );
    assert_eq!(
        frontmatter.date_created(),
        Some(&"[[2024-01-15]]".to_string()),
        "Created date should not have changed"
    );
    assert!(frontmatter.needs_persist(), "needs_persist should be true");
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_update_modified_dates_only_updates_specified_files() {
    let temp_dir = TempDir::new().unwrap();

    // Set January 15th, 2024 as the base date
    let base_date = eastern_midnight(2024, 1, 15);

    // Create two files
    let file_path1 = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-01-15]]".to_string()),
            Some("[[2024-01-15]]".to_string()),
        )
        .with_fs_dates(base_date, base_date)
        .create(&temp_dir, "test1.md");
    let file_path2 = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-01-15]]".to_string()),
            Some("[[2024-01-15]]".to_string()),
        )
        .with_fs_dates(base_date, base_date)
        .create(&temp_dir, "test2.md");

    let mut repository = ObsidianRepository::default();
    let mut markdown_file1 = get_test_markdown_file(file_path1.clone());

    // Only update the first file
    markdown_file1.mark_image_reference_as_updated();

    repository.markdown_files.push(markdown_file1);
    repository
        .markdown_files
        .push(get_test_markdown_file(file_path2.clone()));

    let file1 = &repository.markdown_files[0];
    let file2 = &repository.markdown_files[1];

    // Get today's date for comparison
    let today = Utc::now();
    let expected_date = format!(
        "[[{}-{:02}-{:02}]]",
        today.year(),
        today.month(),
        today.day()
    );

    // First file should have new date and needs_persist
    assert_eq!(
        file1.frontmatter.as_ref().unwrap().date_modified(),
        Some(&expected_date)
    );
    assert!(file1.frontmatter.as_ref().unwrap().needs_persist());

    // Second file should have original date and not need persist
    assert_eq!(
        file2.frontmatter.as_ref().unwrap().date_modified(),
        Some(&"[[2024-01-15]]".to_string())
    );
    assert!(!file2.frontmatter.as_ref().unwrap().needs_persist());
}

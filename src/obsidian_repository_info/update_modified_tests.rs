use super::*;
use crate::test_utils::{eastern_midnight, get_test_markdown_file_info, TestFileBuilder};
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

    let mut repo_info = ObsidianRepositoryInfo::default();
    let markdown_file = get_test_markdown_file_info(file_path.clone());
    repo_info.markdown_files.push(markdown_file);

    // Update the modified dates
    repo_info.update_modified_dates_for_cleanup_images(&[file_path.clone()]);

    let frontmatter = repo_info.markdown_files[0].frontmatter.as_ref().unwrap();

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

    let mut repo_info = ObsidianRepositoryInfo::default();
    repo_info
        .markdown_files
        .push(get_test_markdown_file_info(file_path1.clone()));
    repo_info
        .markdown_files
        .push(get_test_markdown_file_info(file_path2.clone()));

    // Only update the first file
    repo_info.update_modified_dates_for_cleanup_images(&[file_path1]);

    let file1 = &repo_info.markdown_files[0];
    let file2 = &repo_info.markdown_files[1];

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

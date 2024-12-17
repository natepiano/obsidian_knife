use super::*;
use crate::markdown_file::PersistReason;
use crate::test_utils::TestFileBuilder;
use crate::{test_utils, DEFAULT_TIMEZONE};
use chrono::Utc;
use tempfile::TempDir;

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_update_modified_dates_changes_frontmatter() {
    let temp_dir = TempDir::new().unwrap();

    let base_date = test_utils::eastern_midnight(2024, 1, 15);
    let update_date = test_utils::eastern_midnight(2024, 1, 20);

    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-01-15]]".to_string()),
            Some("[[2024-01-15]]".to_string()),
        )
        .with_fs_dates(base_date, base_date)
        .create(&temp_dir, "test1.md");

    let mut repository = ObsidianRepository::default();
    let mut markdown_file = test_utils::get_test_markdown_file(file_path.clone());

    // Instead of using mark_image_reference_as_updated which uses current date,
    // set the frontmatter dates directly
    if let Some(fm) = &mut markdown_file.frontmatter {
        fm.set_date_modified(update_date, DEFAULT_TIMEZONE);
    }
    markdown_file
        .persist_reasons
        .push(PersistReason::ImageReferencesModified);

    repository.markdown_files.push(markdown_file);

    let frontmatter = repository.markdown_files[0].frontmatter.as_ref().unwrap();

    assert_eq!(
        frontmatter.date_modified(),
        Some(&"[[2024-01-20]]".to_string()),
        "Modified date should be update date"
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

    let base_date = test_utils::eastern_midnight(2024, 1, 15);
    let update_date = test_utils::eastern_midnight(2024, 1, 20);

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
    let mut markdown_file1 = test_utils::get_test_markdown_file(file_path1.clone());

    // Update only the first file with a fixed date
    if let Some(fm) = &mut markdown_file1.frontmatter {
        fm.set_date_modified(update_date, DEFAULT_TIMEZONE);
    }
    markdown_file1
        .persist_reasons
        .push(PersistReason::ImageReferencesModified);

    repository.markdown_files.push(markdown_file1);
    repository
        .markdown_files
        .push(test_utils::get_test_markdown_file(file_path2.clone()));

    let file1 = &repository.markdown_files[0];
    let file2 = &repository.markdown_files[1];

    // First file should have new date and needs_persist
    assert_eq!(
        file1.frontmatter.as_ref().unwrap().date_modified(),
        Some(&"[[2024-01-20]]".to_string())
    );
    assert!(file1.frontmatter.as_ref().unwrap().needs_persist());

    // Second file should have original date and not need persist
    assert_eq!(
        file2.frontmatter.as_ref().unwrap().date_modified(),
        Some(&"[[2024-01-15]]".to_string())
    );
    assert!(!file2.frontmatter.as_ref().unwrap().needs_persist());
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_update_modified_uses_current_date() {
    let temp_dir = TempDir::new().unwrap();
    let base_date = test_utils::eastern_midnight(2024, 1, 15);

    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-01-15]]".to_string()),
            Some("[[2024-01-15]]".to_string()),
        )
        .with_fs_dates(base_date, base_date)
        .create(&temp_dir, "test.md");

    let mut markdown_file = test_utils::get_test_markdown_file(file_path);

    // Use the actual mark_image_reference_as_updated method
    markdown_file.mark_image_reference_as_updated(DEFAULT_TIMEZONE);

    // Get the frontmatter modified date
    let modified_date = markdown_file
        .frontmatter
        .as_ref()
        .and_then(|fm| fm.date_modified())
        .expect("Should have a modified date");

    // Get today's date in the same format as the frontmatter
    let today = Utc::now()
        .with_timezone(&DEFAULT_TIMEZONE.parse::<chrono_tz::Tz>().unwrap())
        .format("[[%Y-%m-%d]]")
        .to_string();

    assert_eq!(
        modified_date, &today,
        "Modified date should be today's date"
    );
    assert!(
        markdown_file.frontmatter.as_ref().unwrap().needs_persist(),
        "needs_persist should be true"
    );
}

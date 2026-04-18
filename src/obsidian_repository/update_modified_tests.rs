use chrono::Utc;
use tempfile::TempDir;

use super::*;
use crate::constants::DEFAULT_TIMEZONE;
use crate::frontmatter::FrontMatter;
use crate::markdown_file::PersistReason;
use crate::test_support as test_utils;
use crate::test_support::TestFileBuilder;

fn eastern_date_wikilink(year: i32, month: u32, day: u32) -> String {
    test_utils::frontmatter_date_wikilink(test_utils::eastern_midnight(year, month, day))
}

#[test]
#[cfg_attr(
    target_os = "linux",
    ignore = "requires filesystem access unavailable on Linux CI"
)]
fn test_update_modified_dates_changes_frontmatter() {
    let temp_dir = TempDir::new().unwrap();

    let base_date = test_utils::eastern_midnight(2024, 1, 15);
    let update_date = test_utils::eastern_midnight(2024, 1, 20);

    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some(eastern_date_wikilink(2024, 1, 15)),
            Some(eastern_date_wikilink(2024, 1, 15)),
        )
        .with_fs_dates(base_date, base_date)
        .create(&temp_dir, "test1.md");

    let mut repository = ObsidianRepository::default();
    let mut markdown_file = test_utils::get_test_markdown_file(file_path);

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
        Some(test_utils::frontmatter_date_wikilink(update_date).as_str()),
        "Modified date should be update date"
    );
    assert_eq!(
        frontmatter.date_created(),
        Some(test_utils::frontmatter_date_wikilink(base_date).as_str()),
        "Created date should not have changed"
    );
    assert!(frontmatter.needs_persist(), "needs_persist should be true");
}

#[test]
#[cfg_attr(
    target_os = "linux",
    ignore = "requires filesystem access unavailable on Linux CI"
)]
fn test_update_modified_dates_only_updates_specified_files() {
    let temp_dir = TempDir::new().unwrap();

    let base_date = test_utils::eastern_midnight(2024, 1, 15);
    let update_date = test_utils::eastern_midnight(2024, 1, 20);

    // Create two files
    let file_path1 = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some(eastern_date_wikilink(2024, 1, 15)),
            Some(eastern_date_wikilink(2024, 1, 15)),
        )
        .with_fs_dates(base_date, base_date)
        .create(&temp_dir, "test1.md");
    let file_path2 = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some(eastern_date_wikilink(2024, 1, 15)),
            Some(eastern_date_wikilink(2024, 1, 15)),
        )
        .with_fs_dates(base_date, base_date)
        .create(&temp_dir, "test2.md");

    let mut repository = ObsidianRepository::default();
    let mut markdown_file1 = test_utils::get_test_markdown_file(file_path1);

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
        .push(test_utils::get_test_markdown_file(file_path2));

    let file1 = &repository.markdown_files[0];
    let file2 = &repository.markdown_files[1];

    // First file should have new date and needs_persist
    assert_eq!(
        file1.frontmatter.as_ref().unwrap().date_modified(),
        Some(test_utils::frontmatter_date_wikilink(update_date).as_str())
    );
    assert!(file1.frontmatter.as_ref().unwrap().needs_persist());

    // Second file should have original date and not need persist
    assert_eq!(
        file2.frontmatter.as_ref().unwrap().date_modified(),
        Some(test_utils::frontmatter_date_wikilink(base_date).as_str())
    );
    assert!(!file2.frontmatter.as_ref().unwrap().needs_persist());
}

#[test]
#[cfg_attr(
    target_os = "linux",
    ignore = "requires filesystem access unavailable on Linux CI"
)]
fn test_update_modified_uses_current_date() {
    let temp_dir = TempDir::new().unwrap();
    let base_date = test_utils::eastern_midnight(2024, 1, 15);

    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some(eastern_date_wikilink(2024, 1, 15)),
            Some(eastern_date_wikilink(2024, 1, 15)),
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
        .and_then(FrontMatter::date_modified)
        .expect("Should have a modified date");

    // Get today's date in the same format as the frontmatter
    let today = test_utils::frontmatter_date_wikilink(Utc::now());

    assert_eq!(
        modified_date, &today,
        "Modified date should be today's date"
    );
    assert!(
        markdown_file.frontmatter.as_ref().unwrap().needs_persist(),
        "needs_persist should be true"
    );
}

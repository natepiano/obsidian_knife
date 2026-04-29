use std::error::Error;
use std::fs;

use filetime::FileTime;
use tempfile::TempDir;

use super::DateValidationIssue;
use super::PersistReason;
use crate::constants::DEFAULT_TIMEZONE;
use crate::test_support as test_utils;
use crate::test_support::TestFileBuilder;

#[test]
fn test_date_validation_persist_reasons() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    // Test missing dates
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(None, None)
        .with_title("test".to_string()) // to force valid frontmatter with missing dates
        .create(&temp_dir, "missing_dates.md");

    let markdown_file = test_utils::get_test_markdown_file(file_path);

    assert!(
        markdown_file
            .persist_reasons
            .contains(&PersistReason::DateCreatedUpdated {
                reason: DateValidationIssue::Missing,
            })
    );
    assert!(
        markdown_file
            .persist_reasons
            .contains(&PersistReason::DateModifiedUpdated {
                reason: DateValidationIssue::Missing,
            })
    );

    // Test invalid format dates
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-13-45]]".to_string()),
            Some("[[2024-13-45]]".to_string()),
        )
        .create(&temp_dir, "invalid_dates.md");

    let markdown_file = test_utils::get_test_markdown_file(file_path);

    assert!(
        markdown_file
            .persist_reasons
            .contains(&PersistReason::DateCreatedUpdated {
                reason: DateValidationIssue::InvalidDateFormat,
            })
    );
    assert!(
        markdown_file
            .persist_reasons
            .contains(&PersistReason::DateModifiedUpdated {
                reason: DateValidationIssue::InvalidDateFormat,
            })
    );

    Ok(())
}

#[test]
fn test_date_created_fix_persist_reason() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let test_date = test_utils::eastern_midnight(2024, 1, 15);

    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-01-15]]".to_string()),
            Some("[[2024-01-15]]".to_string()),
        )
        .with_fs_dates(test_date, test_date)
        .with_date_created_fix(Some("2024-01-01".to_string()))
        .create(&temp_dir, "date_fix.md");

    let markdown_file = test_utils::get_test_markdown_file(file_path);

    assert!(
        markdown_file
            .persist_reasons
            .contains(&PersistReason::DateCreatedFixApplied)
    );

    Ok(())
}

#[test]
fn test_back_populate_persist_reason() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-01-15]]".to_string()),
            Some("[[2024-01-15]]".to_string()),
        )
        .create(&temp_dir, "back_populate.md");

    let mut markdown_file = test_utils::get_test_markdown_file(file_path);
    markdown_file.mark_as_back_populated(DEFAULT_TIMEZONE)?;

    assert!(
        markdown_file
            .persist_reasons
            .contains(&PersistReason::BackPopulated)
    );

    Ok(())
}

#[test]
fn test_image_references_persist_reason() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-01-15]]".to_string()),
            Some("[[2024-01-15]]".to_string()),
        )
        .create(&temp_dir, "image_refs.md");

    let mut markdown_file = test_utils::get_test_markdown_file(file_path);
    markdown_file.mark_image_reference_as_updated(DEFAULT_TIMEZONE)?;

    assert!(
        markdown_file
            .persist_reasons
            .contains(&PersistReason::ImageReferencesModified)
    );

    Ok(())
}

#[test]
fn test_multiple_persist_reasons() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(None, None)
        .with_title("test".to_string()) // to force frontmatter creation
        .create(&temp_dir, "multiple_reasons.md");

    let mut markdown_file = test_utils::get_test_markdown_file(file_path);

    // This will add DateCreatedUpdated and DateModifiedUpdated
    assert!(
        markdown_file
            .persist_reasons
            .contains(&PersistReason::DateCreatedUpdated {
                reason: DateValidationIssue::Missing,
            })
    );

    // Add back populate reason
    markdown_file.mark_as_back_populated(DEFAULT_TIMEZONE)?;

    // Add image reference change
    markdown_file.mark_image_reference_as_updated(DEFAULT_TIMEZONE)?;

    // Verify all reasons are present
    // the 3 reasons are DateCreatedUpdated { reason: Missing }, BackPopulated,
    // ImageReferencesModified we don't have an update date missing because if we BackPopulate
    // we automatically remove the modified date reason
    assert_eq!(markdown_file.persist_reasons.len(), 3);
    assert!(
        markdown_file
            .persist_reasons
            .contains(&PersistReason::BackPopulated)
    );
    assert!(
        markdown_file
            .persist_reasons
            .contains(&PersistReason::ImageReferencesModified)
    );

    Ok(())
}

#[test]
fn test_persist_frontmatter() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(Some("2024-01-01".to_string()), None)
        .create(&temp_dir, "test.md");

    let mut markdown_file = test_utils::get_test_markdown_file(file_path.clone());

    // Update frontmatter directly
    if let Some(frontmatter) = &mut markdown_file.frontmatter {
        let created_date = test_utils::eastern_midnight(2024, 1, 2); // Instead of parse_datetime
        frontmatter.set_date_created(created_date, DEFAULT_TIMEZONE);
    }

    markdown_file.persist()?;

    // Verify frontmatter was updated but content preserved
    let updated_content = fs::read_to_string(&file_path)?;
    assert!(
        updated_content.contains("[[2024-01-02]]"),
        "Content '{updated_content}' does not contain expected date string"
    );
    assert!(updated_content.contains("Test content"));

    Ok(())
}

#[test]
fn test_persist_frontmatter_preserves_format() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(Some("2024-01-01".to_string()), None)
        .with_tags(vec!["tag1".to_string(), "tag2".to_string()])
        .create(&temp_dir, "test.md");

    let mut markdown_file = test_utils::get_test_markdown_file(file_path.clone());

    if let Some(frontmatter) = &mut markdown_file.frontmatter {
        let created_date = test_utils::eastern_midnight(2024, 1, 2); // Instead of parse_datetime
        frontmatter.set_date_created(created_date, DEFAULT_TIMEZONE);
    }

    markdown_file.persist()?;

    let updated_content = fs::read_to_string(&file_path)?;
    assert!(updated_content.contains("tags:\n- tag1\n- tag2"));
    assert!(updated_content.contains("[[2024-01-02]]"));

    Ok(())
}

#[test]
#[cfg_attr(
    target_os = "linux",
    ignore = "requires filesystem access unavailable on Linux CI"
)]
fn test_persist_with_created_and_modified_dates() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    // Define the created and modified dates
    let created_date = test_utils::parse_datetime("2024-01-05 10:00:00");
    let modified_date = test_utils::parse_datetime("2024-01-06 15:30:00");

    // Use with_matching_dates to set both frontmatter and file system dates
    let file_path = TestFileBuilder::new()
        .with_matching_dates(created_date) // Set both FS and frontmatter dates to created_date
        .create(&temp_dir, "test_with_both_dates.md");

    let mut markdown_file = test_utils::get_test_markdown_file(file_path.clone());

    if let Some(frontmatter) = &mut markdown_file.frontmatter {
        // Update the frontmatter to match the intended created and modified dates
        frontmatter.raw_created = Some(created_date);
        frontmatter.raw_modified = Some(modified_date);
        frontmatter.set_date_created(created_date, DEFAULT_TIMEZONE); // Ensure frontmatter reflects this change
        frontmatter.set_date_modified(modified_date, DEFAULT_TIMEZONE);
    }

    markdown_file.persist()?;

    let metadata_after = fs::metadata(&file_path)?;
    let created_time_after = FileTime::from_creation_time(&metadata_after).unwrap();
    let modified_time_after = FileTime::from_last_modification_time(&metadata_after);

    assert_eq!(created_time_after.unix_seconds(), created_date.timestamp());
    assert_eq!(
        modified_time_after.unix_seconds(),
        modified_date.timestamp()
    );

    Ok(())
}

#[test]
fn test_disallow_persist_if_date_modified_not_set() {
    let temp_dir = TempDir::new().unwrap();

    // Use with_matching_dates to set consistent creation and modification dates
    let matching_date = test_utils::eastern_midnight(2024, 1, 1); // ("2024-01-01 00:00:00");
    let file_path = TestFileBuilder::new()
        .with_matching_dates(matching_date)
        .create(&temp_dir, "test_invalid_state.md");

    let mut markdown_file = test_utils::get_test_markdown_file(file_path);

    // Simulate the absence of `raw_date_modified` by explicitly removing it
    if let Some(frontmatter) = &mut markdown_file.frontmatter {
        frontmatter.raw_modified = None;
    }

    // Attempt to persist and expect an error
    let result = markdown_file.persist();

    assert!(
        result.is_err(),
        "Expected an error, but persist completed successfully"
    );

    if let Err(err) = result {
        assert_eq!(
            err.to_string(),
            "raw_date_modified must be set for persist",
            "Unexpected error message"
        );
    }
}

#[test]
fn test_persist_preserves_file_content() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = TestFileBuilder::new()
        .with_title("Test Title".to_string())
        .with_content("Sample content for testing")
        .with_frontmatter_dates(
            Some("2024-01-01".to_string()),
            Some("2024-01-02".to_string()),
        )
        .create(&temp_dir, "test_content_preservation.md");

    let mut markdown_file = test_utils::get_test_markdown_file(file_path.clone());

    if let Some(frontmatter) = &mut markdown_file.frontmatter {
        frontmatter.set_date_created(
            test_utils::parse_datetime("2024-01-03 10:00:00"),
            DEFAULT_TIMEZONE,
        );
        frontmatter.set_date_modified(
            test_utils::parse_datetime("2024-01-04 15:00:00"),
            DEFAULT_TIMEZONE,
        );
    }

    markdown_file.persist()?;

    // Verify that the file content remains unchanged except for the frontmatter
    let updated_content = fs::read_to_string(&file_path)?;
    assert!(updated_content.contains("Sample content for testing"));
    assert!(updated_content.contains("[[2024-01-03]]"));
    assert!(updated_content.contains("[[2024-01-04]]"));

    Ok(())
}

#[test]
fn test_ensure_frontmatter_creates_frontmatter_on_back_populate()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    // Create a file without frontmatter
    let file_path = TestFileBuilder::new()
        .with_content("Some text that mentions a wikilink target")
        .create(&temp_dir, "no_frontmatter.md");

    let mut markdown_file = test_utils::get_test_markdown_file(file_path);

    // Confirm starting state: no frontmatter, has frontmatter error
    assert!(markdown_file.frontmatter.is_none());
    assert!(markdown_file.frontmatter_error.is_some());

    markdown_file.mark_as_back_populated(DEFAULT_TIMEZONE)?;

    // Frontmatter was created
    assert!(markdown_file.frontmatter.is_some());
    let frontmatter = markdown_file.frontmatter.as_ref().expect("just confirmed");

    // `date_created` set from filesystem date
    assert!(frontmatter.created.is_some());
    assert!(frontmatter.raw_created.is_some());

    // `date_modified` set (by `set_date_created` auto-call and then `set_date_modified_now`)
    assert!(frontmatter.modified.is_some());
    assert!(frontmatter.raw_modified.is_some());

    // Frontmatter error cleared
    assert!(markdown_file.frontmatter_error.is_none());

    // Persist reasons include both `FrontmatterCreated` and `BackPopulated`
    assert!(
        markdown_file
            .persist_reasons
            .contains(&PersistReason::FrontmatterCreated)
    );
    assert!(
        markdown_file
            .persist_reasons
            .contains(&PersistReason::BackPopulated)
    );

    Ok(())
}

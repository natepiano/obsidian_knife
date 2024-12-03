use super::*;
use crate::test_utils::{
    eastern_midnight, get_test_markdown_file_info, parse_datetime, TestFileBuilder,
};
use tempfile::TempDir;

#[test]
fn test_date_validation_persist_reasons() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    // Test missing dates
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(None, None)
        .with_title("test".to_string()) // to force valid frontmatter with missing dates
        .create(&temp_dir, "missing_dates.md");

    let file_info = get_test_markdown_file_info(file_path);

    assert!(file_info
        .persist_reasons
        .contains(&PersistReason::DateCreatedUpdated {
            reason: DateValidationIssue::Missing
        }));
    assert!(file_info
        .persist_reasons
        .contains(&PersistReason::DateModifiedUpdated {
            reason: DateValidationIssue::Missing
        }));

    // Test invalid format dates
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-13-45]]".to_string()),
            Some("[[2024-13-45]]".to_string()),
        )
        .create(&temp_dir, "invalid_dates.md");

    let file_info = get_test_markdown_file_info(file_path);

    assert!(file_info
        .persist_reasons
        .contains(&PersistReason::DateCreatedUpdated {
            reason: DateValidationIssue::InvalidDateFormat
        }));
    assert!(file_info
        .persist_reasons
        .contains(&PersistReason::DateModifiedUpdated {
            reason: DateValidationIssue::InvalidDateFormat
        }));

    Ok(())
}

#[test]
fn test_date_created_fix_persist_reason() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let test_date = eastern_midnight(2024, 1, 15);

    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-01-15]]".to_string()),
            Some("[[2024-01-15]]".to_string()),
        )
        .with_fs_dates(test_date, test_date)
        .with_date_created_fix(Some("2024-01-01".to_string()))
        .create(&temp_dir, "date_fix.md");

    let file_info = get_test_markdown_file_info(file_path);

    assert!(file_info
        .persist_reasons
        .contains(&PersistReason::DateCreatedFixApplied));

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

    let mut file_info = get_test_markdown_file_info(file_path);
    file_info.mark_as_back_populated();

    assert!(file_info
        .persist_reasons
        .contains(&PersistReason::BackPopulated));

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

    let mut file_info = get_test_markdown_file_info(file_path);
    file_info.mark_image_reference_as_updated();

    assert!(file_info
        .persist_reasons
        .contains(&PersistReason::ImageReferencesModified));

    Ok(())
}

#[test]
fn test_multiple_persist_reasons() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(None, None)
        .with_title("test".to_string()) // to force frontmatter creation
        .create(&temp_dir, "multiple_reasons.md");

    let mut file_info = get_test_markdown_file_info(file_path);

    // This will add DateCreatedUpdated and DateModifiedUpdated
    assert!(file_info
        .persist_reasons
        .contains(&PersistReason::DateCreatedUpdated {
            reason: DateValidationIssue::Missing
        }));

    // Add back populate reason
    file_info.mark_as_back_populated();

    // Add image reference change
    file_info.mark_image_reference_as_updated();

    // Verify all reasons are present
    assert_eq!(file_info.persist_reasons.len(), 4);
    assert!(file_info
        .persist_reasons
        .contains(&PersistReason::BackPopulated));
    assert!(file_info
        .persist_reasons
        .contains(&PersistReason::ImageReferencesModified));

    Ok(())
}

#[test]
fn test_persist_frontmatter() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(Some("2024-01-01".to_string()), None)
        .create(&temp_dir, "test.md");

    let mut file_info = get_test_markdown_file_info(file_path.clone());

    // Update frontmatter directly
    if let Some(fm) = &mut file_info.frontmatter {
        let created_date = parse_datetime("2024-01-02 00:00:00");
        fm.set_date_created(created_date);
    }

    file_info.persist()?;

    // Verify frontmatter was updated but content preserved
    let updated_content = fs::read_to_string(&file_path)?;
    assert!(
        updated_content.contains("[[2024-01-02]]"),
        "Content '{}' does not contain expected date string",
        updated_content
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

    let mut file_info = get_test_markdown_file_info(file_path.clone());

    if let Some(fm) = &mut file_info.frontmatter {
        fm.set_date_created(parse_datetime("2024-01-02 00:00:00"));
    }

    file_info.persist()?;

    let updated_content = fs::read_to_string(&file_path)?;
    assert!(updated_content.contains("tags:\n- tag1\n- tag2"));
    assert!(updated_content.contains("[[2024-01-02]]"));

    Ok(())
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_persist_with_missing_raw_date_created() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    // Set up FS dates
    let fs_created = parse_datetime("2024-01-01 00:00:00");
    let fs_modified = parse_datetime("2024-01-01 00:00:00");

    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(None, Some("2024-01-01".to_string()))
        .with_fs_dates(fs_created, fs_modified)
        .create(&temp_dir, "test_missing_created.md");

    let mut file_info = get_test_markdown_file_info(file_path.clone());

    // Assert initial frontmatter matches FS dates
    assert_eq!(
        file_info.frontmatter.as_ref().unwrap().raw_date_created,
        Some(fs_created)
    );
    assert_eq!(
        file_info.frontmatter.as_ref().unwrap().raw_date_modified,
        Some(fs_modified)
    );

    // Update modification date
    let new_modified = parse_datetime("2024-01-03 12:00:00");
    if let Some(fm) = &mut file_info.frontmatter {
        fm.set_date_modified(new_modified);
    }

    // Assert `raw_date_modified` was updated
    assert_eq!(
        file_info.frontmatter.as_ref().unwrap().raw_date_modified,
        Some(new_modified)
    );

    // Persist the file
    file_info.persist()?;

    // Assert FS timestamps
    let metadata = fs::metadata(&file_path)?;
    let created_time = FileTime::from_creation_time(&metadata).unwrap();
    let modified_time = FileTime::from_last_modification_time(&metadata);

    assert_eq!(created_time.unix_seconds(), fs_created.timestamp());
    assert_eq!(modified_time.unix_seconds(), new_modified.timestamp());

    Ok(())
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_persist_with_created_and_modified_dates() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    // Define the created and modified dates
    let created_date = parse_datetime("2024-01-05 10:00:00");
    let modified_date = parse_datetime("2024-01-06 15:30:00");

    // Use with_matching_dates to set both frontmatter and file system dates
    let file_path = TestFileBuilder::new()
        .with_matching_dates(created_date) // Set both FS and frontmatter dates to created_date
        .create(&temp_dir, "test_with_both_dates.md");

    let mut file_info = get_test_markdown_file_info(file_path.clone());

    if let Some(fm) = &mut file_info.frontmatter {
        // Update the frontmatter to match the intended created and modified dates
        fm.raw_date_created = Some(created_date);
        fm.raw_date_modified = Some(modified_date);
        fm.set_date_created(created_date); // Ensure frontmatter reflects this change
        fm.set_date_modified(modified_date);
    }

    file_info.persist()?;

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
    let matching_date = parse_datetime("2024-01-01 00:00:00");
    let file_path = TestFileBuilder::new()
        .with_matching_dates(matching_date)
        .create(&temp_dir, "test_invalid_state.md");

    let mut file_info = get_test_markdown_file_info(file_path);

    // Simulate the absence of `raw_date_modified` by explicitly removing it
    if let Some(fm) = &mut file_info.frontmatter {
        fm.raw_date_modified = None;
    }

    // Attempt to persist and expect an error
    let result = file_info.persist();

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
fn test_persist_no_changes_when_dates_are_valid() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("2024-01-01".to_string()),
            Some("2024-01-02".to_string()),
        )
        .create(&temp_dir, "test_no_changes.md");

    // Set initial creation and modification times
    let created_time = parse_datetime("2024-01-01 00:00:00");
    let modified_time = parse_datetime("2024-01-02 00:00:00");
    filetime::set_file_times(
        &file_path,
        FileTime::from_system_time(created_time.into()),
        FileTime::from_system_time(modified_time.into()),
    )?;

    let mut file_info = get_test_markdown_file_info(file_path.clone());

    if let Some(fm) = &mut file_info.frontmatter {
        fm.set_date_created(created_time);
        fm.set_date_modified(modified_time);
    }

    let metadata_before = fs::metadata(&file_path)?;
    let created_time_before = FileTime::from_creation_time(&metadata_before).unwrap();
    let modified_time_before = FileTime::from_last_modification_time(&metadata_before);

    file_info.persist()?;

    // Verify that file system dates remain unchanged
    let metadata_after = fs::metadata(&file_path)?;
    let created_time_after = FileTime::from_creation_time(&metadata_after).unwrap();
    let modified_time_after = FileTime::from_last_modification_time(&metadata_after);

    assert_eq!(
        created_time_before, created_time_after,
        "Creation time mismatch"
    );
    assert_eq!(
        modified_time_before, modified_time_after,
        "Modification time mismatch"
    );

    Ok(())
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

    let mut file_info = get_test_markdown_file_info(file_path.clone());

    if let Some(fm) = &mut file_info.frontmatter {
        fm.set_date_created(parse_datetime("2024-01-03 10:00:00"));
        fm.set_date_modified(parse_datetime("2024-01-04 15:00:00"));
    }

    file_info.persist()?;

    // Verify that the file content remains unchanged except for the frontmatter
    let updated_content = fs::read_to_string(&file_path)?;
    assert!(updated_content.contains("Sample content for testing"));
    assert!(updated_content.contains("[[2024-01-03]]"));
    assert!(updated_content.contains("[[2024-01-04]]"));

    Ok(())
}

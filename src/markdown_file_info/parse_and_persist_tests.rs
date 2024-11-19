use super::*;
use crate::test_utils::{parse_datetime, TestFileBuilder};
use std::error::Error;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_persist_frontmatter() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(Some("2024-01-01".to_string()), None)
        .create(&temp_dir, "test.md");

    let mut file_info = MarkdownFileInfo::new(file_path.clone())?;

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

    let mut file_info = MarkdownFileInfo::new(file_path.clone())?;

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
fn test_parse_content_separation() {
    let temp_dir = TempDir::new().unwrap();

    // Test 1: File with frontmatter and content
    let file_with_fm = TestFileBuilder::new()
        .with_title("Test".to_string())
        .with_content("This is the actual content")
        .create(&temp_dir, "with_fm.md");

    let mfi = MarkdownFileInfo::new(file_with_fm).unwrap();
    assert_eq!(mfi.content.trim(), "This is the actual content");

    // Test 2: File with no frontmatter
    let file_no_fm = TestFileBuilder::new()
        .with_content("Pure content\nNo frontmatter")
        .create(&temp_dir, "no_fm.md");

    let mfi = MarkdownFileInfo::new(file_no_fm).unwrap();
    assert_eq!(mfi.content.trim(), "Pure content\nNo frontmatter");

    // Test 3: File with --- separators in content
    let content = "First line\n---\nMiddle section\n---\nLast section";
    let file_with_separators = TestFileBuilder::new()
        .with_title("Test".to_string())
        .with_content(content)
        .create(&temp_dir, "with_separators.md");

    let mfi = MarkdownFileInfo::new(file_with_separators).unwrap();
    assert_eq!(mfi.content.trim(), content);
}

#[test]
fn test_persist_with_missing_raw_date_created() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    // Set up FS dates
    let fs_created = parse_datetime("2024-01-01 00:00:00");
    let fs_modified = parse_datetime("2024-01-01 00:00:00");

    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(None, Some("2024-01-01".to_string()))
        .with_fs_dates(fs_created, fs_modified)
        .create(&temp_dir, "test_missing_created.md");

    let mut file_info = MarkdownFileInfo::new(file_path.clone())?;

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
fn test_persist_with_created_and_modified_dates() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    // Define the created and modified dates
    let created_date = parse_datetime("2024-01-05 10:00:00");
    let modified_date = parse_datetime("2024-01-06 15:30:00");

    println!("Expected created timestamp: {}", created_date.timestamp());
    println!("Expected modified timestamp: {}", modified_date.timestamp());

    // Use with_matching_dates to set both frontmatter and file system dates
    let file_path = TestFileBuilder::new()
        .with_matching_dates(created_date) // Set both FS and frontmatter dates to created_date
        .create(&temp_dir, "test_with_both_dates.md");

    let mut file_info = MarkdownFileInfo::new(file_path.clone())?;

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
    dbg!(
        created_time_after.unix_seconds(),
        modified_time_after.unix_seconds()
    );

    assert_eq!(created_time_after.unix_seconds(), created_date.timestamp());
    assert_eq!(
        modified_time_after.unix_seconds(),
        modified_date.timestamp()
    );

    Ok(())
}

#[test]
fn test_persist_missing_raw_date_modified() {
    let temp_dir = TempDir::new().unwrap();

    // Use with_matching_dates to set consistent creation and modification dates
    let matching_date = parse_datetime("2024-01-01 00:00:00");
    let file_path = TestFileBuilder::new()
        .with_matching_dates(matching_date)
        .create(&temp_dir, "test_invalid_state.md");

    let mut file_info = MarkdownFileInfo::new(file_path).unwrap();

    // Simulate the absence of `raw_date_modified` by explicitly removing it
    if let Some(fm) = &mut file_info.frontmatter {
        fm.raw_date_modified = None;
    }

    // Verify that a panic occurs when persist is called
    let result = std::panic::catch_unwind(|| {
        file_info.persist().unwrap();
    });

    // Assert that the operation panicked
    assert!(
        result.is_err(),
        "Expected a panic, but the operation completed successfully"
    );
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

    let mut file_info = MarkdownFileInfo::new(file_path.clone())?;

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

    let mut file_info = MarkdownFileInfo::new(file_path.clone())?;

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

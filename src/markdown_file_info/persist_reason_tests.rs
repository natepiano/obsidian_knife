use super::*;
use crate::test_utils::TestFileBuilder;
use tempfile::TempDir;

#[test]
fn test_date_validation_persist_reasons() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    // Test missing dates
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(None, None)
        .with_title("test".to_string()) // to force valid fronttmatter with missing dates
        .create(&temp_dir, "missing_dates.md");

    let file_info = MarkdownFileInfo::new(file_path)?;

    assert!(file_info.persist_reasons.contains(&PersistReason::DateCreatedUpdated {
        reason: DateValidationIssue::Missing
    }));
    assert!(file_info.persist_reasons.contains(&PersistReason::DateModifiedUpdated {
        reason: DateValidationIssue::Missing
    }));

    // Test invalid format dates
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-13-45]]".to_string()),
            Some("[[2024-13-45]]".to_string())
        )
        .create(&temp_dir, "invalid_dates.md");

    let file_info = MarkdownFileInfo::new(file_path)?;

    assert!(file_info.persist_reasons.contains(&PersistReason::DateCreatedUpdated {
        reason: DateValidationIssue::InvalidDateFormat
    }));
    assert!(file_info.persist_reasons.contains(&PersistReason::DateModifiedUpdated {
        reason: DateValidationIssue::InvalidDateFormat
    }));

    Ok(())
}

#[test]
fn test_date_created_fix_persist_reason() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let test_date = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();

    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-01-15]]".to_string()),
            Some("[[2024-01-15]]".to_string())
        )
        .with_fs_dates(test_date, test_date)
        .with_date_created_fix(Some("2024-01-01".to_string()))
        .create(&temp_dir, "date_fix.md");

    let file_info = MarkdownFileInfo::new(file_path)?;

    assert!(file_info.persist_reasons.contains(&PersistReason::DateCreatedFixApplied));

    Ok(())
}

#[test]
fn test_back_populate_persist_reason() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-01-15]]".to_string()),
            Some("[[2024-01-15]]".to_string())
        )
        .create(&temp_dir, "back_populate.md");

    let mut file_info = MarkdownFileInfo::new(file_path)?;
    file_info.mark_as_back_populated();

    assert!(file_info.persist_reasons.contains(&PersistReason::BackPopulated));

    Ok(())
}

#[test]
fn test_image_references_persist_reason() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("[[2024-01-15]]".to_string()),
            Some("[[2024-01-15]]".to_string())
        )
        .create(&temp_dir, "image_refs.md");

    let mut file_info = MarkdownFileInfo::new(file_path)?;
    file_info.record_image_references_change();

    assert!(file_info.persist_reasons.contains(&PersistReason::ImageReferencesModified));

    Ok(())
}

#[test]
fn test_multiple_persist_reasons() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(None, None)
        .with_title("test".to_string()) // to force frontmatter creation
        .create(&temp_dir, "multiple_reasons.md");

    let mut file_info = MarkdownFileInfo::new(file_path)?;

    // This will add DateCreatedUpdated and DateModifiedUpdated
    assert!(file_info.persist_reasons.contains(&PersistReason::DateCreatedUpdated {
        reason: DateValidationIssue::Missing
    }));

    // Add back populate reason
    file_info.mark_as_back_populated();

    // Add image reference change
    file_info.record_image_references_change();

    // Verify all reasons are present
    assert_eq!(file_info.persist_reasons.len(), 4);
    assert!(file_info.persist_reasons.contains(&PersistReason::BackPopulated));
    assert!(file_info.persist_reasons.contains(&PersistReason::ImageReferencesModified));

    Ok(())
}

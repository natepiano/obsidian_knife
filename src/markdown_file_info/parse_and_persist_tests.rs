use super::*;
use crate::frontmatter::FrontMatter;
use crate::test_utils::{parse_datetime, TestFileBuilder};
use crate::yaml_frontmatter::YamlFrontMatter;
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
    file_info.frontmatter = Some(FrontMatter::from_markdown_str(&fs::read_to_string(
        &file_path,
    )?)?);

    // Update frontmatter directly
    if let Some(fm) = &mut file_info.frontmatter {
        let created_date = parse_datetime("2024-01-02 00:00:00");
        fm.set_date_created(created_date);
        assert!(fm.needs_persist());
        fm.persist(&file_path)?;
    }

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
    file_info.frontmatter = Some(FrontMatter::from_markdown_str(&fs::read_to_string(
        &file_path,
    )?)?);

    if let Some(fm) = &mut file_info.frontmatter {
        fm.set_date_created(parse_datetime("2024-01-02 00:00:00"));
        fm.persist(&file_path)?;
    }

    let updated_content = fs::read_to_string(&file_path)?;
    // Match exact YAML format serde_yaml produces
    assert!(updated_content.contains("tags:\n- tag1\n- tag2"));
    assert!(updated_content.contains("[[2024-01-02]]"));

    Ok(())
}

#[test]
fn test_content_separation() {
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

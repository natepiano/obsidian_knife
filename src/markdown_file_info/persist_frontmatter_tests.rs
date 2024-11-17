use super::*;
use crate::frontmatter::FrontMatter;
use crate::test_utils::TestFileBuilder;
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
        fm.update_date_created("[[2024-01-02]]".to_string());
        fm.persist(&file_path)?;
    }

    // Verify frontmatter was updated but content preserved
    let updated_content = fs::read_to_string(&file_path)?;
    assert!(updated_content.contains("[[2024-01-02]]"));
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
        fm.update_date_created("[[2024-01-02]]".to_string());
        fm.persist(&file_path)?;
    }

    let updated_content = fs::read_to_string(&file_path)?;
    // Match exact YAML format serde_yaml produces
    assert!(updated_content.contains("tags:\n- tag1\n- tag2"));
    assert!(updated_content.contains("[[2024-01-02]]"));

    Ok(())
}

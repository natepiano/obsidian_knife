use crate::frontmatter::FrontMatter;
use crate::markdown_file_info::MarkdownFileInfo;
use crate::test_utils::{parse_datetime, TestFileBuilder};
use crate::yaml_frontmatter::YamlFrontMatter;
use serde_yaml::Value;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_update_frontmatter_fields() {
    let temp_dir = TempDir::new().unwrap();
    let custom_frontmatter = r#"custom_field: custom value
tags:
- tag1
- tag2"#
        .to_string();

    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("2024-10-23".to_string()),
            Some("2024-10-24".to_string()),
        )
        .with_custom_frontmatter(custom_frontmatter)
        .with_content("# Test Content".to_string())
        .create(&temp_dir, "test.md");

    let mut file_info = MarkdownFileInfo::new(file_path.clone()).unwrap();

    file_info
        .frontmatter
        .as_mut()
        .unwrap()
        .set_date_modified(parse_datetime("2023-10-24"));
    file_info
        .frontmatter
        .as_mut()
        .unwrap()
        .set_date_created(parse_datetime("2023-10-23"));

    file_info.frontmatter.unwrap().persist(&file_path).unwrap();

    let updated_content = fs::read_to_string(&file_path).unwrap();
    let updated_fm = FrontMatter::from_markdown_str(&updated_content).unwrap();

    // Check that the modified date was updated and other fields remain the same
    assert_eq!(updated_fm.date_modified, Some("[[2023-10-24]]".to_string()));
    assert_eq!(updated_fm.date_created, Some("[[2023-10-23]]".to_string()));
    assert_eq!(
        updated_fm.other_fields.get("custom_field"),
        Some(&Value::String("custom value".to_string()))
    );
    assert_eq!(
        updated_fm.other_fields.get("tags"),
        Some(&Value::Sequence(vec![
            Value::String("tag1".to_string()),
            Value::String("tag2".to_string())
        ]))
    );

    // Verify content after frontmatter remains intact
    let parts: Vec<&str> = updated_content.splitn(3, "---").collect();
    assert_eq!(parts[2].trim(), "# Test Content");
}

#[test]
fn test_frontmatter_with_aliases() {
    let content = r#"---
title: Test Note
aliases:
- old name
- another name
date_created: "2024-01-01"
---
Some content"#;

    let fm = FrontMatter::from_markdown_str(content).unwrap();
    assert_eq!(
        fm.aliases,
        Some(vec!["old name".to_string(), "another name".to_string()])
    );
}

#[test]
fn test_frontmatter_serialization_and_deserialization() {
    let temp_dir = TempDir::new().unwrap();
    let custom_frontmatter = r#"tags:
- tag1
- tag2
custom_field: value
nested:
  key1: value1
  key2: value2
array_field: [1, 2, 3]
boolean_field: true"#
        .to_string();

    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some("2024-01-01".to_string()),
            Some("2024-01-01".to_string()),
        )
        .with_custom_frontmatter(custom_frontmatter)
        .with_content("# Test Content".to_string())
        .create(&temp_dir, "test.md");

    // Update frontmatter
    let mut file_info = MarkdownFileInfo::new(file_path.clone()).unwrap();
    let fm = file_info.frontmatter.as_mut().unwrap();

    fm.set_date_created(parse_datetime("2024-01-01"));
    fm.set_date_modified(parse_datetime("2024-01-02"));

    file_info.frontmatter.unwrap().persist(&file_path).unwrap();

    let updated_content = fs::read_to_string(&file_path).unwrap();
    let updated_fm = FrontMatter::from_markdown_str(&updated_content).unwrap();

    // Verify updated fields
    assert_eq!(updated_fm.date_created, Some("[[2024-01-01]]".to_string()));
    assert_eq!(updated_fm.date_modified, Some("[[2024-01-02]]".to_string()));

    // Verify the structure of nested fields
    assert_eq!(
        updated_fm.other_fields.get("custom_field"),
        Some(&Value::String("value".to_string()))
    );
    assert!(updated_fm.other_fields.contains_key("nested"));
    assert!(updated_fm.other_fields.contains_key("array_field"));
    assert!(updated_fm.other_fields.contains_key("boolean_field"));

    // Verify content after frontmatter is preserved
    let parts: Vec<&str> = updated_content.splitn(3, "---").collect();
    assert_eq!(parts[2].trim(), "# Test Content");
}

use crate::frontmatter::FrontMatter;
use crate::markdown_file_info::MarkdownFileInfo;
use crate::yaml_frontmatter::YamlFrontMatter;
use serde_yaml::{Mapping, Number, Value};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;

// Test the basic functionality of updating frontmatter fields
#[test]
fn test_update_frontmatter_fields() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.md");

    let initial_content = r#"---
date_created: "[[2023-10-23]]"
custom_field: custom value
tags:
- tag1
- tag2
---
# Test Content"#;

    let mut file = File::create(&file_path).unwrap();
    write!(file, "{}", initial_content).unwrap();

    let mut file_info = MarkdownFileInfo::new(file_path.clone()).unwrap();
    file_info.frontmatter = Some(FrontMatter::from_markdown_str(&initial_content).unwrap());

    file_info
        .frontmatter
        .as_mut()
        .unwrap()
        .update_date_modified(Some("[[2023-10-24]]".to_string()));

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

// Combined test for serialization and deserialization with rich frontmatter
#[test]
fn test_frontmatter_serialization_and_deserialization() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.md");

    let initial_content = r#"---
title: My Test Note
date_created: "2024-01-01"
date_modified: "2024-01-01"
tags:
- tag1
- tag2
custom_field: value
nested:
key1: value1
key2: value2
array_field: [1, 2, 3]
boolean_field: true
---
# Test Content"#;

    let mut file = File::create(&file_path).unwrap();
    write!(file, "{}", initial_content).unwrap();

    // Update frontmatter
    let mut file_info = MarkdownFileInfo::new(file_path.clone()).unwrap();
    file_info.frontmatter = Some(FrontMatter::from_markdown_str(&initial_content).unwrap());
    file_info
        .frontmatter
        .as_mut()
        .unwrap()
        .update_date_modified(Some("[[2024-01-02]]".to_string()));
    file_info.frontmatter.unwrap().persist(&file_path).unwrap();

    let updated_content = fs::read_to_string(&file_path).unwrap();
    let updated_fm = FrontMatter::from_markdown_str(&updated_content).unwrap();

    // Verify updated fields
    assert_eq!(updated_fm.date_modified, Some("[[2024-01-02]]".to_string()));
    assert_eq!(updated_fm.date_created, Some("2024-01-01".to_string()));

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

#[test]
fn test_preserve_complex_frontmatter_values() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.md");

    let complex_str = "This is a multi-line\nstring value that should\nbe preserved exactly";
    let initial_frontmatter = FrontMatter {
        aliases: None,
        date_created: None,
        date_modified: Some("2024-01-01".to_string()),
        date_created_fix: None,
        do_not_back_populate: None,
        needs_persist: false,
        needs_filesystem_update: None,
        other_fields: {
            let mut map = HashMap::new();
            map.insert(
                "complex_field".to_string(),
                Value::String(complex_str.to_string()),
            );
            let mut nested_map = Mapping::new();
            nested_map.insert(
                Value::String("name".to_string()),
                Value::String("item1".to_string()),
            );
            nested_map.insert(
                Value::String("value".to_string()),
                Value::Number(Number::from(100)),
            );
            map.insert("nested_field".to_string(), Value::Mapping(nested_map));
            map.into()
        },
    };

    let initial_content = format!(
        "---\n{}---\n# Test Content",
        serde_yaml::to_string(&initial_frontmatter).unwrap()
    );

    let mut file = File::create(&file_path).unwrap();
    write!(file, "{}", initial_content).unwrap();

    // Update the frontmatter
    let mut file_info = MarkdownFileInfo::new(file_path.clone()).unwrap();
    file_info.frontmatter = Some(initial_frontmatter);
    file_info
        .frontmatter
        .as_mut()
        .unwrap()
        .update_date_modified(Some("[[2024-01-02]]".to_string()));
    file_info.frontmatter.unwrap().persist(&file_path).unwrap();

    let updated_content = fs::read_to_string(&file_path).unwrap();

    // Verify updated fields and ensure complex structure is preserved
    let updated_fm = FrontMatter::from_markdown_str(&updated_content).unwrap();
    assert_eq!(
        updated_fm.other_fields.get("complex_field"),
        Some(&Value::String(complex_str.to_string()))
    );
    assert!(updated_fm.other_fields.contains_key("nested_field"));
}

use crate::scan::MarkdownFileInfo;
use crate::wikilink::format_wikilink;
use crate::yaml_frontmatter::YamlFrontMatter;
use crate::{constants::*, ThreadSafeWriter};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct FrontMatterError {
    pub message: String,
    pub yaml_content: String,
}

impl FrontMatterError {
    pub fn new(message: String, content: &str) -> Self {
        // Strip out any "Content:" prefix from the message
        let message = if message.contains("Content:") {
            message
                .split("Content:")
                .next()
                .unwrap_or(&message)
                .trim()
                .to_string()
        } else {
            message
        };

        // Extract the YAML section between --- markers
        let yaml_content = FrontMatter::extract_yaml_section(content)
            .unwrap_or_default();

        FrontMatterError {
            message,
            yaml_content,
        }
    }

    pub fn get_yaml_with_line_numbers(&self) -> String {
        self.yaml_content.trim()
            .lines()
            .enumerate()
            .map(|(i, line)| format!("{:>3} | {}", i, line))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// when we set date_created_fix to None it won't serialize - cool
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FrontMatter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    // Fields we explicitly care about
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_created_fix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub do_not_back_populate: Option<Vec<String>>,
    // Catch-all field for unknown properties
    #[serde(flatten)]
    pub(crate) other_fields: HashMap<String, Value>,
}

impl FrontMatter {
    pub fn aliases(&self) -> Option<&Vec<String>> {
        self.aliases.as_ref()
    }

    pub fn date_created(&self) -> Option<&String> {
        self.date_created.as_ref()
    }

    pub fn date_modified(&self) -> Option<&String> {
        self.date_modified.as_ref()
    }

    pub fn date_created_fix(&self) -> Option<&String> {
        self.date_created_fix.as_ref()
    }

    pub fn update_date_created(&mut self, value: Option<String>) {
        self.date_created = value;
    }

    pub fn update_date_modified(&mut self, value: Option<String>) {
        self.date_modified = value;
    }

    pub fn update_date_created_fix(&mut self, value: Option<String>) {
        self.date_created_fix = value;
    }
}

impl YamlFrontMatter for FrontMatter {}

// Update frontmatter in a file
pub fn update_file_frontmatter(
    file_path: &Path,
    update_fn: impl FnOnce(&mut FrontMatter),
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let content = fs::read_to_string(file_path)?;
    let mut frontmatter = FrontMatter::from_markdown_str(&content)?;

    update_fn(&mut frontmatter);

    let updated_content = frontmatter.update_in_markdown_str(&content)
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

    fs::write(file_path, updated_content)?;

    Ok(())
}

pub fn report_frontmatter_issues(
    markdown_files: &HashMap<PathBuf, MarkdownFileInfo>,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let files_with_errors: Vec<_> = markdown_files
        .iter()
        .filter_map(|(path, info)| info.frontmatter_error.as_ref().map(|err| (path, err)))
        .collect();

    writer.writeln(LEVEL1, "frontmatter")?;

    if files_with_errors.is_empty() {
        return Ok(());
    }

    writer.writeln(
        "",
        &format!(
            "found {} files with frontmatter parsing errors",
            files_with_errors.len()
        ),
    )?;

    for (path, err) in files_with_errors {
        writer.writeln(LEVEL3, &format!("in file {}", format_wikilink(path)))?;
        writer.writeln("", &format!("error: {}", err.message))?;

        if !err.yaml_content.trim().is_empty() && err.yaml_content.trim() != "---" {
            writer.writeln("", "yaml content:")?;
            writer.writeln("", "```yaml")?;
            writer.writeln("", &err.get_yaml_with_line_numbers())?;
            writer.writeln("", "```")?;
        }

        writer.writeln("", "")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use serde_yaml::{Mapping, Number};
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

        update_file_frontmatter(&file_path, |frontmatter| {
            frontmatter.update_date_modified(Some("[[2023-10-24]]".to_string()));
        })
        .unwrap();

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
    fn test_serialize_frontmatter_with_aliases() {
        let fm = FrontMatter {
            aliases: Some(vec!["alias1".to_string(), "alias2".to_string()]),
            date_created: Some("2024-01-01".to_string()),
            date_modified: None,
            date_created_fix: None,
            do_not_back_populate: None,
            other_fields: HashMap::new(),
        };

        let yaml = serde_yaml::to_string(&fm).unwrap();
        assert!(yaml.contains("aliases:"));
        assert!(yaml.contains("- alias1"));
        assert!(yaml.contains("- alias2"));
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
        update_file_frontmatter(&file_path, |fm| {
            fm.update_date_modified(Some("[[2024-01-02]]".to_string()));
        })
            .unwrap();

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
                map
            },
        };

        let initial_content = format!(
            "---\n{}---\n# Test Content",
            serde_yaml::to_string(&initial_frontmatter).unwrap()
        );

        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", initial_content).unwrap();

        // Update the frontmatter
        update_file_frontmatter(&file_path, |fm| {
            fm.update_date_modified(Some("[[2024-01-02]]".to_string()));
        })
            .unwrap();

        let updated_content = fs::read_to_string(&file_path).unwrap();

        // Verify updated fields and ensure complex structure is preserved
        let updated_fm = FrontMatter::from_markdown_str(&updated_content).unwrap();
        assert_eq!(
            updated_fm.other_fields.get("complex_field"),
            Some(&Value::String(complex_str.to_string()))
        );
        assert!(updated_fm.other_fields.contains_key("nested_field"));
    }

    #[test]
    fn test_frontmatter_error() {
        let test_cases = vec![
            (
                "basic frontmatter",
                "---\ntitle: test\n---\ncontent",
                "some error",
                "title: test",
            ),
            (
                "missing frontmatter",
                "no frontmatter here",
                "missing frontmatter",
                "",
            ),
            (
                "content prefix stripping",
                "---\ntitle: test\n---",
                "Error: Content: something went wrong",
                "title: test",
            ),
            (
                "multiple sections",
                "---\ntitle: test\n---\ncontent\n---\nmore: yaml\n---",
                "error message",
                "title: test",
            ),
        ];

        for (name, content, message, expected_yaml) in test_cases {
            let error = FrontMatterError::new(message.to_string(), content);

            // Test message handling
            if message.contains("Content:") {
                assert!(!error.message.contains("Content:"), "Failed to strip Content: prefix in test: {}", name);
            }

            // Test YAML content extraction
            assert_eq!(
                error.yaml_content.trim(),
                expected_yaml.trim(),
                "YAML content mismatch in test: {}",
                name
            );

            // Test line number formatting
            if !expected_yaml.is_empty() {
                let with_line_numbers = error.get_yaml_with_line_numbers();
                assert!(with_line_numbers.contains("0 |"), "Missing line numbers in test: {}", name);
            }
        }
    }
}

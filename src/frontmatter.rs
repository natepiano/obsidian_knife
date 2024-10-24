use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;
use crate::yaml_utils;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FrontMatter {
    // Fields we explicitly care about
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_created_fix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_modified: Option<String>,
    // Catch-all field for unknown properties
    #[serde(flatten)]
    other_fields: HashMap<String, Value>,
}

impl FrontMatter {
    pub fn update_date_created(&mut self, value: Option<String>) {
        self.date_created = value;
    }

    pub fn update_date_modified(&mut self, value: Option<String>) {
        self.date_modified = value;
    }

    pub fn update_date_created_fix(&mut self, value: Option<String>) {
        self.date_created_fix = value;
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
}

/// Extract frontmatter from content and deserialize it
pub fn deserialize_frontmatter(content: &str) -> Result<FrontMatter, Box<dyn Error + Send + Sync>> {
    yaml_utils::deserialize_yaml_frontmatter(content)
}

/// Update the frontmatter in a file's content
pub fn update_frontmatter(
    content: &str,
    updated_frontmatter: &FrontMatter,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    // Serialize the entire frontmatter to YAML, including all fields
    let yaml_str = serde_yaml::to_string(&updated_frontmatter)?;

    // Trim any leading/trailing whitespace and remove any leading '---'
    let yaml_str = yaml_str.trim_start_matches("---").trim().to_string();

    // Find the opening '---\n'
    if let Some(start) = content.find("---\n") {
        // Start searching for the closing delimiter after the opening '---\n'
        let search_start = start + 4; // Length of '---\n' is 4
        if let Some(end_rel) = content[search_start..].find("\n---\n") {
            let end = search_start + end_rel + 1; // Position of '\n---\n'

            // Extract content before frontmatter
            let before = &content[..start];
            // Extract content after frontmatter
            let after = &content[end + 4..]; // Skip '\n---\n'

            // Reconstruct content with updated frontmatter
            // Ensures that closing '---' is on its own line, by adding a newline
            Ok(format!("{}---\n{}\n---\n{}", before, yaml_str, after))
        } else {
            // If no closing '---' is found, append it on its own line
            let before = &content[..start];
            let after = &content[start + 4..]; // Skip past '---\n'
            Ok(format!("{}---\n{}\n---\n{}", before, yaml_str, after))
        }
    } else {
        // No existing frontmatter, add new frontmatter at the beginning
        Ok(format!("---\n{}\n---\n{}", yaml_str, content))
    }
}


/// Update frontmatter in a file
pub fn update_file_frontmatter(
    file_path: &Path,
    update_fn: impl FnOnce(&mut FrontMatter),
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let content = fs::read_to_string(file_path)?;
    let mut frontmatter = deserialize_frontmatter(&content)?;

    update_fn(&mut frontmatter);

    let updated_content = update_frontmatter(&content, &frontmatter)?;
    fs::write(file_path, updated_content)?;

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
    fn test_update_file_frontmatter() {
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
        let updated_fm = deserialize_frontmatter(&updated_content).unwrap();

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
        let updated_fm = deserialize_frontmatter(&updated_content).unwrap();

        // Verify updated fields
        assert_eq!(updated_fm.date_modified, Some("[[2024-01-02]]".to_string()));
        assert_eq!(updated_fm.date_created, Some("2024-01-01".to_string()));

        // Verify the structure of nested fields
        assert_eq!(updated_fm.other_fields.get("custom_field"), Some(&Value::String("value".to_string())));
        assert!(updated_fm.other_fields.contains_key("nested"));
        assert!(updated_fm.other_fields.contains_key("array_field"));
        assert!(updated_fm.other_fields.contains_key("boolean_field"));

        // Verify content after frontmatter is preserved
        let parts: Vec<&str> = updated_content.splitn(3, "---").collect();
        assert_eq!(parts[2].trim(), "# Test Content");
    }

    // Focused test for ensuring that malformed YAML causes deserialization to fail
    #[test]
    fn test_deserialize_invalid_frontmatter() {
        let invalid_content = r#"---
invalid: [yaml
---"#;

        let result = deserialize_frontmatter(invalid_content);
        assert!(result.is_err(), "Expected deserialization of invalid YAML to fail");
    }

    // Focused test for complex field preservation, combining tests for complex/nested values
    #[test]
    fn test_preserve_complex_and_nested_values() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let complex_str = "This is a multi-line\nstring value that should\nbe preserved exactly";
        let initial_frontmatter = FrontMatter {
            date_created: None,
            date_modified: Some("2024-01-01".to_string()),
            date_created_fix: None,
            other_fields: {
                let mut map = HashMap::new();
                map.insert("complex_field".to_string(), Value::String(complex_str.to_string()));
                let mut nested_map = Mapping::new();
                nested_map.insert(Value::String("name".to_string()), Value::String("item1".to_string()));
                nested_map.insert(Value::String("value".to_string()), Value::Number(Number::from(100)));
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
        let updated_fm: FrontMatter = deserialize_frontmatter(&updated_content).unwrap();
        assert_eq!(
            updated_fm.other_fields.get("complex_field"),
            Some(&Value::String(complex_str.to_string()))
        );
        assert!(updated_fm.other_fields.contains_key("nested_field"));
    }
}

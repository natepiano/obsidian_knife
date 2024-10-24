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
    use serde_yaml::{Mapping, Number, Value};
    use tempfile::TempDir;

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

        // Deserialize the updated frontmatter
        let updated_fm = deserialize_frontmatter(&updated_content).unwrap();

        // Assert that date_modified is correctly updated
        assert_eq!(
            updated_fm.date_modified,
            Some("[[2023-10-24]]".to_string())
        );

        // Assert that other fields remain unchanged
        assert_eq!(
            updated_fm.date_created,
            Some("[[2023-10-23]]".to_string())
        );
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

        // Additionally, verify that the content after frontmatter remains intact
        let parts: Vec<&str> = updated_content.splitn(3, "---").collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[2].trim(), "# Test Content");
    }

    #[test]
    fn test_deserialize_frontmatter() {
        let content = r#"---
date_created: "[[2023-10-23]]"
date_modified: "[[2023-10-24]]"
custom_field: custom value
---
# Content"#;

        let frontmatter = deserialize_frontmatter(content).unwrap();
        assert_eq!(frontmatter.date_created, Some("[[2023-10-23]]".to_string()));
        assert_eq!(frontmatter.date_modified, Some("[[2023-10-24]]".to_string()));
        assert!(frontmatter.other_fields.contains_key("custom_field"));
    }

    #[test]
    fn test_deserialize_invalid_frontmatter() {
        let content = r#"---
invalid: [yaml
---"#;

        let result = deserialize_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_frontmatter_serialization() {
        let mut frontmatter = FrontMatter {
            date_created: Some("[[2023-10-23]]".to_string()),
            date_modified: Some("[[2023-10-23]]".to_string()),
            date_created_fix: None,
            other_fields: {
                let mut map = HashMap::new();
                map.insert(
                    "custom_field".to_string(),
                    Value::String("custom value".to_string()),
                );
                map.insert(
                    "tags".to_string(),
                    Value::Sequence(vec![
                        Value::String("tag1".to_string()),
                        Value::String("tag2".to_string()),
                    ]),
                );
                map
            },
        };

        let yaml = serde_yaml::to_string(&frontmatter).unwrap();
        let deserialized: FrontMatter = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(deserialized.date_created, Some("[[2023-10-23]]".to_string()));
        assert_eq!(deserialized.date_modified, Some("[[2023-10-23]]".to_string()));
        assert_eq!(deserialized.date_created_fix, None);

        assert_eq!(
            deserialized.other_fields.get("custom_field").unwrap(),
            &Value::String("custom value".to_string())
        );

        // Test update methods
        frontmatter.update_date_modified(Some("[[2023-10-24]]".to_string()));
        assert_eq!(frontmatter.date_modified, Some("[[2023-10-24]]".to_string()));
    }

    #[test]
    fn test_preserve_frontmatter_fields() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create a file with rich frontmatter including various YAML types
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

        // Update just the date fields
        update_file_frontmatter(&file_path, |fm| {
            fm.update_date_modified(Some("[[2024-01-02]]".to_string()));
            fm.update_date_created(Some("[[2024-01-01]]".to_string()));
        })
            .unwrap();

        // Read the updated content
        let updated_content = fs::read_to_string(&file_path).unwrap();

        // Deserialize the updated frontmatter
        let updated_fm = deserialize_frontmatter(&updated_content).unwrap();

        // Verify the dates were updated correctly
        assert_eq!(
            updated_fm.date_modified,
            Some("[[2024-01-02]]".to_string())
        );
        assert_eq!(
            updated_fm.date_created,
            Some("[[2024-01-01]]".to_string())
        );

        // Verify that other fields are preserved
        assert_eq!(
            updated_fm.other_fields.get("title"),
            Some(&Value::String("My Test Note".to_string()))
        );

        // Verify 'tags' field
        if let Some(Value::Sequence(tags)) = updated_fm.other_fields.get("tags") {
            let expected_tags = vec![
                Value::String("tag1".to_string()),
                Value::String("tag2".to_string()),
            ];
            assert_eq!(tags, &expected_tags);
        } else {
            panic!("'tags' field is missing or not a sequence");
        }

        // Verify 'custom_field'
        assert_eq!(
            updated_fm.other_fields.get("custom_field"),
            Some(&Value::String("value".to_string()))
        );

        // Verify 'nested' field
        if let Some(Value::Mapping(nested)) = updated_fm.other_fields.get("nested") {
            let mut expected_nested = Mapping::new();
            expected_nested.insert(
                Value::String("key1".to_string()),
                Value::String("value1".to_string()),
            );
            expected_nested.insert(
                Value::String("key2".to_string()),
                Value::String("value2".to_string()),
            );
            assert_eq!(nested, &expected_nested);
        } else {
            panic!("'nested' field is missing or not a mapping");
        }

        // Verify 'array_field'
        if let Some(Value::Sequence(array_field)) = updated_fm.other_fields.get("array_field") {
            let expected_array = vec![
                Value::Number(serde_yaml::Number::from(1)),
                Value::Number(serde_yaml::Number::from(2)),
                Value::Number(serde_yaml::Number::from(3)),
            ];
            assert_eq!(array_field, &expected_array);
        } else {
            panic!("'array_field' is missing or not a sequence");
        }

        // Verify 'boolean_field'
        assert_eq!(
            updated_fm.other_fields.get("boolean_field"),
            Some(&Value::Bool(true))
        );

        // Additionally, verify that the content after frontmatter is preserved
        let parts: Vec<&str> = updated_content.splitn(3, "---").collect();
        assert_eq!(parts.len(), 3, "Frontmatter delimiters not found correctly");
        assert_eq!(parts[2].trim(), "# Test Content");
    }

    #[test]
    fn test_preserve_complex_yaml_values() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create complex YAML with nested structures
        let mut other_fields = HashMap::new();

        // Add complex field
        let complex_str = "This is a multi-line\nstring value that should\nbe preserved exactly";
        other_fields.insert("complex_field".to_string(), Value::String(complex_str.to_string()));

        // Add list with objects
        let mut item1 = Mapping::new();
        item1.insert(Value::String("name".to_string()), Value::String("item1".to_string()));
        item1.insert(Value::String("value".to_string()), Value::Number(Number::from(100)));

        let mut item2 = Mapping::new();
        item2.insert(Value::String("name".to_string()), Value::String("item2".to_string()));
        item2.insert(Value::String("value".to_string()), Value::Number(Number::from(200)));

        let list = vec![Value::Mapping(item1), Value::Mapping(item2)];
        other_fields.insert("list_with_objects".to_string(), Value::Sequence(list));

        let initial_frontmatter = FrontMatter {
            date_created: None,
            date_modified: Some("2024-01-01".to_string()),
            date_created_fix: None,
            other_fields,
        };

        // Create initial content
        let initial_content = format!(
            "---\n{}---\nContent",
            serde_yaml::to_string(&initial_frontmatter).unwrap()
        );

        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", initial_content).unwrap();

        // Update frontmatter
        update_file_frontmatter(&file_path, |fm| {
            fm.update_date_modified(Some("[[2024-01-02]]".to_string()));
        })
            .unwrap();

        let updated_content = fs::read_to_string(&file_path).unwrap();

        // Optionally, print updated_content for debugging
        // println!("{}", updated_content);

        // Verify complex YAML structures are preserved
        assert!(updated_content.contains("complex_field:"));
        assert!(updated_content.contains("This is a multi-line"));
        assert!(updated_content.contains("string value that should"));
        assert!(updated_content.contains("be preserved exactly"));

        assert!(updated_content.contains("list_with_objects:"));
        assert!(updated_content.contains("- name: item1"));
        assert!(updated_content.contains("  value: 100"));
        assert!(updated_content.contains("- name: item2"));
        assert!(updated_content.contains("  value: 200"));

        // Parse the updated content to verify the structure
        let updated_fm: FrontMatter = deserialize_frontmatter(&updated_content).unwrap();

        // Verify the date was updated correctly
        assert_eq!(
            updated_fm.date_modified,
            Some("[[2024-01-02]]".to_string())
        );

        // Additionally, verify other fields
        assert_eq!(
            updated_fm.other_fields.get("complex_field"),
            Some(&Value::String(complex_str.to_string()))
        );
    }

    #[test]
    fn test_preserve_frontmatter_field_order() {
        // Initialize other_fields with HashMap (order is not preserved)
        let mut other_fields = HashMap::new();
        other_fields.insert("title".to_string(), Value::String("Test".to_string()));
        other_fields.insert("custom1".to_string(), Value::String("value1".to_string()));
        other_fields.insert("custom2".to_string(), Value::String("value2".to_string()));
        other_fields.insert("custom3".to_string(), Value::String("value3".to_string()));

        // Create the FrontMatter instance
        let fm = FrontMatter {
            date_created: Some("2024-01-01".to_string()),
            date_created_fix: None,
            date_modified: Some("2024-01-01".to_string()),
            other_fields,
        };

        // Original content with frontmatter
        let content = "---\ntitle: Test\ncustom1: value1\ndate_created: \"2024-01-01\"\ncustom2: value2\ndate_modified: \"2024-01-01\"\ncustom3: value3\n---\nContent";

        // Update frontmatter
        let updated = update_frontmatter(content, &fm).unwrap();

        // Optionally, print updated_content for debugging
        // println!("Updated Content:\n{}", updated);

        // Parse the updated content's frontmatter
        let updated_fm = deserialize_frontmatter(&updated).unwrap();

        // Define the expected FrontMatter
        let mut expected_other_fields = HashMap::new();
        expected_other_fields.insert("title".to_string(), Value::String("Test".to_string()));
        expected_other_fields.insert("custom1".to_string(), Value::String("value1".to_string()));
        expected_other_fields.insert("custom2".to_string(), Value::String("value2".to_string()));
        expected_other_fields.insert("custom3".to_string(), Value::String("value3".to_string()));

        let expected_fm = FrontMatter {
            date_created: Some("2024-01-01".to_string()),
            date_created_fix: None,
            date_modified: Some("2024-01-01".to_string()),
            other_fields: expected_other_fields,
        };

        // Assert that the updated FrontMatter matches the expected one
        assert_eq!(updated_fm, expected_fm);

        // Additionally, verify that date_created and date_modified have the correct values
        assert_eq!(updated_fm.date_created, Some("2024-01-01".to_string()));
        assert_eq!(updated_fm.date_modified, Some("2024-01-01".to_string()));

        // Verify that other_fields contain the expected key-value pairs
        assert_eq!(updated_fm.other_fields.get("title"), Some(&Value::String("Test".to_string())));
        assert_eq!(updated_fm.other_fields.get("custom1"), Some(&Value::String("value1".to_string())));
        assert_eq!(updated_fm.other_fields.get("custom2"), Some(&Value::String("value2".to_string())));
        assert_eq!(updated_fm.other_fields.get("custom3"), Some(&Value::String("value3".to_string())));
    }

    #[test]
    fn test_update_frontmatter_closing_delimiter_on_new_line() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Initial content with frontmatter, including 'maison---' in 'tags'
        let initial_content = r#"---
date_created: "[[2013-10-06]]"
date_modified: "[[2024-10-21]]"
date_created_fix: "[[2013-10-06]]"
return:
  - '[[domiciles]]'
aliases:
  - Villaggio
tags:
  - maison
---
lived here in the separation times - 2013/2014 era

# Address
4305 Lake Washington Blvd. NE Apt #2114
Kirkland, WA 98033"#;

        // Write initial content to the temporary file
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", initial_content).unwrap();

        // Update the frontmatter
        update_file_frontmatter(&file_path, |fm| {
            fm.update_date_modified(Some("[[2024-10-23]]".to_string()));
            fm.update_date_created(Some("[[2024-10-15]]".to_string()));
        })
            .unwrap();

        // Read the updated content
        let updated_content = fs::read_to_string(&file_path).unwrap();

        // Debugging: Uncomment the next line to print the updated content
        // println!("Updated Content:\n{}", updated_content);

        // 1. Verify that the closing '---' is on its own line
        assert!(
            updated_content.contains("\n---\n"),
            "Closing '---' is not on its own line"
        );

        // 2. Ensure that '---' appears exactly twice (opening and closing)
        let delimiter_count = updated_content.matches("---").count();
        assert_eq!(
            delimiter_count, 2,
            "Expected exactly two '---' delimiters, found {}",
            delimiter_count
        );

        // 3. Deserialize the updated frontmatter to verify data integrity
        let updated_fm = deserialize_frontmatter(&updated_content).expect("Failed to deserialize frontmatter");

        // 4. Verify the dates were updated correctly
        assert_eq!(
            updated_fm.date_modified,
            Some("[[2024-10-23]]".to_string()),
            "date_modified was not updated correctly"
        );
        assert_eq!(
            updated_fm.date_created,
            Some("[[2024-10-15]]".to_string()),
            "date_created was not updated correctly"
        );

        // 5. Verify that other fields are preserved correctly

        // 'date_created_fix' should remain unchanged
        assert_eq!(
            updated_fm.date_created_fix,
            Some("[[2013-10-06]]".to_string()),
            "date_created_fix was altered unexpectedly"
        );

        // 'return' field
        assert_eq!(
            updated_fm.other_fields.get("return"),
            Some(&Value::Sequence(vec![Value::String("[[domiciles]]".to_string())])),
            "'return' field was not preserved correctly"
        );

        // 'aliases' field
        assert_eq!(
            updated_fm.other_fields.get("aliases"),
            Some(&Value::Sequence(vec![Value::String("Villaggio".to_string())])),
            "'aliases' field was not preserved correctly"
        );

        // 'tags' field
        assert_eq!(
            updated_fm.other_fields.get("tags"),
            Some(&Value::Sequence(vec![Value::String("maison".to_string())])),
            "'tags' field was not preserved correctly"
        );

        // 6. Verify that the content after frontmatter is preserved intact
        let parts: Vec<&str> = updated_content.splitn(3, "---").collect();
        assert_eq!(
            parts.len(),
            3,
            "Frontmatter delimiters not found correctly in the updated content"
        );
        assert_eq!(
            parts[2].trim(),
            "lived here in the separation times - 2013/2014 era\n\n# Address\n4305 Lake Washington Blvd. NE Apt #2114\nKirkland, WA 98033",
            "Content after frontmatter was altered unexpectedly"
        );
    }
}

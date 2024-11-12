use serde::de::DeserializeOwned;
use std::error::Error;
use crate::frontmatter::FrontMatter;
use crate::yaml_frontmatter::YamlFrontMatter;

#[derive(Debug)]
pub enum YamlError {
    Missing,
    Invalid(String),
    Parse(String),
}

// In YamlError::fmt implementation
impl std::fmt::Display for YamlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            YamlError::Missing => write!(f, "file must start with YAML frontmatter (---)"),
            YamlError::Invalid(msg) => {
                write!(f, "file must have closing YAML frontmatter (---): {}", msg)
            }
            YamlError::Parse(msg) => write!(f, "error parsing YAML frontmatter: {}", msg),
        }
    }
}

impl Error for YamlError {}

/// Extracts and deserializes YAML front matter from the given content string.
///
/// # Arguments
///
/// * `content` - A string slice containing the entire file content.
///
/// # Returns
///
/// * `Ok(T)` where `T` is the deserialized structure.
/// * `Err(Box<dyn Error + Send + Sync>)` if extraction or deserialization fails.
pub fn deserialize_yaml_frontmatter<T: YamlFrontMatter>(content: &str) -> Result<T, Box<dyn Error + Send + Sync>> {
    T::from_markdown_str(content)
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
}

/// Extracts YAML front matter from the given content string.
///
/// # Arguments
///
/// * `content` - A string slice containing the entire file content.
///
/// # Returns
///
/// * `Ok(String)` containing the extracted YAML front matter.
/// * `Err(Box<dyn Error + Send + Sync>)` if extraction fails.
pub fn extract_yaml_frontmatter(content: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err(Box::new(YamlError::Missing));
    }

    let after_first = &trimmed[3..];
    if let Some(end_index) = after_first.find("---") {
        Ok(after_first[..end_index].trim().to_string())
    } else {
        Err(Box::new(YamlError::Invalid(
            "missing closing frontmatter delimiter (---)".to_string(),
        )))
    }
}

pub fn update_yaml_in_markdown<T: YamlFrontMatter>(
    content: &str,
    updated_frontmatter: &T,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let yaml_str = updated_frontmatter.to_yaml_str()
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

    let yaml_str = yaml_str.trim_start_matches("---").trim().to_string();

    // Find the opening '---\n'
    if let Some(start) = content.find("---\n") {
        // Start searching for the closing delimiter after the opening '---\n'
        let search_start = start + 4; // Length of '---\n' is 4
        if let Some(end_rel) = content[search_start..].find("\n---\n") {
            let end = search_start + end_rel + 1; // Position of '\n---\n'
            let before = &content[..start];
            let after = &content[end + 4..];
            Ok(format!("{}---\n{}\n---\n{}", before, yaml_str, after))
        } else {
            // No closing delimiter found - this is invalid frontmatter
            Err("Invalid frontmatter: missing closing delimiter (---)".into())
        }
    } else {
        // No opening delimiter - add new frontmatter at the beginning
        Ok(format!("---\n{}\n---\n{}", yaml_str, content))
    }
}

pub fn extract_yaml_section(content: &str) -> String {
    content
        .lines()
        .skip_while(|line| !line.starts_with("---"))
        .take_while(|line| !line.starts_with("---") || line == &"---")
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::fs::File;
    use std::io::Write;
    use serde::{Deserialize, Serialize};
    use serde_yaml::{Mapping, Number, Value};
    use tempfile::TempDir;
    use crate::frontmatter:: update_file_frontmatter;

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct TestConfig {
        title: String,
        value: i32,
    }

    impl YamlFrontMatter for TestConfig {}

    // Combined test for serialization and deserialization with rich frontmatter
    #[test]
    fn test_yaml_serialization_and_deserialization() {
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
        let updated_fm:FrontMatter = deserialize_yaml_frontmatter(&updated_content).unwrap();

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

    // New struct for deserialization test cases
    struct YamlDeserializeTestCase {
        description: &'static str,
        input: &'static str,
        expected_result: Option<TestConfig>,
        expected_err_type: Option<&'static str>,
    }

    #[test]
    fn test_preserve_complex_yaml_values() {
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
        let updated_fm: FrontMatter = deserialize_yaml_frontmatter(&updated_content).unwrap();
        assert_eq!(
            updated_fm.other_fields.get("complex_field"),
            Some(&Value::String(complex_str.to_string()))
        );
        assert!(updated_fm.other_fields.contains_key("nested_field"));
    }

    struct YamlUpdateTestCase {
        description: &'static str,
        input: &'static str,
        frontmatter: FrontMatter,
        expected: Option<&'static str>,
        expected_err: Option<&'static str>,
    }

    fn assert_yaml_update(test_case: YamlUpdateTestCase) {
        let result = update_yaml_in_markdown(test_case.input, &test_case.frontmatter);

        match (result, test_case.expected_err) {
            (Ok(output), None) => {
                // If we expected success, ensure output matches
                assert_eq!(
                    output.trim(),
                    test_case.expected.unwrap().trim(),
                    "Failed test: {}",
                    test_case.description
                );
            }
            (Err(e), Some(expected_err)) => {
                // If we expected an error, ensure error message matches
                assert_eq!(
                    e.to_string(),
                    expected_err,
                    "Failed test: {} - error message mismatch",
                    test_case.description
                );
            }
            (Ok(_), Some(expected_err)) => {
                panic!(
                    "Failed test: {} - expected error '{}' but got success",
                    test_case.description, expected_err
                );
            }
            (Err(e), None) => {
                panic!(
                    "Failed test: {} - expected success but got error: {}",
                    test_case.description, e
                );
            }
        }
    }

    #[test]
    fn test_update_yaml_in_markdown() {
        let test_cases = vec![
            YamlUpdateTestCase {
                description: "Basic YAML update",
                input: "---\ndate_created: old\n---\ncontent",
                frontmatter: FrontMatter {
                    aliases: None,
                    date_created: Some("new".to_string()),
                    date_modified: None,
                    date_created_fix: None,
                    do_not_back_populate: None,
                    other_fields: HashMap::new(),
                },
                expected: Some("---\ndate_created: new\n---\ncontent"),
                expected_err: None,
            },
            YamlUpdateTestCase {
                description: "Complex frontmatter update",
                input: "---\ndate_created: old\naliases:\n  - alias1\n---\ncontent",
                frontmatter: FrontMatter {
                    aliases: Some(vec!["alias1".to_string(), "alias2".to_string()]),
                    date_created: Some("new".to_string()),
                    date_modified: Some("today".to_string()),
                    date_created_fix: None,
                    do_not_back_populate: None,
                    other_fields: {
                        let mut map = HashMap::new();
                        map.insert("custom_field".to_string(), Value::String("value".to_string()));
                        map
                    },
                },
                expected: Some(r#"---
aliases:
- alias1
- alias2
date_created: new
date_modified: today
custom_field: value
---
content"#),
                expected_err: None,
            },
            YamlUpdateTestCase {
                description: "Invalid frontmatter - no closing delimiter",
                input: "---\ndate_created: old\ncontent",
                frontmatter: FrontMatter {
                    aliases: None,
                    date_created: Some("new".to_string()),
                    date_modified: None,
                    date_created_fix: None,
                    do_not_back_populate: None,
                    other_fields: HashMap::new(),
                },
                expected: None,
                expected_err: Some("Invalid frontmatter: missing closing delimiter (---)"),
            },
            YamlUpdateTestCase {
                description: "Empty document",
                input: "",
                frontmatter: FrontMatter {
                    aliases: None,
                    date_created: Some("new".to_string()),
                    date_modified: None,
                    date_created_fix: None,
                    do_not_back_populate: None,
                    other_fields: HashMap::new(),
                },
                expected: Some("---\ndate_created: new\n---\n"),
                expected_err: None,
            },
            YamlUpdateTestCase {
                description: "Preserve spacing",
                input: "---\ndate_created: old\n---\n\nContent with\n\nmultiple lines",
                frontmatter: FrontMatter {
                    aliases: None,
                    date_created: Some("new".to_string()),
                    date_modified: None,
                    date_created_fix: None,
                    do_not_back_populate: None,
                    other_fields: HashMap::new(),
                },
                expected: Some("---\ndate_created: new\n---\n\nContent with\n\nmultiple lines"),
                expected_err: None,
            },
        ];

        for test_case in test_cases {
            assert_yaml_update(test_case);
        }
    }

    struct YamlExtractionTestCase {
        description: &'static str,
        input: &'static str,
        expected: Option<&'static str>,
    }

    fn assert_yaml_extraction(test_case: YamlExtractionTestCase) {
        let result = extract_yaml_section(test_case.input);
        match test_case.expected {
            Some(expected) => assert_eq!(
                result.trim(),
                expected.trim(),
                "Failed test: {}",
                test_case.description
            ),
            None => assert_eq!(
                result.trim(),
                "",
                "Failed test: {}",
                test_case.description
            ),
        }
    }

    #[test]
    fn test_extract_yaml_section() {
        let test_cases = vec![
            YamlExtractionTestCase {
                description: "Basic YAML section",
                input: "---\nkey: value\n---\ncontent",
                expected: Some("---\nkey: value\n---\ncontent"),
            },
            YamlExtractionTestCase {
                description: "Empty YAML section",
                input: "---\n---\ncontent",
                expected: Some("---\n---\ncontent"),
            },
            YamlExtractionTestCase {
                description: "No YAML section",
                input: "Just content\nNo YAML here",
                expected: None,
            },
            YamlExtractionTestCase {
                description: "Multiple YAML sections",
                input: "---\nfirst: yaml\n---\ncontent\n---\nsecond: yaml\n---",
                expected: Some("---\nfirst: yaml\n---\ncontent\n---\nsecond: yaml\n---"),
            },
            YamlExtractionTestCase {
                description: "YAML with nested dashes",
                input: "---\nlist:\n  - item1\n  - item2\n---\ncontent",
                expected: Some("---\nlist:\n  - item1\n  - item2\n---\ncontent"),
            },
        ];

        for test_case in test_cases {
            assert_yaml_extraction(test_case);
        }
    }
}

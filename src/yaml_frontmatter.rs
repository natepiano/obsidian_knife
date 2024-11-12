use serde::{de::DeserializeOwned, Serialize};
use std::error::Error;

/// Error types specific to YAML frontmatter handling
#[derive(Debug, Clone)]
pub enum YamlFrontMatterError {
    /// there two lines with --- at the start, but nothing is there)
    Empty,
    /// No YAML frontmatter section found (no opening ---)
    Missing,
    /// Invalid YAML frontmatter (no closing ---)
    Invalid(String),
    /// Error parsing YAML content
    Parse(String),
    /// Error serializing to YAML
    Serialize(String),
}

impl std::fmt::Display for YamlFrontMatterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "yaml frontmatter delimiters are present but there is no yaml"),
            Self::Missing => write!(f, "file must start with YAML frontmatter (---)"),
            Self::Invalid(msg) => write!(f, "invalid YAML frontmatter: {}", msg),
            Self::Parse(msg) => write!(f, "error parsing YAML frontmatter: {}", msg),
            Self::Serialize(msg) => write!(f, "error serializing YAML frontmatter: {}", msg),
        }
    }
}

impl Error for YamlFrontMatterError {}

/// Trait for types that can be serialized to and deserialized from YAML frontmatter
pub trait YamlFrontMatter: Sized + DeserializeOwned + Serialize {
    /// Creates an instance from a YAML string
    fn from_yaml_str(yaml: &str) -> Result<Self, YamlFrontMatterError> {
        serde_yaml::from_str(yaml).map_err(|e| YamlFrontMatterError::Parse(e.to_string()))
    }

    /// Converts the instance to a YAML string
    fn to_yaml_str(&self) -> Result<String, YamlFrontMatterError> {
        serde_yaml::to_string(self).map_err(|e| YamlFrontMatterError::Serialize(e.to_string()))
    }

    /// Creates an instance from markdown content containing YAML frontmatter
    fn from_markdown_str(content: &str) -> Result<Self, YamlFrontMatterError> {
        let yaml = Self::extract_yaml_section(content)?;
        Self::from_yaml_str(&yaml)
    }

    fn extract_yaml_section(content: &str) -> Result<String, YamlFrontMatterError> {
        // Check that the content starts with "---\n"
        if !content.starts_with("---\n") {
            return Err(YamlFrontMatterError::Missing);
        }

        // Look for the closing "\n---\n" after the opening "---\n"
        let after_start = &content[4..]; // Skip the first "---\n" (4 characters)
        if let Some(end_index) = after_start.find("\n---\n") {
            let yaml_section = &after_start[..end_index].trim();
            if yaml_section.is_empty() {
                return Err(YamlFrontMatterError::Empty); // Return error for empty YAML
            }
            Ok(yaml_section.to_string())
        } else {
            // If we don't find the "\n---\n", it's an invalid frontmatter
            Err(YamlFrontMatterError::Invalid(
                "missing closing frontmatter delimiter (---)".to_string(),
            ))
        }
    }


    /// Updates YAML frontmatter in markdown content with this instance's data
    fn update_in_markdown_str(&self, content: &str) -> Result<String, YamlFrontMatterError> {
        let yaml_str = self.to_yaml_str()?;
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
                Err(YamlFrontMatterError::Invalid(
                    "missing closing delimiter (---)".to_string(),
                ))
            }
        } else {
            // No opening delimiter - add new frontmatter at the beginning
            Ok(format!("---\n{}\n---\n{}", yaml_str, content))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    // Add Clone derive to test struct
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
    struct TestFrontMatter {
        title: String,
        tags: Vec<String>,
    }

    impl YamlFrontMatter for TestFrontMatter {}

    fn assert_result<T, F>(
        result: Result<T, YamlFrontMatterError>,
        expected: Result<T, YamlFrontMatterError>,
        test_name: &str,
        ok_compare: F,
    ) where
        F: FnOnce(&T, &T),
        T: std::fmt::Debug + PartialEq,
    {
        match (&result, &expected) {
            (Ok(actual), Ok(expected)) => ok_compare(actual, expected),
            (Err(actual_err), Err(_expected_err)) => {
                assert!(
                    matches!(&actual_err, _),
                    "Failed test: {} - Expected error {:?}, got {:?}",
                    test_name,
                    _expected_err,
                    actual_err
                );
            }
            _ => panic!(
                "Failed test: {} - Result mismatch. Expected {:?}, got {:?}",
                test_name, expected, result
            ),
        }
    }

    struct YamlTestCase {
        name: &'static str,
        input: &'static str,
        expected: Result<TestFrontMatter, YamlFrontMatterError>,
    }

    #[test]
    fn test_yaml_frontmatter_parsing() {
        let test_cases = vec![
            YamlTestCase {
                name: "valid frontmatter",
                input: r#"---
title: test doc
tags:
  - tag1
  - tag2
---
content"#,
                expected: Ok(TestFrontMatter {
                    title: "test doc".to_string(),
                    tags: vec!["tag1".to_string(), "tag2".to_string()],
                }),
            },
            YamlTestCase {
                name: "missing frontmatter",
                input: "no frontmatter here",
                expected: Err(YamlFrontMatterError::Missing),
            },
            YamlTestCase {
                name: "unclosed frontmatter",
                input: r#"---
title: test doc
tags:
  - tag1"#,
                expected: Err(YamlFrontMatterError::Invalid(
                    "missing closing frontmatter delimiter (---)".to_string(),
                )),
            },
            YamlTestCase {
                name: "invalid yaml structure",
                input: r#"---
title: "unclosed string
tags: [not, valid, yaml
---"#,
                expected: Err(YamlFrontMatterError::Parse(
                    "found unexpected end of stream".to_string(),
                )),
            },
        ];

        for test_case in &test_cases {
            assert_result(
                TestFrontMatter::from_markdown_str(test_case.input),
                test_case.expected.clone(),
                test_case.name,
                |actual, expected| assert_eq!(actual, expected, "Failed test: {}", test_case.name),
            );
        }
    }

    struct SerializeTestCase {
        name: &'static str,
        input: TestFrontMatter,
        expected_contains: Vec<&'static str>,
    }

    #[test]
    fn test_yaml_frontmatter_serialization() {
        let test_cases = vec![
            SerializeTestCase {
                name: "basic serialization",
                input: TestFrontMatter {
                    title: "test doc".to_string(),
                    tags: vec!["tag1".to_string(), "tag2".to_string()],
                },
                expected_contains: vec!["title: test doc", "tags:", "- tag1", "- tag2"],
            },
            SerializeTestCase {
                name: "empty tags",
                input: TestFrontMatter {
                    title: "no tags".to_string(),
                    tags: vec![],
                },
                expected_contains: vec!["title: no tags", "tags: []"],
            },
        ];

        for test_case in test_cases {
            let result = test_case.input.to_yaml_str().unwrap();
            for expected in test_case.expected_contains {
                assert!(
                    result.contains(expected),
                    "Failed test: {} - Expected result to contain '{}', got:\n{}",
                    test_case.name,
                    expected,
                    result
                );
            }
        }
    }

    #[test]
    fn test_extract_yaml_section() {
        struct ExtractionTestCase {
            name: &'static str,
            input: &'static str,
            expected: Result<String, YamlFrontMatterError>,
        }

        let test_cases = vec![
            ExtractionTestCase {
                name: "valid yaml section",
                input: "---\ntitle: test\n---\ncontent",
                expected: Ok("title: test".to_string()),
            },
            ExtractionTestCase {
                name: "missing opening delimiter",
                input: "title: test\n---\ncontent",
                expected: Err(YamlFrontMatterError::Missing),
            },
            ExtractionTestCase {
                name: "missing closing delimiter",
                input: "---\ntitle: test\ncontent",
                expected: Err(YamlFrontMatterError::Invalid(
                    "missing closing frontmatter delimiter (---)".to_string(),
                )),
            },
            ExtractionTestCase {
                name: "empty yaml section",
                input: "---\n---\ncontent",
                expected: Err(YamlFrontMatterError::Empty), // Updated to expect `Empty` error
            },
        ];

        for test_case in &test_cases {
            assert_result(
                TestFrontMatter::extract_yaml_section(test_case.input),
                test_case.expected.to_owned(),
                test_case.name,
                |actual, expected| {
                    assert_eq!(
                        actual.trim(),
                        expected.trim(),
                        "Failed test: {}",
                        test_case.name
                    )
                },
            );
        }
    }

    #[test]
    fn test_update_in_markdown_str() {
        struct YamlUpdateTestCase {
            description: &'static str,
            input: &'static str,
            frontmatter: TestFrontMatter,
            expected: Option<&'static str>,
            expected_err: Option<YamlFrontMatterError>,
        }

        let test_cases = vec![
            YamlUpdateTestCase {
                description: "Basic YAML update",
                input: "---\ntitle: old\ntags:\n  - tag1\n---\ncontent",
                frontmatter: TestFrontMatter {
                    title: "new".to_string(),
                    tags: vec!["tag1".to_string()],
                },
                expected: Some("---\ntitle: new\ntags:\n- tag1\n---\ncontent"),
                expected_err: None,
            },
            YamlUpdateTestCase {
                description: "Complex frontmatter update",
                input: "---\ntitle: old\ntags:\n  - tag1\n---\ncontent",
                frontmatter: TestFrontMatter {
                    title: "new".to_string(),
                    tags: vec!["tag1".to_string(), "tag2".to_string()],
                },
                expected: Some(
                    "---\ntitle: new\ntags:\n- tag1\n- tag2\n---\ncontent",
                ),
                expected_err: None,
            },
            YamlUpdateTestCase {
                description: "Invalid frontmatter - no closing delimiter",
                input: "---\ntitle: old\ncontent",
                frontmatter: TestFrontMatter {
                    title: "new".to_string(),
                    tags: vec![],
                },
                expected: None,
                expected_err: Some(YamlFrontMatterError::Invalid(
                    "missing closing delimiter (---)".to_string(),
                )),
            },
            YamlUpdateTestCase {
                description: "Empty document",
                input: "",
                frontmatter: TestFrontMatter {
                    title: "new".to_string(),
                    tags: vec![],
                },
                expected: Some("---\ntitle: new\ntags: []\n---\n"),
                expected_err: None,
            },
            YamlUpdateTestCase {
                description: "Preserve spacing",
                input: "---\ntitle: old\n---\n\nContent with\n\nmultiple lines",
                frontmatter: TestFrontMatter {
                    title: "new".to_string(),
                    tags: vec![],
                },
                expected: Some("---\ntitle: new\ntags: []\n---\n\nContent with\n\nmultiple lines"),
                expected_err: None,
            },
        ];

        for test_case in test_cases {
            let result = test_case.frontmatter.update_in_markdown_str(test_case.input);

            match (result, test_case.expected_err) {
                (Ok(output), None) => {
                    assert_eq!(
                        output.trim(),
                        test_case.expected.unwrap().trim(),
                        "Failed test: {}",
                        test_case.description
                    );
                }
                (Err(e), Some(expected_err)) => {
                    assert_eq!(
                        e.to_string(),
                        expected_err.to_string(),
                        "Failed test: {} - error message mismatch",
                        test_case.description
                    );
                }
                (Ok(_), Some(expected_err)) => {
                    panic!(
                        "Failed test: {} - expected error '{}' but got success",
                        test_case.description,
                        expected_err
                    );
                }
                (Err(e), None) => {
                    panic!(
                        "Failed test: {} - expected success but got error: {}",
                        test_case.description,
                        e
                    );
                }
            }
        }
    }
}

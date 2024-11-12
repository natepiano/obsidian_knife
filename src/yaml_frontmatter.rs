use std::error::Error;
use serde::{Serialize, de::DeserializeOwned};

/// Error types specific to YAML frontmatter handling
#[derive(Debug, Clone)]
pub enum YamlFrontMatterError {
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

    /// Extracts the YAML section from markdown content
    fn extract_yaml_section(content: &str) -> Result<String, YamlFrontMatterError> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return Err(YamlFrontMatterError::Missing);
        }

        let after_first = &trimmed[3..];
        if let Some(end_index) = after_first.find("---") {
            Ok(after_first[..end_index].trim().to_string())
        } else {
            Err(YamlFrontMatterError::Invalid(
                "missing closing frontmatter delimiter (---)".to_string(),
            ))
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
            },
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
                expected: Ok("".to_string()),
            },
        ];

        for test_case in &test_cases {
            assert_result(
                TestFrontMatter::extract_yaml_section(test_case.input),
                test_case.expected.to_owned(),
                test_case.name,
                |actual, expected| assert_eq!(
                    actual.trim(),
                    expected.trim(),
                    "Failed test: {}",
                    test_case.name
                ),
            );
        }
    }
}

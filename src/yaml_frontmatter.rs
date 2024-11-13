use serde::{de::DeserializeOwned, Serialize};
use serde_yaml::Value;
use std::error::Error;
use std::fs;
use std::path::Path;

/// this macro allows us to persist any extra fields not specifically implemented in
/// a struct you want to deserialize into the yaml frontmatter of a markdown file
///
/// that way if other fields are added, they're not lost
///
/// this makes it so we don't have to remember to manually
/// add the field definitions - which we really couldn't know in advance anyway
#[macro_export]
macro_rules! yaml_frontmatter_struct {
    (
        $(#[$struct_meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field_name:ident : $field_ty:ty
            ),*
            $(,)?
        }
    ) => {
        // Main struct with flattened HashMap
        $(#[$struct_meta])*
        $vis struct $name {
            $(
                $(#[$field_meta])*
                $field_vis $field_name: $field_ty,
            )*
            #[serde(flatten)]
            other_fields: std::collections::HashMap<String, serde_yaml::Value>,
        }

        impl $name {
            pub fn new() -> Self {
                Self {
                    $(
                        $field_name: Default::default(),
                    )*
                    other_fields: std::collections::HashMap::new(),
                }
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl $crate::yaml_frontmatter::YamlFrontMatter for $name {}
    };
}

/// Error types specific to YAML frontmatter handling
#[derive(Debug, Clone)]
pub enum YamlFrontMatterError {
    /// there are two lines with --- at the start, but nothing is there
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

impl PartialEq for YamlFrontMatterError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (YamlFrontMatterError::Empty, YamlFrontMatterError::Empty) => true,
            (YamlFrontMatterError::Missing, YamlFrontMatterError::Missing) => true,
            (YamlFrontMatterError::Invalid(_), YamlFrontMatterError::Invalid(_)) => true,
            (YamlFrontMatterError::Parse(_), YamlFrontMatterError::Parse(_)) => true,
            (YamlFrontMatterError::Serialize(_), YamlFrontMatterError::Serialize(_)) => true,
            _ => false,
        }
    }
}

impl std::fmt::Display for YamlFrontMatterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(
                f,
                "yaml frontmatter delimiters are present but there is no yaml"
            ),
            Self::Missing => write!(f, "file must start with YAML frontmatter (---)"),
            Self::Invalid(msg) => write!(f, "invalid YAML frontmatter: {}", msg),
            Self::Parse(msg) => write!(f, "error parsing YAML frontmatter: {}", msg),
            Self::Serialize(msg) => write!(f, "error serializing YAML frontmatter: {}", msg),
        }
    }
}

impl Error for YamlFrontMatterError {}

/// Trait for types that can be serialized to and deserialized from YAML frontmatter
pub trait YamlFrontMatter: DeserializeOwned + Serialize {
    /// Creates an instance from a YAML string
    fn from_yaml_str(yaml: &str) -> Result<Self, YamlFrontMatterError> {
        serde_yaml::from_str(yaml).map_err(|e| YamlFrontMatterError::Parse(e.to_string()))
    }

    /// serialize the instance to a YAML string
    /// sorts all properties alphabetically, plus any contained lists are also sorted
    fn to_yaml_str(&self) -> Result<String, YamlFrontMatterError> {
        // First serialize to Value to manipulate the structure
        let value = serde_yaml::to_value(self)
            .map_err(|e| YamlFrontMatterError::Serialize(e.to_string()))?;

        if let Value::Mapping(map) = value {
            // Create a sorted mapping
            let mut sorted_map = serde_yaml::Mapping::new();

            // Collect all keys and sort them
            let mut keys: Vec<String> = map
                .keys()
                .filter_map(|k| k.as_str().map(String::from))
                .collect();
            keys.sort();

            // Rebuild mapping in sorted order
            for key in keys {
                if let Some(value) = map.get(&Value::String(key.clone())) {
                    // Sort sequence/list values if present
                    let sorted_value = match value {
                        Value::Sequence(seq) => {
                            let mut sorted_seq: Vec<String> = seq
                                .iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect();
                            sorted_seq.sort();
                            Value::Sequence(sorted_seq.into_iter().map(Value::String).collect())
                        }
                        _ => value.clone(),
                    };
                    sorted_map.insert(Value::String(key), sorted_value);
                }
            }

            // Serialize the sorted mapping
            serde_yaml::to_string(&Value::Mapping(sorted_map))
                .map_err(|e| YamlFrontMatterError::Serialize(e.to_string()))
        } else {
            Err(YamlFrontMatterError::Serialize(
                "Expected a mapping".to_string(),
            ))
        }
    }

    /// Creates an instance from markdown content containing YAML frontmatter
    fn from_markdown_str(content: &str) -> Result<Self, YamlFrontMatterError> {
        let yaml = Self::extract_yaml_section(content)?;
        Self::from_yaml_str(&yaml)
    }

    fn extract_yaml_section(content: &str) -> Result<String, YamlFrontMatterError> {
        match find_yaml_section(content)? {
            Some((yaml_section, _)) => Ok(yaml_section.to_string()),
            None => Err(YamlFrontMatterError::Missing),
        }
    }

    fn update_in_markdown_str(&self, content: &str) -> Result<String, YamlFrontMatterError> {
        let yaml_str = self.to_yaml_str()?;
        let yaml_str = yaml_str.trim_start_matches("---").trim();

        match find_yaml_section(content)? {
            Some((_, after_yaml)) => {
                // Replace the existing YAML frontmatter
                Ok(format!("---\n{}\n---\n{}", yaml_str, after_yaml))
            }
            None => {
                // No frontmatter found; add new YAML at the beginning
                Ok(format!("---\n{}\n---\n{}", yaml_str, content))
            }
        }
    }

    fn persist(&self, path: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
        let content = fs::read_to_string(path)?;
        let updated_content = self
            .update_in_markdown_str(&content)
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        fs::write(path, updated_content)?;

        Ok(())
    }
}

fn find_yaml_section(content: &str) -> Result<Option<(&str, &str)>, YamlFrontMatterError> {
    if !content.starts_with("---\n") {
        return Err(YamlFrontMatterError::Missing); // No YAML section found
    }

    let after_start = &content[4..]; // Skip "---\n"

    // Check for immediate closing delimiter for an empty YAML section
    if after_start.starts_with("---\n") {
        return Err(YamlFrontMatterError::Empty);
    }

    // Check for a closing delimiter followed by either `\n` or end of file
    if let Some(end_index) = after_start.find("\n---\n").or_else(|| {
        after_start
            .find("\n---")
            .filter(|&i| i + 4 == after_start.len())
    }) {
        let yaml_section = &after_start[..end_index].trim();
        if yaml_section.is_empty() {
            return Err(YamlFrontMatterError::Empty);
        }

        // If `\n---\n` was found,d skip 5 characters; otherwise, skip only 4 for `\n---`
        let after_yaml_start = if after_start[end_index..].starts_with("\n---\n") {
            end_index + 5
        } else {
            end_index + 4
        };

        let after_yaml = &after_start[after_yaml_start..];
        Ok(Some((yaml_section, after_yaml)))
    } else {
        // No closing delimiter found
        Err(YamlFrontMatterError::Invalid(
            "missing closing frontmatter delimiter (---)".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::assert_result;
    use crate::yaml_frontmatter_struct;
    use serde::{Deserialize, Serialize};
    use std::cmp::PartialEq;

    yaml_frontmatter_struct! {
        #[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
        struct TestFrontMatter {
            title: String,
            tags: Vec<String>,
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
                    other_fields: Default::default(),
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
                    other_fields: Default::default(),
                },
                expected_contains: vec!["title: test doc", "tags:", "- tag1", "- tag2"],
            },
            SerializeTestCase {
                name: "empty tags",
                input: TestFrontMatter {
                    title: "no tags".to_string(),
                    tags: vec![],
                    other_fields: Default::default(),
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
            expected: Result<String, YamlFrontMatterError>,
        }

        let test_cases = vec![
            YamlUpdateTestCase {
                description: "Basic YAML update",
                input: "---\ntags:\n- tag1\ntitle: new\n---\ncontent",
                frontmatter: TestFrontMatter {
                    title: "new".to_string(),
                    tags: vec!["tag1".to_string()],
                    other_fields: Default::default(),
                },
                expected: Ok("---\ntags:\n- tag1\ntitle: new\n---\ncontent".to_string()),
            },
            YamlUpdateTestCase {
                description: "Complex frontmatter update",
                input: "---\ntitle: old\ntags:\n  - tag1\n---\ncontent",
                frontmatter: TestFrontMatter {
                    title: "new".to_string(),
                    tags: vec!["tag1".to_string(), "tag2".to_string()],
                    other_fields: Default::default(),
                },
                expected: Ok("---\ntags:\n- tag1\n- tag2\ntitle: new\n---\ncontent".to_string()),
            },
            YamlUpdateTestCase {
                description: "Invalid frontmatter - no closing delimiter",
                input: "---\ntitle: old\ncontent",
                frontmatter: TestFrontMatter {
                    title: "new".to_string(),
                    tags: vec![],
                    other_fields: Default::default(),
                },
                expected: Err(YamlFrontMatterError::Invalid("".to_string())), // Error variant only
            },
            YamlUpdateTestCase {
                description: "Preserve spacing",
                input: "---\ntitle: old\n---\n\nContent with\n\nmultiple lines",
                frontmatter: TestFrontMatter {
                    title: "new".to_string(),
                    tags: vec![],
                    other_fields: Default::default(),
                },
                expected: Ok(
                    "---\ntags: []\ntitle: new\n---\n\nContent with\n\nmultiple lines".to_string(),
                ),
            },
        ];

        for test_case in &test_cases {
            assert_result(
                test_case
                    .frontmatter
                    .update_in_markdown_str(test_case.input),
                test_case.expected.clone(),
                test_case.description,
                |actual, expected| {
                    assert_eq!(
                        actual.trim(),
                        expected.trim(),
                        "Failed test: {}",
                        test_case.description
                    )
                },
            );
        }
    }

    #[test]
    fn test_yaml_frontmatter_sorted_serialization() {
        struct SerializationOrderTestCase {
            name: &'static str,
            input: TestFrontMatter,
            expected: Result<String, YamlFrontMatterError>,
        }

        let test_cases = vec![
            SerializationOrderTestCase {
                name: "fields and lists should be sorted alphabetically",
                input: TestFrontMatter {
                    tags: vec!["zebra".to_string(), "alpha".to_string(), "beta".to_string()],
                    title: "test doc".to_string(),
                    other_fields: Default::default(),
                },
                expected: Ok("tags:\n- alpha\n- beta\n- zebra\ntitle: test doc".to_string()),
            },
            SerializationOrderTestCase {
                name: "empty lists should maintain alphabetical field order",
                input: TestFrontMatter {
                    tags: vec![],
                    title: "no tags".to_string(),
                    other_fields: Default::default(),
                },
                expected: Ok("tags: []\ntitle: no tags".to_string()),
            },
            SerializationOrderTestCase {
                name: "single item lists should be sorted",
                input: TestFrontMatter {
                    tags: vec!["tag1".to_string()],
                    title: "one tag".to_string(),
                    other_fields: Default::default(),
                },
                expected: Ok("tags:\n- tag1\ntitle: one tag".to_string()),
            },
        ];

        for test_case in &test_cases {
            assert_result(
                test_case.input.to_yaml_str(),
                test_case.expected.clone(),
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
}

use crate::frontmatter::FrontMatter;
use crate::yaml_frontmatter::YamlFrontMatter;
use std::error::Error;

pub fn update_yaml_in_markdown<T: YamlFrontMatter>(
    content: &str,
    updated_frontmatter: &T,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let yaml_str = updated_frontmatter
        .to_yaml_str()
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
    use serde::{Deserialize, Serialize};
    use serde_yaml::{Value};
    use std::collections::HashMap;

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct TestConfig {
        title: String,
        value: i32,
    }

    impl YamlFrontMatter for TestConfig {}

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
                        map.insert(
                            "custom_field".to_string(),
                            Value::String("value".to_string()),
                        );
                        map
                    },
                },
                expected: Some(
                    r#"---
aliases:
- alias1
- alias2
date_created: new
date_modified: today
custom_field: value
---
content"#,
                ),
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
            None => assert_eq!(result.trim(), "", "Failed test: {}", test_case.description),
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

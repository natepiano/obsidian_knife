use serde::de::DeserializeOwned;
use std::error::Error;

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
            YamlError::Invalid(msg) => write!(f, "file must have closing YAML frontmatter (---): {}", msg),
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
pub fn deserialize_yaml_frontmatter<T>(content: &str) -> Result<T, Box<dyn Error + Send + Sync>>
where
    T: DeserializeOwned,
{
    let yaml_str = extract_yaml_frontmatter(content)?;
    match serde_yaml::from_str(&yaml_str) {
        Ok(value) => Ok(value),
        Err(e) => Err(Box::new(YamlError::Parse(format!(
            "{} Content:\n{}",
            e, yaml_str
        )))),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestConfig {
        title: String,
        value: i32,
    }

    #[test]
    fn test_deserialize_valid_frontmatter() {
        let content = r#"---
title: test
value: 42
---
Some content"#;

        let config: TestConfig = deserialize_yaml_frontmatter(content).unwrap();
        assert_eq!(
            config,
            TestConfig {
                title: "test".to_string(),
                value: 42
            }
        );
    }

    #[test]
    fn test_missing_frontmatter() {
        let content = "No frontmatter here";
        let result = deserialize_yaml_frontmatter::<TestConfig>(content);
        assert!(matches!(
            result.unwrap_err().downcast_ref::<YamlError>(),
            Some(YamlError::Missing)
        ));
    }

    #[test]
    fn test_invalid_yaml() {
        let content = r#"---
title: "unclosed string
value: not-a-number
---"#;

        let result = deserialize_yaml_frontmatter::<TestConfig>(content);
        assert!(matches!(
            result.unwrap_err().downcast_ref::<YamlError>(),
            Some(YamlError::Parse(_))
        ));
    }

    #[test]
    fn test_unclosed_frontmatter() {
        let content = r#"---
title: test
value: 42"#;

        let result = deserialize_yaml_frontmatter::<TestConfig>(content);
        assert!(matches!(
            result.unwrap_err().downcast_ref::<YamlError>(),
            Some(YamlError::Invalid(_))
        ));
    }
    
}

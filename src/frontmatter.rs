use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    let yaml_str = extract_frontmatter(content)?;
    match serde_yaml::from_str(&yaml_str) {
        Ok(value) => Ok(value),
        Err(e) => {
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "error parsing yaml frontmatter: {}. Content:\n{}",
                    e, yaml_str
                ),
            )))
        }
    }
}

/// Extract the YAML frontmatter section from the content
pub fn extract_frontmatter(content: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file must start with YAML frontmatter (---)",
        )));
    }

    let after_first = &trimmed[3..];
    if let Some(end_index) = after_first.find("---") {
        Ok(after_first[..end_index].trim().to_string())
    } else {
        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file must have closing YAML frontmatter (---)",
        )))
    }
}

/// Update the frontmatter in a file's content
pub fn update_frontmatter(content: &str, updated_frontmatter: &FrontMatter) -> Result<String, Box<dyn Error + Send + Sync>> {
    let mut yaml_str = String::new();

    // Handle date fields with explicit formatting
    if let Some(date_created) = &updated_frontmatter.date_created {
        yaml_str.push_str(&format!("date_created: \"{}\"\n", date_created));
    }
    if let Some(date_modified) = &updated_frontmatter.date_modified {
        yaml_str.push_str(&format!("date_modified: \"{}\"\n", date_modified));
    }
    if let Some(date_created_fix) = &updated_frontmatter.date_created_fix {
        yaml_str.push_str(&format!("date_created_fix: \"{}\"\n", date_created_fix));
    }

    // Serialize remaining fields
    if !updated_frontmatter.other_fields.is_empty() {
        let other_yaml = serde_yaml::to_string(&updated_frontmatter.other_fields)?;
        if !other_yaml.trim().is_empty() {
            yaml_str.push_str(&other_yaml);
        }
    }

    let yaml_str = yaml_str.trim();

    if let (Some(start), Some(end)) = (content.find("---"), content[3..].find("---").map(|i| i + 3)) {
        // Replace existing frontmatter
        let before = &content[..start];
        let after = &content[end + 3..];
        Ok(format!("{}---\n{}---{}", before, yaml_str, after))
    } else {
        // Add new frontmatter
        Ok(format!("---\n{}---\n{}", yaml_str, content))
    }
}

/// Update frontmatter in a file
pub fn update_file_frontmatter(file_path: &Path, update_fn: impl FnOnce(&mut FrontMatter)) -> Result<(), Box<dyn Error + Send + Sync>> {
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

        assert!(updated_content.contains("date_modified: \"[[2023-10-24]]\""));
        assert!(updated_content.contains("custom_field: custom value"));
        assert!(updated_content.contains("- tag1"));
        assert!(updated_content.contains("# Test Content"));
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
    fn test_deserialize_invalid_frontmatter() {
        let content = r#"---
invalid: [yaml
---
# Content"#;

        let result = deserialize_frontmatter(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("error parsing yaml frontmatter"));
    }

    #[test]
    fn test_missing_frontmatter() {
        let content = "# Just content\nNo frontmatter here";

        let result = deserialize_frontmatter(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must start with YAML frontmatter"));
    }

    #[test]
    fn test_unclosed_frontmatter() {
        let content = r#"---
date_created: "[[2023-10-23]]"
# No closing delimiter"#;

        let result = deserialize_frontmatter(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must have closing YAML frontmatter"));
    }
}

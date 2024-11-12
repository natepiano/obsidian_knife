use crate::scan::MarkdownFileInfo;
use crate::wikilink::format_wikilink;
use crate::yaml_frontmatter::YamlFrontMatter;
use crate::yaml_utils::{extract_yaml_section, update_yaml_in_markdown};
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
        let yaml_content = extract_yaml_section(content);

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

    let updated_content = update_yaml_in_markdown(&content, &frontmatter)?;

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
}

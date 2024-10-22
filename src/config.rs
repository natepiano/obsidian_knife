use crate::validated_config::ValidatedConfig;
use serde::Deserialize;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    apply_changes: Option<bool>,
    ignore_folders: Option<Vec<String>>,
    ignore_text: Option<Vec<String>>,
    obsidian_path: String,
    output_folder: Option<String>,
    simplify_wikilinks: Option<Vec<String>>,
}

impl Config {
    fn from_yaml_str(yaml: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        serde_yaml::from_str(yaml).map_err(|e| {
            let error = std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("error parsing yaml configuration: {}", e),
            );
            Box::new(error) as Box<dyn Error + Send + Sync>
        })
    }

    fn extract_yaml_frontmatter(content: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "configuration file must start with yaml frontmatter (---)",
            )));
        }

        // Find the second occurrence of "---"
        let after_first = &trimmed[3..];
        if let Some(end_index) = after_first.find("---") {
            Ok(after_first[..end_index].trim().to_string())
        } else {
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "configuration file must have closing yaml frontmatter (---)",
            )))
        }
    }

    pub fn from_obsidian_file(path: &Path) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let expanded_path = expand_tilde(path);
        let contents = fs::read_to_string(&expanded_path).map_err(|e| -> Box<dyn Error + Send + Sync> {
            if e.kind() == std::io::ErrorKind::NotFound {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("config file not found: {}", expanded_path.display()),
                ))
            } else {
                Box::new(std::io::Error::new(
                    e.kind(),
                    format!("error reading config file '{}': {}", expanded_path.display(), e),
                ))
            }
        })?;

        let yaml = Self::extract_yaml_frontmatter(&contents)?;
        Self::from_yaml_str(&yaml)
    }

    pub fn validate(self) -> Result<ValidatedConfig, Box<dyn Error + Send + Sync>> {
        let expanded_path = expand_tilde(&self.obsidian_path);
        if !expanded_path.exists() {
            return Err(format!("Path does not exist: {:?}", expanded_path).into());
        }

        // Handle output folder
        let output_folder = if let Some(ref folder) = self.output_folder {
            if folder.trim().is_empty() {
                return Err("output_folder cannot be empty".into());
            }
            expanded_path.join(folder.trim())
        } else {
            expanded_path.join("obsidian_knife") // Default folder name
        };

        // Add output folder and cache folder to ignored folders
        let mut ignore_folders = self.validate_ignore_folders(&expanded_path)?;
        let mut folders_to_add = vec![
            output_folder.clone(),
            expanded_path.join(crate::constants::CACHE_FOLDER),
        ];

        if let Some(ref mut folders) = ignore_folders {
            folders.append(&mut folders_to_add);
        } else {
            ignore_folders = Some(folders_to_add);
        }

        let validated_simplify_wikilinks = self.validate_simplify_wikilinks()?;
        let validate_ignore_text = self.validate_ignore_text()?;

        Ok(ValidatedConfig::new(
            self.apply_changes.unwrap_or(false),
            ignore_folders,
            expanded_path,
            output_folder,
            validated_simplify_wikilinks,
            validate_ignore_text,
        ))
    }

    fn validate_ignore_text(&self) -> Result<Option<Vec<String>>, Box<dyn Error + Send + Sync>> {
        match &self.ignore_text {
            Some(patterns) => {
                let mut validated = Vec::new();
                for (index, pattern) in patterns.iter().enumerate() {
                    let trimmed = pattern.trim();
                    if trimmed.is_empty() {
                        return Err(format!(
                            "ignore: entry at index {} is empty or only contains whitespace",
                            index
                        )
                        .into());
                    }
                    validated.push(trimmed.to_string());
                }
                if validated.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(validated))
                }
            }
            None => Ok(None),
        }
    }

    fn validate_ignore_folders(
        &self,
        expanded_path: &PathBuf,
    ) -> Result<Option<Vec<PathBuf>>, Box<dyn Error + Send + Sync>> {
        let ignore_folders = if let Some(folders) = &self.ignore_folders {
            if folders.is_empty() {
                None
            } else {
                let mut validated_folders = Vec::new();
                for (index, folder) in folders.iter().enumerate() {
                    if folder.trim().is_empty() {
                        return Err(format!("ignore_folders: entry at index {} is empty or only contains whitespace", index).into());
                    }
                    let full_path = expanded_path.join(folder);
                    if !full_path.exists() {
                        return Err(format!("Ignore folder does not exist: {:?}", full_path).into());
                    }
                    validated_folders.push(full_path);
                }
                Some(validated_folders)
            }
        } else {
            None
        };
        Ok(ignore_folders)
    }

    fn validate_simplify_wikilinks(
        &self,
    ) -> Result<Option<Vec<String>>, Box<dyn Error + Send + Sync>> {
        match &self.simplify_wikilinks {
            Some(patterns) => {
                let mut validated = Vec::new();
                for (index, pattern) in patterns.iter().enumerate() {
                    let trimmed = pattern.trim();
                    if trimmed.is_empty() {
                        return Err(format!("simplify_wikilinks: entry at index {} is empty or only contains whitespace", index).into());
                    }
                    validated.push(trimmed.to_string());
                }
                if validated.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(validated))
                }
            }
            None => Ok(None),
        }
    }
}

fn expand_tilde<P: AsRef<Path>>(path: P) -> PathBuf {
    let path_str = path.as_ref().to_str().unwrap_or("");
    if path_str.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(&path_str[2..]);
        }
    }
    path.as_ref().to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use tempfile::TempDir;


    #[test]
    fn test_extract_yaml_frontmatter() {
        let content = r#"---
obsidian_path: ~/Documents/brain
apply_changes: false
cleanup_image_files: true
---
# Configuration
This is my Obsidian configuration file.
"#;
        let yaml = Config::extract_yaml_frontmatter(content).unwrap();
        assert!(yaml.contains("obsidian_path"));
        assert!(yaml.contains("apply_changes"));
        assert!(!yaml.contains("# Configuration"));
    }

    #[test]
    fn test_extract_yaml_frontmatter_no_start() {
        let content = "not a yaml file";
        assert!(Config::extract_yaml_frontmatter(content).is_err());
    }

    #[test]
    fn test_extract_yaml_frontmatter_no_end() {
        let content = "---\nsome: yaml\nbut no end";
        assert!(Config::extract_yaml_frontmatter(content).is_err());
    }

    #[test]
    fn test_from_obsidian_file_with_tilde() {
        // Only run this test if we can get the home directory
        if let Some(home) = std::env::var_os("HOME") {
            let mut temp_file = NamedTempFile::new().unwrap();

            let config_content = r#"---
obsidian_path: ~/Documents/brain
apply_changes: false
cleanup_image_files: true
---"#;

            temp_file.write_all(config_content.as_bytes()).unwrap();

            // Create stable string values
            let home_str = PathBuf::from(home).to_string_lossy().into_owned();
            let temp_str = temp_file.path().to_string_lossy().into_owned();
            let tilde_path = temp_str.replace(&home_str, "~");

            let config = Config::from_obsidian_file(Path::new(&tilde_path)).unwrap();
            assert_eq!(config.obsidian_path, "~/Documents/brain");
            assert_eq!(config.apply_changes, Some(false));
            assert_eq!(config.cleanup_image_files, Some(true));
        }
    }

    #[test]
    fn test_from_obsidian_file_not_found() {
        let result = Config::from_obsidian_file(Path::new("~/nonexistent/config.md"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("config file not found"));
    }

    #[test]
    fn test_from_obsidian_file_invalid_yaml() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"---\ninvalid: yaml: content:\n---").unwrap();

        let result = Config::from_obsidian_file(temp_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_config_with_output_folder() {
        let yaml = r#"
obsidian_path: ~/Documents/brain
output_folder: custom_output
apply_changes: false"#;

        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.output_folder, Some("custom_output".to_string()));
    }

    #[test]
    fn test_config_without_output_folder() {
        let yaml = r#"
obsidian_path: ~/Documents/brain
apply_changes: false"#;

        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.output_folder, None);
    }

    #[test]
    fn test_validate_empty_output_folder() {
        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();

        let yaml = format!(r#"
obsidian_path: {}
output_folder: "  ""#, temp_dir.path().display());

        let config: Config = serde_yaml::from_str(&yaml).unwrap();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("output_folder cannot be empty"));
    }

    #[test]
    fn test_output_folder_added_to_ignore() {
        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();

        // Create the .obsidian directory
        let obsidian_dir = temp_dir.path().join(".obsidian");
        fs::create_dir(&obsidian_dir).unwrap();

        let yaml = format!(r#"
obsidian_path: {}
output_folder: custom_output
ignore_folders:
  - .obsidian"#, temp_dir.path().display());

        let config: Config = serde_yaml::from_str(&yaml).unwrap();
        let validated = config.validate().unwrap();

        let ignore_folders = validated.ignore_folders().unwrap();
        let output_path = validated.output_folder();

        assert!(ignore_folders.contains(&output_path.to_path_buf()));
        assert!(ignore_folders.contains(&obsidian_dir));
    }

    #[test]
    fn test_from_obsidian_file_with_output_folder() {
        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();
        let temp_file = temp_dir.path().join("config.md");

        let config_content = format!(r#"---
obsidian_path: {}
output_folder: custom_output
apply_changes: false
---
# Configuration
This is a test configuration file."#, temp_dir.path().display());

        fs::write(&temp_file, config_content).unwrap();

        let config = Config::from_obsidian_file(&temp_file).unwrap();
        assert_eq!(config.output_folder, Some("custom_output".to_string()));
    }

    #[test]
    fn test_default_output_folder() {
        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();

        let yaml = format!(r#"
obsidian_path: {}"#, temp_dir.path().display());

        let config: Config = serde_yaml::from_str(&yaml).unwrap();
        let validated = config.validate().unwrap();

        let expected_output = temp_dir.path().join("obsidian_knife");
        assert_eq!(validated.output_folder(), expected_output.as_path());
    }

    #[test]
    fn test_validate_simplify_wikilinks() {
        // Test valid config
        let yaml = r#"
        obsidian_path: ~/Documents/brain
        apply_changes: false
        cleanup_image_files: true
        ignore_folders:
          - .idea
          - .obsidian
        simplify_wikilinks:
          - "Ed:"
          - "  Valid Entry  "
        "#;

        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let result = config.validate_simplify_wikilinks().unwrap();
        assert_eq!(
            result,
            Some(vec![String::from("Ed:"), String::from("Valid Entry")])
        );

        // Test config with empty entry
        let yaml_with_empty = r#"
        obsidian_path: ~/Documents/brain
        simplify_wikilinks:
          - "Ed:"
          - ""
          - "Valid Entry"
        "#;

        let config_with_empty: Config = serde_yaml::from_str(yaml_with_empty).unwrap();
        let result = config_with_empty.validate_simplify_wikilinks();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "simplify_wikilinks: entry at index 1 is empty or only contains whitespace"
        );

        // Test config with whitespace-only entry
        let yaml_with_whitespace = r#"
        obsidian_path: ~/Documents/brain
        simplify_wikilinks:
          - "Ed:"
          - "  "
          - "Valid Entry"
        "#;

        let config_with_whitespace: Config = serde_yaml::from_str(yaml_with_whitespace).unwrap();
        let result = config_with_whitespace.validate_simplify_wikilinks();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "simplify_wikilinks: entry at index 1 is empty or only contains whitespace"
        );
    }
}

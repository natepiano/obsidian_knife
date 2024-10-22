use crate::validated_config::ValidatedConfig;
use serde::Deserialize;
use std::error::Error;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    apply_changes: Option<bool>,
    obsidian_path: String,
    ignore_folders: Option<Vec<String>>,
    cleanup_image_files: Option<bool>,
    simplify_wikilinks: Option<Vec<String>>,
    ignore_text: Option<Vec<String>>,
}

impl Config {
    pub fn validate(self) -> Result<ValidatedConfig, Box<dyn Error + Send + Sync>> {
        let expanded_path = expand_tilde(&self.obsidian_path);
        if !expanded_path.exists() {
            return Err(format!("Path does not exist: {:?}", expanded_path).into());
        }

        let mut ignore_folders = self.validate_ignore_folders(&expanded_path)?;

        // Add the cache folder to ignored_folders
        if let Some(folders) = &mut ignore_folders {
            folders.push(expanded_path.join(crate::constants::CACHE_FOLDER));
        } else {
            ignore_folders = Some(vec![expanded_path.join(crate::constants::CACHE_FOLDER)]);
        }

        let validated_simplify_wikilinks = self.validate_simplify_wikilinks()?;
        let validate_ignore_text = self.validate_ignore_text()?;

        Ok(ValidatedConfig::new(
            self.apply_changes.unwrap_or(false),
            self.cleanup_image_files.unwrap_or(false),
            ignore_folders,
            expanded_path,
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

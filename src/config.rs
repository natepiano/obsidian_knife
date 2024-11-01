use crate::constants::*;
use crate::file_utils::expand_tilde;
use crate::validated_config::ValidatedConfig;
use crate::yaml_utils::deserialize_yaml_frontmatter;
use serde::Deserialize;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    apply_changes: Option<bool>,
    back_populate_file_count: Option<usize>,
    back_populate_file_filter: Option<String>,
    do_not_back_populate: Option<Vec<String>>,
    ignore_folders: Option<Vec<PathBuf>>,
    obsidian_path: String,
    output_folder: Option<String>,
}

impl Config {
    /// Creates a `Config` instance from an Obsidian file by deserializing the YAML front matter.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the Obsidian configuration file.
    ///
    /// # Returns
    ///
    /// * `Ok(Config)` if successful.
    /// * `Err(Box<dyn Error + Send + Sync>)` if reading or deserialization fails.
    pub fn from_obsidian_file(path: &Path) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let expanded_path = expand_tilde(path);
        let contents =
            fs::read_to_string(&expanded_path).map_err(|e| -> Box<dyn Error + Send + Sync> {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("{}{}", ERROR_NOT_FOUND, expanded_path.display()),
                    ))
                } else {
                    Box::new(std::io::Error::new(
                        e.kind(),
                        format!("{}'{}': {}", ERROR_READING, expanded_path.display(), e),
                    ))
                }
            })?;

        deserialize_yaml_frontmatter(&contents)
    }

    /// Validates the `Config` and returns a `ValidatedConfig`.
    ///
    /// # Returns
    ///
    /// * `Ok(ValidatedConfig)` if validation succeeds.
    /// * `Err(Box<dyn Error + Send + Sync>)` if validation fails.
    pub fn validate(self) -> Result<ValidatedConfig, Box<dyn Error + Send + Sync>> {
        let expanded_path = expand_tilde(&self.obsidian_path);
        if !expanded_path.exists() {
            return Err(format!("obsidian path does not exist: {:?}", expanded_path).into());
        }

        // Validate back_populate_file_filter if present
        let validated_back_populate_file_filter =
            if let Some(ref filter) = self.back_populate_file_filter {
                if filter.trim().is_empty() {
                    return Err(ERROR_BACK_POPULATE_FILE_FILTER.into());
                }
                Some(filter.trim().to_string())
            } else {
                None
            };

        // Handle output folder
        let output_folder = if let Some(ref folder) = self.output_folder {
            if folder.trim().is_empty() {
                return Err(ERROR_OUTPUT_FOLDER.into());
            }
            expanded_path.join(folder.trim())
        } else {
            expanded_path.join(DEFAULT_OUTPUT_FOLDER) // Default folder name
        };

        // Add output folder and cache folder to ignored folders
        let mut ignore_folders = self.validate_ignore_folders(&expanded_path)?;
        let mut folders_to_add = vec![output_folder.clone(), expanded_path.join(CACHE_FOLDER)];

        if let Some(ref mut folders) = ignore_folders {
            folders.append(&mut folders_to_add);
        } else {
            ignore_folders = Some(folders_to_add);
        }

        let validated_do_not_back_populate = self.validate_do_not_back_populate()?;

        // Validate `back_populate_file_count`
        let validated_back_populate_file_count = match self.back_populate_file_count {
            Some(count) if count >= 1 => Some(count),
            Some(_) => return Err("back_populate_file_count must be >= 1 or None".into()),
            None => None,
        };

        Ok(ValidatedConfig::new(
            self.apply_changes.unwrap_or(false),
            validated_back_populate_file_count,
            validated_back_populate_file_filter, // Add new parameter
            validated_do_not_back_populate,
            ignore_folders,
            expanded_path,
            output_folder,
        ))
    }

    fn validate_do_not_back_populate(
        &self,
    ) -> Result<Option<Vec<String>>, Box<dyn Error + Send + Sync>> {
        match &self.do_not_back_populate {
            Some(patterns) => {
                let mut validated = Vec::new();
                for (index, pattern) in patterns.iter().enumerate() {
                    let trimmed = pattern.trim();
                    if trimmed.is_empty() {
                        return Err(format!(
                            "do_not_back_populate: entry at index {} is empty or only contains whitespace",
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
                    // Convert PathBuf to string and check if it's empty
                    if folder.to_string_lossy().trim().is_empty() {
                        return Err(format!(
                            "ignore_folders: entry at index {} is empty or only contains whitespace",
                            index
                        )
                            .into());
                    }
                    let full_path = expanded_path.join(folder);
                    validated_folders.push(full_path);
                }
                Some(validated_folders)
            }
        } else {
            None
        };
        Ok(ignore_folders)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use tempfile::TempDir;

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
        }
    }

    #[test]
    fn test_from_obsidian_file_not_found() {
        let result = Config::from_obsidian_file(Path::new("~/nonexistent/config.md"));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("config file not found"));
    }

    #[test]
    fn test_from_obsidian_file_invalid_yaml() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file
            .write_all(b"---\ninvalid: yaml: content:\n---")
            .unwrap();

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

        let yaml = format!(
            r#"
obsidian_path: {}
output_folder: "  ""#,
            temp_dir.path().display()
        );

        let config: Config = serde_yaml::from_str(&yaml).unwrap();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("output_folder cannot be empty"));
    }

    #[test]
    fn test_output_folder_added_to_ignore() {
        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();

        // Create the .obsidian directory
        let obsidian_dir = temp_dir.path().join(".obsidian");
        fs::create_dir(&obsidian_dir).unwrap();

        let yaml = format!(
            r#"
obsidian_path: {}
output_folder: custom_output
ignore_folders:
  - .obsidian"#,
            temp_dir.path().display()
        );

        let config: Config = serde_yaml::from_str(&yaml).unwrap();
        let validated = config.validate().unwrap();

        let ignore_folders = validated.ignore_folders().unwrap();
        let output_path = validated.output_folder();

        assert!(ignore_folders.contains(&output_path.to_path_buf()));
        assert!(ignore_folders.contains(&obsidian_dir));
    }

    #[test]
    fn test_default_output_folder() {
        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();

        let yaml = format!(
            r#"
obsidian_path: {}"#,
            temp_dir.path().display()
        );

        let config: Config = serde_yaml::from_str(&yaml).unwrap();
        let validated = config.validate().unwrap();

        let expected_output = temp_dir.path().join("obsidian_knife");
        assert_eq!(validated.output_folder(), expected_output.as_path());
    }
}

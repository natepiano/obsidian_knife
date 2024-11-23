use crate::config::validated_config::ValidatedConfig;
use crate::constants::*;
use crate::frontmatter::FrontMatter;
use crate::utils::expand_tilde;
use crate::yaml_frontmatter::YamlFrontMatter;
use crate::yaml_frontmatter_struct;
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::PathBuf;

yaml_frontmatter_struct! {
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct Config {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub apply_changes: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub back_populate_file_count: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub back_populate_file_filter: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub do_not_back_populate: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub ignore_folders: Option<Vec<PathBuf>>,
        pub obsidian_path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub operational_timezone: Option<String>,
        pub output_folder: Option<String>,
        #[serde(skip)]
        pub config_file_path: PathBuf,
    }
}

impl Config {
    pub fn from_frontmatter(
        frontmatter: FrontMatter,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let yaml_str = frontmatter.to_yaml_str()?;
        Config::from_yaml_str(&yaml_str).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    /// Validates the `Config` and returns a `ValidatedConfig`.
    ///
    /// # Returns
    ///
    /// * `Ok(ValidatedConfig)` if validation succeeds.
    /// * `Err(Box<dyn Error + Send + Sync>)` if validation fails.
    pub fn validate(&self) -> Result<ValidatedConfig, Box<dyn Error + Send + Sync>> {
        let expanded_obsidian_path = expand_tilde(&self.obsidian_path);
        if !expanded_obsidian_path.exists() {
            return Err(
                format!("obsidian path does not exist: {:?}", expanded_obsidian_path).into(),
            );
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
            expanded_obsidian_path.join(folder.trim())
        } else {
            expanded_obsidian_path.join(DEFAULT_OUTPUT_FOLDER) // Default folder name
        };

        // Add output folder and cache folder to ignored folders
        let mut ignore_folders = self.validate_ignore_folders(&expanded_obsidian_path)?;
        let mut folders_to_add = vec![
            output_folder.clone(),
            expanded_obsidian_path.join(CACHE_FOLDER),
            expanded_obsidian_path.join(OBSIDIAN_HIDDEN_FOLDER),
        ];

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

        // Validate operational timezone if specified
        let validated_timezone = if let Some(ref tz) = self.operational_timezone {
            tz.parse::<Tz>()
                .map_err(|_| format!("Invalid timezone: {}", tz))?;
            tz.to_string()
        } else {
            DEFAULT_TIMEZONE.to_string()
        };

        Ok(ValidatedConfig::new(
            self.apply_changes.unwrap_or(false),
            validated_back_populate_file_count,
            validated_back_populate_file_filter, // Add new parameter
            validated_do_not_back_populate,
            ignore_folders,
            expanded_obsidian_path,
            Some(validated_timezone),
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
        Ok(if let Some(folders) = &self.ignore_folders {
            let mut validated_folders = Vec::new();
            for folder in folders.iter() {
                let full_path = expanded_path.join(folder);
                validated_folders.push(full_path);
            }
            Some(validated_folders)
        } else {
            None
        })
    }
}

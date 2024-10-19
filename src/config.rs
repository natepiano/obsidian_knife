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

        Ok(ValidatedConfig::new(
            self.apply_changes.unwrap_or(false),
            self.cleanup_image_files.unwrap_or(false),
            ignore_folders,
            expanded_path,
            validated_simplify_wikilinks,
        ))
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
        if let Some(patterns) = &self.simplify_wikilinks {
            if patterns.is_empty() {
                Ok(None)
            } else {
                let validated_patterns: Vec<String> = patterns
                    .iter()
                    .enumerate()
                    .filter_map(|(index, pattern)| {
                        if pattern.trim().is_empty() {
                            println!("Warning: simplify_wikilinks: entry at index {} is empty or only contains whitespace", index);
                            None
                        } else {
                            Some(pattern.clone())
                        }
                    })
                    .collect();

                if validated_patterns.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(validated_patterns))
                }
            }
        } else {
            Ok(None)
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

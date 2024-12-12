#[cfg(test)]
pub(crate) mod validated_config_tests;

use crate::{utils, DEFAULT_TIMEZONE, EXTENSION_MARKDOWN};
use chrono_tz::Tz;
use derive_builder::Builder;
use regex::Regex;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Empty back populate file filter")]
    EmptyBackPopulateFileFilter,
    #[error("Empty output folder")]
    EmptyOutputFolder,
    #[error("Back populate file count must be >= 1")]
    InvalidFileProcessLimit,
    #[error("Invalid timezone: {0}")]
    InvalidTimezone(String),
    #[error("Obsidian path does not exist: {0}")]
    InvalidObsidianPath(String),
    #[error("Missing obsidian path")]
    MissingObsidianPath,
    #[error("Field not initialized: {0}")]
    UninitializedField(String),
}

impl From<derive_builder::UninitializedFieldError> for ValidationError {
    fn from(err: derive_builder::UninitializedFieldError) -> Self {
        ValidationError::UninitializedField(err.field_name().to_string())
    }
}

#[derive(Debug, Builder)]
#[builder(
    pattern = "mutable",
    build_fn(
        error = "ValidationError",
        validate = "ValidatedConfigBuilder::validate"
    )
)]
pub struct ValidatedConfig {
    #[builder(default = "false")]
    apply_changes: bool,
    #[builder(default)]
    back_populate_file_filter: Option<String>,
    #[builder(setter(custom), default)]
    #[allow(dead_code)]
    do_not_back_populate: Option<Vec<String>>,
    #[builder(setter(strip_option), default)]
    do_not_back_populate_regexes: Option<Vec<Regex>>,
    #[builder(default)]
    file_process_limit: Option<usize>,
    #[builder(setter(custom), default)]
    ignore_folders: Option<Vec<PathBuf>>,
    #[builder(setter(into))]
    obsidian_path: PathBuf,
    #[builder(default = "DEFAULT_TIMEZONE.to_string()")]
    operational_timezone: String,
    #[builder(setter(custom))]
    output_folder: PathBuf,
}

impl ValidatedConfigBuilder {
    fn validate(&self) -> Result<(), ValidationError> {
        // First check if we have a path at all
        let path = match self.obsidian_path.as_ref() {
            None => return Err(ValidationError::MissingObsidianPath),
            Some(p) => p,
        };

        // Then check if the path exists
        if !path.exists() {
            return Err(ValidationError::InvalidObsidianPath(
                path.display().to_string(),
            ));
        }

        // Validate file_process_limit
        if let Some(Some(count)) = self.file_process_limit {
            if count < 1 {
                return Err(ValidationError::InvalidFileProcessLimit);
            }
        }

        // Validate back_populate_file_filter
        if let Some(Some(filter)) = &self.back_populate_file_filter {
            if filter.trim().is_empty() {
                return Err(ValidationError::EmptyBackPopulateFileFilter);
            }
        }

        // Validate output_folder
        if let Some(folder) = &self.output_folder {
            let path_str = folder.as_os_str().to_string_lossy();
            if path_str.trim().is_empty() {
                return Err(ValidationError::EmptyOutputFolder);
            }
        }

        // Validate timezone
        let timezone = self
            .operational_timezone
            .clone()
            .unwrap_or_else(|| DEFAULT_TIMEZONE.to_string());
        if timezone.parse::<Tz>().is_err() {
            return Err(ValidationError::InvalidTimezone(timezone));
        }

        Ok(())
    }

    pub fn do_not_back_populate(&mut self, val: Option<Vec<String>>) -> &mut Self {
        if let Some(patterns) = val {
            let validated: Vec<String> = patterns
                .iter()
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();

            if !validated.is_empty() {
                self.do_not_back_populate = Some(Some(validated.clone()));
                self.do_not_back_populate_regexes = Some(Some(
                    utils::build_case_insensitive_word_finder(&Some(validated)).unwrap(),
                ));
            } else {
                self.do_not_back_populate = Some(None);
                self.do_not_back_populate_regexes = Some(Some(Vec::new()));
            }
        } else {
            self.do_not_back_populate = Some(None);
            self.do_not_back_populate_regexes = Some(Some(Vec::new()));
        }
        self
    }

    fn resolve_paths(&self, paths: Vec<PathBuf>) -> Vec<PathBuf> {
        if let Some(obsidian_path) = &self.obsidian_path {
            paths
                .iter()
                .map(|p| {
                    if p.is_absolute() {
                        p.clone()
                    } else {
                        obsidian_path.join(p)
                    }
                })
                .collect()
        } else {
            paths
        }
    }

    fn get_or_create_ignore_folders(&self) -> Vec<PathBuf> {
        self.ignore_folders
            .as_ref()
            .and_then(|opt| opt.as_ref())
            .cloned()
            .unwrap_or_default()
    }

    pub fn output_folder(&mut self, val: PathBuf) -> &mut Self {
        // Handle empty path case
        if let Some(obsidian_path) = &self.obsidian_path {
            if let Ok(relative) = val.strip_prefix(obsidian_path) {
                if relative.as_os_str().to_string_lossy().trim().is_empty() {
                    self.output_folder = Some(PathBuf::from(relative));
                    return self;
                }
            }
        }

        let mut folders = self.get_or_create_ignore_folders();
        if !folders.contains(&val) {
            folders.push(val.clone());
        }

        self.ignore_folders = Some(Some(self.resolve_paths(folders)));
        self.output_folder = Some(val);
        self
    }

    pub fn ignore_folders(&mut self, val: Option<Vec<PathBuf>>) -> &mut Self {
        let mut folders = val.unwrap_or_default();
        let obsidian_folder = PathBuf::from(".obsidian");

        if !folders.contains(&obsidian_folder) {
            folders.push(obsidian_folder);
        }

        self.ignore_folders = Some(Some(self.resolve_paths(folders)));
        self
    }
}

impl ValidatedConfig {
    pub fn apply_changes(&self) -> bool {
        self.apply_changes
    }

    pub fn file_process_limit(&self) -> Option<usize> {
        self.file_process_limit
    }

    pub fn back_populate_file_filter(&self) -> Option<String> {
        self.back_populate_file_filter.as_ref().map(|filter| {
            // If it's a wikilink, extract the inner text
            let filter_text = if filter.starts_with("[[") && filter.ends_with("]]") {
                &filter[2..filter.len() - 2]
            } else {
                filter
            };

            // Add .md extension if not present
            if !filter_text.ends_with(EXTENSION_MARKDOWN) {
                format!("{}.md", filter_text)
            } else {
                filter_text.to_string()
            }
        })
    }

    #[cfg(test)]
    pub fn do_not_back_populate(&self) -> Option<&[String]> {
        self.do_not_back_populate.as_deref()
    }

    pub fn do_not_back_populate_regexes(&self) -> Option<&[Regex]> {
        self.do_not_back_populate_regexes.as_deref()
    }

    pub fn ignore_folders(&self) -> Option<&[PathBuf]> {
        self.ignore_folders.as_deref()
    }

    pub fn obsidian_path(&self) -> &Path {
        &self.obsidian_path
    }

    pub fn operational_timezone(&self) -> &str {
        &self.operational_timezone
    }

    pub fn output_folder(&self) -> &Path {
        &self.output_folder
    }
}

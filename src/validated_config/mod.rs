#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod validated_config_tests;

use std::path::Path;
use std::path::PathBuf;

use chrono_tz::Tz;
use derive_builder::Builder;
use derive_builder::UninitializedFieldError;
use regex::Regex;
use thiserror::Error;

use crate::constants::CLOSING_WIKILINK;
use crate::constants::DEFAULT_TIMEZONE;
use crate::constants::MARKDOWN_SUFFIX;
use crate::constants::OPENING_WIKILINK;
use crate::utils;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ChangeMode {
    #[default]
    DryRun,
    Apply,
}

#[derive(Error, Debug)]
pub(crate) enum ValidationError {
    #[error("Empty back populate file filter")]
    EmptyBackPopulateFileFilter,
    #[error("Empty output folder")]
    EmptyOutputFolder,
    #[error("Back populate file count must be >= 1")]
    InvalidFileLimit,
    #[error("Invalid timezone: {0}")]
    InvalidTimezone(String),
    #[error("Obsidian path does not exist: {0}")]
    InvalidObsidianPath(String),
    #[error("Missing obsidian path")]
    MissingObsidianPath,
    #[error("Field not initialized: {0}")]
    UninitializedField(String),
}

impl From<UninitializedFieldError> for ValidationError {
    fn from(err: UninitializedFieldError) -> Self {
        Self::UninitializedField(err.field_name().to_string())
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
pub(crate) struct ValidatedConfig {
    #[builder(default)]
    change_mode:                  ChangeMode,
    #[builder(default)]
    back_populate_file_filter:    Option<String>,
    #[builder(setter(custom), default)]
    #[allow(dead_code, reason = "read only in tests via #[cfg(test)] getter")]
    do_not_back_populate:         Option<Vec<String>>,
    #[builder(setter(strip_option), default)]
    do_not_back_populate_regexes: Option<Vec<Regex>>,
    #[builder(default)]
    file_limit:                   Option<usize>,
    #[builder(setter(custom), default)]
    ignore_folders:               Option<Vec<PathBuf>>,
    #[builder(setter(into))]
    obsidian_path:                PathBuf,
    #[builder(default = "DEFAULT_TIMEZONE.to_string()")]
    operational_timezone:         String,
    #[builder(setter(custom))]
    output_folder:                PathBuf,
}

impl ValidatedConfigBuilder {
    fn validate(&self) -> Result<(), ValidationError> {
        // First check if we have a path at all
        let Some(path) = self.obsidian_path.as_ref() else {
            return Err(ValidationError::MissingObsidianPath);
        };

        // Then check if the path exists
        if !path.exists() {
            return Err(ValidationError::InvalidObsidianPath(
                path.display().to_string(),
            ));
        }

        // Validate file_limit
        if let Some(Some(count)) = self.file_limit
            && count < 1
        {
            return Err(ValidationError::InvalidFileLimit);
        }

        // Validate back_populate_file_filter
        if let Some(Some(filter)) = &self.back_populate_file_filter
            && filter.trim().is_empty()
        {
            return Err(ValidationError::EmptyBackPopulateFileFilter);
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

    pub(crate) fn do_not_back_populate(&mut self, val: Option<Vec<String>>) -> &mut Self {
        if let Some(patterns) = val {
            let validated: Vec<String> = patterns
                .iter()
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();

            if validated.is_empty() {
                self.do_not_back_populate = Some(None);
                self.do_not_back_populate_regexes = Some(Some(Vec::new()));
            } else {
                self.do_not_back_populate = Some(Some(validated.clone()));
                self.do_not_back_populate_regexes =
                    Some(Some(utils::build_case_insensitive_word_finder(&validated)));
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

    pub(crate) fn output_folder(&mut self, val: PathBuf) -> &mut Self {
        // Handle empty path case
        if let Some(obsidian_path) = &self.obsidian_path
            && let Ok(relative) = val.strip_prefix(obsidian_path)
            && relative.as_os_str().to_string_lossy().trim().is_empty()
        {
            self.output_folder = Some(PathBuf::from(relative));
            return self;
        }

        let mut folders = self.get_or_create_ignore_folders();
        if !folders.contains(&val) {
            folders.push(val.clone());
        }

        self.ignore_folders = Some(Some(self.resolve_paths(folders)));
        self.output_folder = Some(val);
        self
    }

    pub(crate) fn ignore_folders(&mut self, val: Option<Vec<PathBuf>>) -> &mut Self {
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
    pub(crate) const fn apply_changes(&self) -> bool {
        match self.change_mode {
            ChangeMode::Apply => true,
            ChangeMode::DryRun => false,
        }
    }

    pub(crate) const fn file_limit(&self) -> Option<usize> { self.file_limit }

    pub(crate) fn back_populate_file_filter(&self) -> Option<String> {
        self.back_populate_file_filter.as_ref().map(|filter| {
            // If it's a wikilink, extract the inner text
            let filter_text =
                if filter.starts_with(OPENING_WIKILINK) && filter.ends_with(CLOSING_WIKILINK) {
                    &filter[2..filter.len() - 2]
                } else {
                    filter
                };

            // Add .md extension if not present
            if filter_text.ends_with(MARKDOWN_SUFFIX) {
                filter_text.to_string()
            } else {
                format!("{filter_text}.md")
            }
        })
    }

    #[cfg(test)]
    pub fn do_not_back_populate(&self) -> Option<&[String]> { self.do_not_back_populate.as_deref() }

    pub(crate) fn do_not_back_populate_regexes(&self) -> Option<&[Regex]> {
        self.do_not_back_populate_regexes.as_deref()
    }

    pub(crate) fn ignore_folders(&self) -> Option<&[PathBuf]> { self.ignore_folders.as_deref() }

    pub(crate) fn obsidian_path(&self) -> &Path { &self.obsidian_path }

    pub(crate) fn operational_timezone(&self) -> &str { &self.operational_timezone }

    pub(crate) fn output_folder(&self) -> &Path { &self.output_folder }
}

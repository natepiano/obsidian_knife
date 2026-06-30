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
use crate::constants::MIN_FILE_LIMIT;
use crate::constants::OBSIDIAN_FOLDER;
use crate::constants::OPENING_WIKILINK;
use crate::support;

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
        // MissingObsidianPath applies before path existence checks.
        let Some(path) = self.obsidian_path.as_ref() else {
            return Err(ValidationError::MissingObsidianPath);
        };

        // `obsidian_path` must exist after `MissingObsidianPath` has passed.
        if !path.exists() {
            return Err(ValidationError::InvalidObsidianPath(
                path.display().to_string(),
            ));
        }

        // `file_limit` must meet `MIN_FILE_LIMIT`.
        if let Some(Some(count)) = self.file_limit
            && count < MIN_FILE_LIMIT
        {
            return Err(ValidationError::InvalidFileLimit);
        }

        // `back_populate_file_filter` must not be blank.
        if let Some(Some(filter)) = &self.back_populate_file_filter
            && filter.trim().is_empty()
        {
            return Err(ValidationError::EmptyBackPopulateFileFilter);
        }

        // `output_folder` must not be blank.
        if let Some(folder) = &self.output_folder {
            let path_str = folder.as_os_str().to_string_lossy();
            if path_str.trim().is_empty() {
                return Err(ValidationError::EmptyOutputFolder);
            }
        }

        // `timezone` must parse as a `Tz`.
        let timezone = self
            .operational_timezone
            .clone()
            .unwrap_or_else(|| DEFAULT_TIMEZONE.to_string());
        if timezone.parse::<Tz>().is_err() {
            return Err(ValidationError::InvalidTimezone(timezone));
        }

        Ok(())
    }

    pub(crate) fn do_not_back_populate(&mut self, patterns: Option<Vec<String>>) -> &mut Self {
        if let Some(patterns) = patterns {
            let validated: Vec<String> = patterns
                .iter()
                .map(|pattern| pattern.trim().to_string())
                .filter(|pattern| !pattern.is_empty())
                .collect();

            if validated.is_empty() {
                self.do_not_back_populate_regexes = Some(Some(Vec::new()));
            } else {
                self.do_not_back_populate_regexes = Some(Some(
                    support::build_case_insensitive_word_finder(&validated),
                ));
            }
        } else {
            self.do_not_back_populate_regexes = Some(Some(Vec::new()));
        }
        self
    }

    fn resolve_paths(&self, paths: Vec<PathBuf>) -> Vec<PathBuf> {
        if let Some(obsidian_path) = &self.obsidian_path {
            paths
                .iter()
                .map(|path| {
                    if path.is_absolute() {
                        path.clone()
                    } else {
                        obsidian_path.join(path)
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
            .and_then(Option::as_ref)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn output_folder(&mut self, folder_path: PathBuf) -> &mut Self {
        // Empty relative output_folder keeps reports at the Obsidian root.
        if let Some(obsidian_path) = &self.obsidian_path
            && let Ok(relative) = folder_path.strip_prefix(obsidian_path)
            && relative.as_os_str().to_string_lossy().trim().is_empty()
        {
            self.output_folder = Some(PathBuf::from(relative));
            return self;
        }

        let mut folders = self.get_or_create_ignore_folders();
        if !folders.contains(&folder_path) {
            folders.push(folder_path.clone());
        }

        self.ignore_folders = Some(Some(self.resolve_paths(folders)));
        self.output_folder = Some(folder_path);
        self
    }

    pub(crate) fn ignore_folders(&mut self, folders: Option<Vec<PathBuf>>) -> &mut Self {
        let mut folders = folders.unwrap_or_default();
        let obsidian_folder = PathBuf::from(OBSIDIAN_FOLDER);

        if !folders.contains(&obsidian_folder) {
            folders.push(obsidian_folder);
        }

        self.ignore_folders = Some(Some(self.resolve_paths(folders)));
        self
    }
}

impl ValidatedConfig {
    pub(crate) const fn change_mode(&self) -> ChangeMode { self.change_mode }

    pub(crate) const fn file_limit(&self) -> Option<usize> { self.file_limit }

    pub(crate) fn back_populate_file_filter(&self) -> Option<String> {
        self.back_populate_file_filter.as_ref().map(|filter| {
            // Wikilink filters use the inner target text before suffix handling.
            let filter_text =
                if filter.starts_with(OPENING_WIKILINK) && filter.ends_with(CLOSING_WIKILINK) {
                    &filter[OPENING_WIKILINK.len()..filter.len() - CLOSING_WIKILINK.len()]
                } else {
                    filter
                };

            // back_populate_file_filter stores paths with MARKDOWN_SUFFIX.
            if filter_text.ends_with(MARKDOWN_SUFFIX) {
                filter_text.to_string()
            } else {
                format!("{filter_text}{MARKDOWN_SUFFIX}")
            }
        })
    }
    pub(crate) fn do_not_back_populate_regexes(&self) -> Option<&[Regex]> {
        self.do_not_back_populate_regexes.as_deref()
    }

    pub(crate) fn ignore_folders(&self) -> Option<&[PathBuf]> { self.ignore_folders.as_deref() }

    pub(crate) fn obsidian_path(&self) -> &Path { &self.obsidian_path }

    pub(crate) fn operational_timezone(&self) -> &str { &self.operational_timezone }

    pub(crate) fn output_folder(&self) -> &Path { &self.output_folder }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde_yaml::from_str;
    use tempfile::TempDir;

    use super::ValidatedConfigBuilder;
    use super::*;
    use crate::config::Config;
    use crate::constants::DEFAULT_OUTPUT_FOLDER;
    use crate::constants::DEFAULT_TIMEZONE;
    use crate::constants::MARKDOWN_SUFFIX;
    use crate::constants::OBSIDIAN_FOLDER;
    use crate::test_support;

    #[test]
    fn test_back_populate_file_filter() {
        let expected_markdown_file = format!("test_file{MARKDOWN_SUFFIX}");
        let temp_dir = TempDir::new().unwrap();
        let validated_config =
            test_support::get_test_validated_config(&temp_dir, Some("test_file"));

        assert_eq!(
            validated_config.back_populate_file_filter(),
            Some(expected_markdown_file.clone())
        );

        let validated_config =
            test_support::get_test_validated_config(&temp_dir, Some("[[test_file]]"));
        assert_eq!(
            validated_config.back_populate_file_filter(),
            Some(expected_markdown_file.clone())
        );

        let validated_config = test_support::get_test_validated_config(
            &temp_dir,
            Some(expected_markdown_file.as_str()),
        );
        assert_eq!(
            validated_config.back_populate_file_filter(),
            Some(expected_markdown_file.clone())
        );

        let validated_config = test_support::get_test_validated_config(
            &temp_dir,
            Some(format!("[[{expected_markdown_file}]]").as_str()),
        );
        assert_eq!(
            validated_config.back_populate_file_filter(),
            Some(expected_markdown_file)
        );

        let validated_config = test_support::get_test_validated_config(&temp_dir, None);
        assert_eq!(validated_config.back_populate_file_filter(), None);
    }

    #[test]
    fn test_preserve_obsidian_in_ignore_folders() {
        let temp_dir = TempDir::new().unwrap();
        let obsidian_path = temp_dir.path().to_path_buf();

        let mut builder = ValidatedConfigBuilder::default();
        builder.obsidian_path(obsidian_path.clone());

        builder.ignore_folders(Some(vec![PathBuf::from(OBSIDIAN_FOLDER)]));

        builder.output_folder(obsidian_path.join("custom_output"));

        let validated_config = builder.build().unwrap();
        let ignore_folders = validated_config.ignore_folders().unwrap();

        let obsidian_dir = obsidian_path.join(OBSIDIAN_FOLDER);
        let output_dir = obsidian_path.join("custom_output");

        assert!(
            ignore_folders.contains(&obsidian_dir),
            "Should contain .obsidian directory"
        );
        assert!(
            ignore_folders.contains(&output_dir),
            "Should contain output directory"
        );
    }

    #[test]
    fn test_timezone_validation() {
        let temp_dir = TempDir::new().unwrap();

        let yaml = format!(
            r#"
    obsidian_path: {}
    operational_timezone: "America/Los_Angeles""#,
            temp_dir.path().display()
        );

        let config: Config = from_str(&yaml).unwrap();
        let result = config.validate();
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().operational_timezone(),
            "America/Los_Angeles"
        );

        let yaml = format!(
            r#"
    obsidian_path: {}
    operational_timezone: "Invalid/Timezone""#,
            temp_dir.path().display()
        );

        let config: Config = from_str(&yaml).unwrap();
        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid timezone: Invalid/Timezone")
        );
    }

    #[test]
    fn test_default_timezone() {
        let temp_dir = TempDir::new().unwrap();

        let yaml = format!(
            r"
    obsidian_path: {}",
            temp_dir.path().display()
        );

        let config: Config = from_str(&yaml).unwrap();
        let result = config.validate();
        assert!(result.is_ok());
        assert_eq!(result.unwrap().operational_timezone(), DEFAULT_TIMEZONE);
    }

    #[test]
    fn test_default_output_folder() {
        let temp_dir = TempDir::new().unwrap();

        let yaml = format!(
            r"
    obsidian_path: {}",
            temp_dir.path().display()
        );

        let config: Config = from_str(&yaml).unwrap();
        let validated = config.validate().unwrap();

        let expected_output = temp_dir.path().join(DEFAULT_OUTPUT_FOLDER);
        assert_eq!(validated.output_folder(), expected_output.as_path());
    }

    #[test]
    fn test_output_folder_added_to_ignore() {
        let temp_dir = TempDir::new().unwrap();

        let obsidian_dir = temp_dir.path().join(OBSIDIAN_FOLDER);
        fs::create_dir(&obsidian_dir).unwrap();

        let yaml = format!(
            r"
    obsidian_path: {}
    output_folder: custom_output
    ignore_folders:
      - {}",
            temp_dir.path().display(),
            OBSIDIAN_FOLDER
        );

        let config: Config = from_str(&yaml).unwrap();
        let validated = config.validate().unwrap();

        let ignore_folders = validated.ignore_folders().unwrap();
        let output_path = validated.output_folder();

        assert!(ignore_folders.contains(&output_path.to_path_buf()));
        assert!(ignore_folders.contains(&obsidian_dir));
    }

    #[test]
    fn test_validate_empty_output_folder() {
        let temp_dir = TempDir::new().unwrap();

        let yaml = format!(
            r#"
    obsidian_path: {}
    output_folder: "  ""#,
            temp_dir.path().display()
        );

        let config: Config = from_str(&yaml).unwrap();
        let result = config.validate();
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(
            *err.downcast_ref::<ValidationError>().unwrap(),
            ValidationError::EmptyOutputFolder
        ));
    }

    #[test]
    fn test_invalid_back_populate_count() {
        let temp_dir = TempDir::new().unwrap();
        let result = test_support::get_test_validated_config_result(&temp_dir, |builder| {
            builder.file_limit(Some(0));
        });

        assert!(matches!(
            result.unwrap_err(),
            ValidationError::InvalidFileLimit
        ));

        let result = test_support::get_test_validated_config_result(&temp_dir, |builder| {
            builder.file_limit(Some(1));
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_empty_back_populate_file_filter() {
        let temp_dir = TempDir::new().unwrap();
        let result = test_support::get_test_validated_config_result(&temp_dir, |builder| {
            builder.back_populate_file_filter(Some("   ".to_string()));
        });

        assert!(matches!(
            result.unwrap_err(),
            ValidationError::EmptyBackPopulateFileFilter
        ));

        let result = test_support::get_test_validated_config_result(&temp_dir, |builder| {
            builder.back_populate_file_filter(Some("valid_filter".to_string()));
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_obsidian_path() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent_path = temp_dir.path().join("nonexistent");

        let result = test_support::get_test_validated_config_result(&temp_dir, |builder| {
            builder.obsidian_path(nonexistent_path.clone());
        });

        assert!(matches!(
            result.unwrap_err(),
            ValidationError::InvalidObsidianPath(path) if path == nonexistent_path.display().to_string()
        ));
    }

    #[test]
    fn test_missing_obsidian_path() {
        let temp_dir = TempDir::new().unwrap();
        let mut builder = ValidatedConfigBuilder::default();
        // Don't set obsidian_path at all
        builder.output_folder(temp_dir.path().join("output"));

        let result = builder.build();
        assert!(matches!(
            result.unwrap_err(),
            ValidationError::MissingObsidianPath
        ));
    }

    #[test]
    fn test_uninitialized_fields() {
        let temp_dir = TempDir::new().unwrap();
        let mut builder = ValidatedConfigBuilder::default();

        // Set obsidian_path but not output_folder
        builder.obsidian_path(temp_dir.path().to_path_buf());
        let result = builder.build();

        assert!(matches!(
            result.unwrap_err(),
            ValidationError::UninitializedField(field) if field == "output_folder"
        ));

        let mut builder = ValidatedConfigBuilder::default();
        builder.output_folder(temp_dir.path().join("output"));
        let result = builder.build();

        assert!(matches!(
            result.unwrap_err(),
            ValidationError::MissingObsidianPath
        ));
    }

    #[test]
    fn test_multiple_validation_errors() {
        let temp_dir = TempDir::new().unwrap();
        let result = test_support::get_test_validated_config_result(&temp_dir, |builder| {
            builder
                .file_limit(Some(0))
                .back_populate_file_filter(Some(String::new()));
        });

        assert!(matches!(
            result.unwrap_err(),
            ValidationError::InvalidFileLimit
        ));
    }

    #[test]
    fn test_all_validation_passes() {
        let temp_dir = TempDir::new().unwrap();
        let result = test_support::get_test_validated_config_result(&temp_dir, |builder| {
            builder
                .file_limit(Some(1))
                .back_populate_file_filter(Some("valid_filter".to_string()))
                .operational_timezone(DEFAULT_TIMEZONE.to_string());
        });

        assert!(result.is_ok());
    }

    #[test]
    fn test_timezone_edge_cases() {
        let temp_dir = TempDir::new().unwrap();

        let result = test_support::get_test_validated_config_result(&temp_dir, |builder| {
            builder.operational_timezone(String::new());
        });
        assert!(matches!(
            result.unwrap_err(),
            ValidationError::InvalidTimezone(_)
        ));

        let result = test_support::get_test_validated_config_result(&temp_dir, |builder| {
            builder.operational_timezone("America/New@York".to_string());
        });
        assert!(matches!(
            result.unwrap_err(),
            ValidationError::InvalidTimezone(_)
        ));
    }

    #[test]
    fn test_output_folder_edge_cases() {
        let temp_dir = TempDir::new().unwrap();

        let absolute_path = temp_dir.path().join("absolute_output");
        let result = test_support::get_test_validated_config_result(&temp_dir, |builder| {
            builder.output_folder(absolute_path.clone());
        });
        assert!(result.is_ok());
        let validated_config = result.unwrap();
        assert!(
            validated_config
                .ignore_folders()
                .unwrap()
                .contains(&absolute_path)
        );

        let result = test_support::get_test_validated_config_result(&temp_dir, |builder| {
            builder.output_folder(PathBuf::from("relative_output"));
        });
        assert!(result.is_ok());

        let validated_config = result.unwrap();
        let expected_path = temp_dir.path().join("relative_output");
        assert!(
            validated_config
                .ignore_folders()
                .unwrap()
                .contains(&expected_path),
            "\nExpected path: {:?}\nIgnore folders: {:?}",
            expected_path,
            validated_config.ignore_folders().unwrap()
        );
    }
}

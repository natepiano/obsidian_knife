use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde_yaml::Value;

use crate::constants::DEFAULT_OUTPUT_FOLDER;
use crate::constants::DEFAULT_TIMEZONE;
use crate::frontmatter::FrontMatter;
use crate::support;
use crate::validated_config::ChangeMode;
use crate::validated_config::ValidatedConfig;
use crate::validated_config::ValidatedConfigBuilder;
use crate::yaml_frontmatter::YamlFrontMatter;
use crate::yaml_frontmatter_struct;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum ConfiguredChanges {
    #[default]
    Unspecified,
    DryRun,
    Apply,
}

impl ConfiguredChanges {
    pub(crate) const fn is_unspecified(&self) -> bool { matches!(self, Self::Unspecified) }

    const fn resolve(&self) -> ChangeMode {
        match self {
            Self::Apply => ChangeMode::Apply,
            Self::DryRun | Self::Unspecified => ChangeMode::DryRun,
        }
    }
}

impl Serialize for ConfiguredChanges {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Apply => true.serialize(serializer),
            Self::DryRun | Self::Unspecified => false.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ConfiguredChanges {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        bool::deserialize(deserializer).map(Self::from)
    }
}

impl From<bool> for ConfiguredChanges {
    fn from(apply_changes: bool) -> Self {
        if apply_changes {
            Self::Apply
        } else {
            Self::DryRun
        }
    }
}

yaml_frontmatter_struct! {
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct Config {
        #[serde(
            default,
            rename = "apply_changes",
            skip_serializing_if = "ConfiguredChanges::is_unspecified"
        )]
        pub configured_changes: ConfiguredChanges,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub back_populate_file_filter: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub do_not_back_populate: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub file_limit: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub ignore_folders: Option<Vec<PathBuf>>,
        pub obsidian_path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub operational_timezone: Option<String>,
        pub output_folder: Option<String>,
        #[serde(skip)]
        pub file_path: PathBuf,
    }
}

impl Config {
    pub(crate) const fn change_mode(&self) -> ChangeMode { self.configured_changes.resolve() }

    pub(crate) fn validate(&self) -> Result<ValidatedConfig, Box<dyn Error + Send + Sync>> {
        ValidatedConfigBuilder::default()
            .change_mode(self.change_mode())
            .back_populate_file_filter(self.back_populate_file_filter.clone())
            .do_not_back_populate(self.do_not_back_populate.clone())
            .file_limit(self.file_limit)
            .ignore_folders(self.ignore_folders.clone())
            .obsidian_path(support::expand_tilde(&self.obsidian_path))
            .operational_timezone(
                self.operational_timezone
                    .clone()
                    .unwrap_or_else(|| DEFAULT_TIMEZONE.to_string()),
            )
            .output_folder(
                support::expand_tilde(&self.obsidian_path).join(
                    self.output_folder
                        .as_deref()
                        .unwrap_or(DEFAULT_OUTPUT_FOLDER),
                ),
            )
            .build()
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }
}

impl TryFrom<&FrontMatter> for Config {
    type Error = Box<dyn Error + Send + Sync>;

    fn try_from(frontmatter: &FrontMatter) -> Result<Self, Self::Error> {
        let yaml = frontmatter.to_yaml_str()?;
        Self::from_yaml_str(&yaml).map_err(|e| Box::new(e) as Self::Error)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde_yaml::from_str;
    use tempfile::TempDir;

    use super::Config;
    use super::ConfiguredChanges;
    use crate::constants::DEFAULT_TIMEZONE;
    use crate::constants::ERROR_NOT_FOUND;
    use crate::constants::OBSIDIAN_FOLDER;
    use crate::frontmatter::FrontMatter;
    use crate::markdown_file::MarkdownFile;
    use crate::test_support as test_utils;
    use crate::test_support::TestFileBuilder;
    use crate::validated_config::ChangeMode;
    use crate::yaml_frontmatter::YamlFrontMatter;

    fn create_test_environment() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();

        // Create Obsidian vault structure
        let obsidian_path = temp_dir.path().join("vault");
        fs::create_dir(&obsidian_path).unwrap();
        fs::create_dir(obsidian_path.join(OBSIDIAN_FOLDER)).unwrap();

        let canonical_path = obsidian_path
            .canonicalize()
            .expect("Failed to get canonical path");

        // Create output directory
        fs::create_dir(canonical_path.join("output")).unwrap();

        // Create config file using `TestFileBuilder`
        let config_yaml = format!(
            "obsidian_path: {}\napply_changes: false\noutput_folder: output",
            canonical_path.to_string_lossy()
        );

        let config_path = TestFileBuilder::new()
            .with_custom_frontmatter(config_yaml)
            .create(&temp_dir, "config.md");

        assert!(
            config_path.exists(),
            "Config file does not exist after creation"
        );

        (temp_dir, config_path)
    }

    #[test]
    fn test_reset_apply_changes() {
        let temp_dir = TempDir::new().unwrap();
        let yaml = format!(
            r#"
obsidian_path: /test/path
apply_changes: true
file_limit: 5
back_populate_file_filter: "*test*"
operational_timezone: {DEFAULT_TIMEZONE}
do_not_back_populate:
 - "*.png"
ignore_folders:
 - .git
output_folder: output"#
        );

        let config_path = TestFileBuilder::new()
            .with_custom_frontmatter(yaml)
            .create(&temp_dir, "config.md");

        let mut markdown_file = test_utils::get_test_markdown_file(config_path.clone());
        let mut config = Config::try_from(markdown_file.frontmatter.as_ref().unwrap()).unwrap();

        // `Config::try_from` preserves the initial frontmatter values.
        assert_eq!(config.configured_changes, ConfiguredChanges::Apply);
        assert_eq!(config.file_limit, Some(5));
        assert_eq!(config.back_populate_file_filter, Some("*test*".to_string()));
        assert_eq!(config.do_not_back_populate, Some(vec!["*.png".to_string()]));
        assert_eq!(config.ignore_folders, Some(vec![PathBuf::from(".git")]));
        assert_eq!(config.output_folder, Some("output".to_string()));
        assert_eq!(config.obsidian_path, "/test/path".to_string());

        // Test apply_changes update
        config.configured_changes = ConfiguredChanges::DryRun;
        let config_yaml = config.to_yaml_str().unwrap();

        let updated_frontmatter = FrontMatter::from_yaml_str(&config_yaml).unwrap();
        markdown_file.frontmatter = Some(updated_frontmatter);
        markdown_file
            .frontmatter
            .as_mut()
            .unwrap()
            .set_date_modified_now(DEFAULT_TIMEZONE);
        markdown_file.persist().unwrap();

        // Verify all fields after update
        let new_markdown_file = test_utils::get_test_markdown_file(config_path);
        let new_config = Config::try_from(&new_markdown_file.frontmatter.unwrap()).unwrap();

        assert_eq!(new_config.configured_changes, ConfiguredChanges::DryRun);
        assert_eq!(new_config.file_limit, Some(5));
        assert_eq!(
            new_config.back_populate_file_filter,
            Some("*test*".to_string())
        );
        assert_eq!(
            new_config.do_not_back_populate,
            Some(vec!["*.png".to_string()])
        );
        assert_eq!(new_config.ignore_folders, Some(vec![PathBuf::from(".git")]));
        assert_eq!(new_config.output_folder, Some("output".to_string()));
        assert_eq!(new_config.obsidian_path, "/test/path".to_string());
    }

    #[test]
    fn test_config_from_markdown() {
        let temp_dir = TempDir::new().unwrap();

        let yaml = r"
obsidian_path: ~/Documents/brain
apply_changes: false
cleanup_image_files: true";

        let config_path = TestFileBuilder::new()
            .with_custom_frontmatter(yaml.to_string())
            .create(&temp_dir, "config.md");

        let markdown_file = test_utils::get_test_markdown_file(config_path);
        let config = Config::try_from(&markdown_file.frontmatter.unwrap()).unwrap();

        assert_eq!(config.obsidian_path, "~/Documents/brain");
        assert_eq!(config.configured_changes, ConfiguredChanges::DryRun);
    }

    #[test]
    fn test_config_file_not_found() {
        let nonexistent_path = PathBuf::from("nonexistent/config.md");
        let result = MarkdownFile::new(nonexistent_path.clone(), DEFAULT_TIMEZONE);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(&format!(
            "{}{}",
            ERROR_NOT_FOUND,
            nonexistent_path.display()
        )));
    }

    #[test]
    fn test_config_invalid_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let invalid_yaml = r"---
invalid: yaml: content:
---
";

        let config_path = TestFileBuilder::new()
            .with_content(invalid_yaml.to_string())
            .create(&temp_dir, "config.md");

        let markdown_file = test_utils::get_test_markdown_file(config_path);
        let result = Config::try_from(&markdown_file.frontmatter.unwrap_or_default());

        assert!(result.is_err());
    }

    #[test]
    fn test_config_with_output_folder() {
        let yaml = r"
obsidian_path: ~/Documents/brain
output_folder: custom_output
apply_changes: false";

        let config: Config = from_str(yaml).unwrap();
        assert_eq!(config.output_folder, Some("custom_output".to_string()));
    }

    #[test]
    fn test_config_without_output_folder() {
        let yaml = r"
obsidian_path: ~/Documents/brain
apply_changes: false";

        let config: Config = from_str(yaml).unwrap();
        assert_eq!(config.output_folder, None);
    }

    #[test]
    fn test_process_config_with_valid_setup() {
        let (_temp_dir, config_path) = create_test_environment();

        let markdown_file = test_utils::get_test_markdown_file(config_path);
        let config = Config::try_from(&markdown_file.frontmatter.unwrap()).unwrap();

        let validated_config = config.validate().unwrap();
        assert_eq!(validated_config.change_mode(), ChangeMode::DryRun);
        assert!(validated_config.obsidian_path().exists());
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod config_tests;

use std::error::Error;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;

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

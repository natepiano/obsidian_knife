#[cfg(test)]
mod config_tests;

use crate::constants::*;
use crate::frontmatter::FrontMatter;
use crate::validated_config::{ValidatedConfig, ValidatedConfigBuilder};
use crate::yaml_frontmatter::YamlFrontMatter;
use crate::{utils, yaml_frontmatter_struct};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::PathBuf;

yaml_frontmatter_struct! {
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct Config {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub apply_changes: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub back_populate_file_filter: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub do_not_back_populate: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub file_process_limit: Option<usize>,
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

    pub fn validate(&self) -> Result<ValidatedConfig, Box<dyn Error + Send + Sync>> {
        ValidatedConfigBuilder::default()
            .apply_changes(self.apply_changes.unwrap_or(false))
            .back_populate_file_filter(self.back_populate_file_filter.clone())
            .do_not_back_populate(self.do_not_back_populate.clone())
            .file_process_limit(self.file_process_limit)
            .ignore_folders(self.ignore_folders.clone())
            .obsidian_path(utils::expand_tilde(&self.obsidian_path))
            .operational_timezone(
                self.operational_timezone
                    .clone()
                    .unwrap_or_else(|| DEFAULT_TIMEZONE.to_string()),
            )
            .output_folder(
                utils::expand_tilde(&self.obsidian_path).join(
                    self.output_folder
                        .as_deref()
                        .unwrap_or(DEFAULT_OUTPUT_FOLDER),
                ),
            )
            .build()
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }
}

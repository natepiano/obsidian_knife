use crate::utils::build_case_insensitive_word_finder;
use regex::Regex;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ValidatedConfig {
    apply_changes: bool,
    back_populate_file_count: Option<usize>,
    back_populate_file_filter: Option<String>, // New field
    do_not_back_populate: Option<Vec<String>>,
    do_not_back_populate_regexes: Option<Vec<Regex>>,
    ignore_folders: Option<Vec<PathBuf>>, // Changed from String to PathBuf
    obsidian_path: PathBuf,
    output_folder: PathBuf,
}

impl ValidatedConfig {
    pub fn new(
        apply_changes: bool,
        back_populate_file_count: Option<usize>,
        back_populate_file_filter: Option<String>,
        do_not_back_populate: Option<Vec<String>>,
        ignore_folders: Option<Vec<PathBuf>>,
        obsidian_path: PathBuf,
        output_folder: PathBuf,
    ) -> Self {
        // Build regexes if we have patterns to exclude
        let do_not_back_populate_regexes =
            build_case_insensitive_word_finder(&do_not_back_populate);

        Self {
            apply_changes,
            back_populate_file_count,
            back_populate_file_filter,
            do_not_back_populate,
            do_not_back_populate_regexes,
            ignore_folders,
            obsidian_path,
            output_folder,
        }
    }

    pub fn apply_changes(&self) -> bool {
        self.apply_changes
    }

    pub fn back_populate_file_count(&self) -> Option<usize> {
        self.back_populate_file_count
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
            if !filter_text.ends_with(".md") {
                format!("{}.md", filter_text)
            } else {
                filter_text.to_string()
            }
        })
    }

    pub fn do_not_back_populate(&self) -> Option<&[String]> {
        self.do_not_back_populate.as_deref()
    }

    pub fn do_not_back_populate_regexes(&self) -> Option<&[Regex]> {
        self.do_not_back_populate_regexes.as_deref()
    }

    pub fn ignore_folders(&self) -> Option<&[PathBuf]> {
        // Changed return type
        self.ignore_folders.as_deref()
    }

    pub fn obsidian_path(&self) -> &Path {
        &self.obsidian_path
    }

    pub fn output_folder(&self) -> &Path {
        &self.output_folder
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_back_populate_file_filter() {
        let temp_dir = TempDir::new().unwrap();
        let config = ValidatedConfig::new(
            false,
            None,
            Some("test_file".to_string()),
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        assert_eq!(
            config.back_populate_file_filter(),
            Some("test_file.md".to_string())
        );

        // Test with wikilink format
        let config = ValidatedConfig::new(
            false,
            None,
            Some("[[test_file]]".to_string()),
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        assert_eq!(
            config.back_populate_file_filter(),
            Some("test_file.md".to_string())
        );

        // Test with existing .md extension
        let config = ValidatedConfig::new(
            false,
            None,
            Some("test_file.md".to_string()),
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        assert_eq!(
            config.back_populate_file_filter(),
            Some("test_file.md".to_string())
        );

        // Test with wikilink and .md extension
        let config = ValidatedConfig::new(
            false,
            None,
            Some("[[test_file.md]]".to_string()),
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        assert_eq!(
            config.back_populate_file_filter(),
            Some("test_file.md".to_string())
        );

        // Test with None
        let config = ValidatedConfig::new(
            false,
            None,
            None::<String>,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        assert_eq!(config.back_populate_file_filter(), None);
    }
}

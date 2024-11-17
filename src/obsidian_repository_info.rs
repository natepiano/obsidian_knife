use crate::markdown_file_info::MarkdownFileInfo;
use crate::scan::ImageInfo;
use crate::wikilink_types::Wikilink;
use aho_corasick::AhoCorasick;
use chrono::Local;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Default)]
pub struct ObsidianRepositoryInfo {
    pub markdown_files: Vec<MarkdownFileInfo>,
    pub image_map: HashMap<PathBuf, ImageInfo>,
    pub other_files: Vec<PathBuf>,
    pub wikilinks_ac: Option<AhoCorasick>,
    pub wikilinks_sorted: Vec<Wikilink>,
}

impl ObsidianRepositoryInfo {
    pub fn update_modified_dates(&mut self, paths: &[PathBuf]) {
        let today = Local::now();
        let paths_set: HashSet<_> = paths.iter().collect();

        self.markdown_files
            .iter_mut()
            .filter(|file_info| paths_set.contains(&file_info.path))
            .filter_map(|file_info| file_info.frontmatter.as_mut())
            .for_each(|frontmatter| frontmatter.set_date_modified(today));
    }
}

#[cfg(test)]
mod update_modified_dates_tests {
    use super::*;
    use tempfile::TempDir;
    use chrono::{Datelike, Local};
    use crate::test_utils::create_test_date_create_fix_markdown_file;

    #[test]
    fn test_update_modified_dates_changes_frontmatter() {
        let temp_dir = TempDir::new().unwrap();

        // Use the existing helper function with no date_created_fix
        let file_path = create_test_date_create_fix_markdown_file(&temp_dir, None, "test1.md");

        let mut repo_info = ObsidianRepositoryInfo::default();
        let markdown_file = MarkdownFileInfo::new(file_path.clone()).unwrap();
        repo_info.markdown_files.push(markdown_file);

        // Update the modified dates
        repo_info.update_modified_dates(&[file_path.clone()]);

        let frontmatter = repo_info.markdown_files[0].frontmatter.as_ref().unwrap();

        // Get today's date for comparison
        let today = Local::now();
        let expected_date = format!(
            "[[{}-{:02}-{:02}]]",
            today.year(),
            today.month(),
            today.day()
        );

        assert_eq!(frontmatter.date_modified(), Some(&expected_date), "Modified date should be today's date");
        assert_eq!(frontmatter.date_created(), Some(&"[[2024-01-15]]".to_string()), "Created date should not have changed");
        assert!(frontmatter.needs_persist(), "needs_persist should be true");
    }

    #[test]
    fn test_update_modified_dates_only_updates_specified_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create two files using the helper function
        let file_path1 = create_test_date_create_fix_markdown_file(&temp_dir, None, "test1.md");
        let file_path2 = create_test_date_create_fix_markdown_file(&temp_dir, None, "test2.md");

        let mut repo_info = ObsidianRepositoryInfo::default();
        repo_info.markdown_files.push(MarkdownFileInfo::new(file_path1.clone()).unwrap());
        repo_info.markdown_files.push(MarkdownFileInfo::new(file_path2.clone()).unwrap());

        // Only update the first file
        repo_info.update_modified_dates(&[file_path1]);

        let file1 = &repo_info.markdown_files[0];
        let file2 = &repo_info.markdown_files[1];

        // Get today's date for comparison
        let today = Local::now();
        let expected_date = format!(
            "[[{}-{:02}-{:02}]]",
            today.year(),
            today.month(),
            today.day()
        );

        // First file should have new date and needs_persist
        assert_eq!(file1.frontmatter.as_ref().unwrap().date_modified(), Some(&expected_date));
        assert!(file1.frontmatter.as_ref().unwrap().needs_persist());

        // Second file should have original date and not need persist
        assert_eq!(file2.frontmatter.as_ref().unwrap().date_modified(), Some(&"[[2024-01-15]]".to_string()));
        assert!(!file2.frontmatter.as_ref().unwrap().needs_persist());
    }
}

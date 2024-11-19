#[cfg(test)]
mod persist_file_tests;
#[cfg(test)]
mod update_modified_tests;

use crate::markdown_file_info::MarkdownFileInfo;
use crate::scan::ImageInfo;
use crate::wikilink_types::Wikilink;
use aho_corasick::AhoCorasick;
use std::collections::{HashMap, HashSet};
use std::error::Error;
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
    pub fn persist(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        for file_info in &self.markdown_files {
            if let Some(frontmatter) = &file_info.frontmatter {
                if frontmatter.needs_persist() {
                    file_info.persist()?;
                }
            }
        }
        Ok(())
    }

    pub fn update_modified_dates(&mut self, paths: &[PathBuf]) {
        let paths_set: HashSet<_> = paths.iter().collect();

        self.markdown_files
            .iter_mut()
            .filter(|file_info| paths_set.contains(&file_info.path))
            .filter_map(|file_info| file_info.frontmatter.as_mut())
            .for_each(|frontmatter| frontmatter.set_date_modified_now());
    }
}

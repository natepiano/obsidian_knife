#[cfg(test)]
mod persist_file_tests;
#[cfg(test)]
mod update_modified_tests;

use crate::markdown_files::MarkdownFiles;
use crate::markdown_file_info::MarkdownFileInfo;
use crate::scan::ImageInfo;
use crate::wikilink_types::Wikilink;
use aho_corasick::AhoCorasick;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

#[derive(Default)]
pub struct ObsidianRepositoryInfo {
    pub markdown_files: MarkdownFiles,
    pub image_map: HashMap<PathBuf, ImageInfo>,
    pub other_files: Vec<PathBuf>,
    pub wikilinks_ac: Option<AhoCorasick>,
    pub wikilinks_sorted: Vec<Wikilink>,
}

impl ObsidianRepositoryInfo {

    pub fn persist(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.markdown_files.persist_all()
    }

    pub fn update_modified_dates_for_cleanup_images(&mut self, paths: &[PathBuf]) {
        self.markdown_files.update_modified_dates_for_cleanup_images(paths);
    }
}

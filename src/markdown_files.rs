use crate::markdown_file::{BackPopulateMatch, MarkdownFile};
use crate::validated_config::ValidatedConfig;
use crate::wikilink::Wikilink;

use aho_corasick::AhoCorasick;
use rayon::prelude::*;
use std::error::Error;
use vecollect::collection;

#[derive(Debug, Default)]
#[collection(field = "files")]
pub struct MarkdownFiles {
    pub(crate) files: Vec<MarkdownFile>,
    pub(crate) file_limit: Option<usize>,
}

impl MarkdownFiles {
    pub fn new(files: Vec<MarkdownFile>, file_limit: Option<usize>) -> Self {
        Self { files, file_limit }
    }

    pub fn process_files_for_back_populate_matches(
        &mut self,
        config: &ValidatedConfig,
        sorted_wikilinks: Vec<&Wikilink>,
        ac: &AhoCorasick,
    ) {
        // this use of rayon generally makes it go about 100ms faster
        self.par_iter_mut().for_each(|markdown_file| {
            if !cfg!(test) {
                if let Some(filter) = config.back_populate_file_filter() {
                    if !markdown_file.path.ends_with(filter) {
                        return;
                    }
                }
            }

            markdown_file.process_file_for_back_populate_replacements(
                &sorted_wikilinks,
                config,
                ac,
            );
        });
    }

    pub fn unambiguous_matches(&self) -> Vec<BackPopulateMatch> {
        self.iter()
            .flat_map(|file| file.matches.unambiguous.clone())
            .collect()
    }

    pub fn persist_all(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        for file_info in &self.files {
            file_info.persist()?;
        }
        Ok(())
    }

    pub fn total_files_to_persist(&self) -> usize {
        self.iter()
            .filter(|file_info| {
                file_info
                    .frontmatter
                    .as_ref()
                    .is_some_and(|fm| fm.needs_persist())
            })
            .count()
    }

    pub fn files_to_persist(&self) -> Self {
        let mut files_to_persist: Vec<MarkdownFile> = self
            .iter()
            .filter(|file_info| {
                file_info
                    .frontmatter
                    .as_ref()
                    .is_some_and(|fm| fm.needs_persist())
            })
            .cloned()
            .collect();

        files_to_persist.sort_by(|a, b| a.path.cmp(&b.path));

        let total_files = files_to_persist.len();
        let count = self.file_limit.unwrap_or(total_files);

        Self {
            files: files_to_persist.into_iter().take(count).collect(),
            file_limit: self.file_limit,
        }
    }
}

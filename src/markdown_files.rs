use std::error::Error;

use aho_corasick::AhoCorasick;
use derive_more::Deref;
use derive_more::DerefMut;
use derive_more::IntoIterator;
use rayon::prelude::*;

use crate::markdown_file::BackPopulateMatch;
use crate::markdown_file::MarkdownFile;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::Wikilink;

#[derive(Debug, Default, Deref, DerefMut, IntoIterator)]
pub struct MarkdownFiles {
    #[deref]
    #[deref_mut]
    #[into_iterator]
    pub(super) files:      Vec<MarkdownFile>,
    pub(super) file_limit: Option<usize>,
}

impl FromIterator<MarkdownFile> for MarkdownFiles {
    fn from_iter<I: IntoIterator<Item = MarkdownFile>>(iter: I) -> Self {
        Self {
            files:      iter.into_iter().collect(),
            file_limit: None,
        }
    }
}

impl<'a> IntoIterator for &'a MarkdownFiles {
    type Item = &'a MarkdownFile;
    type IntoIter = std::slice::Iter<'a, MarkdownFile>;

    fn into_iter(self) -> Self::IntoIter { self.files.iter() }
}

impl<'a> IntoIterator for &'a mut MarkdownFiles {
    type Item = &'a mut MarkdownFile;
    type IntoIter = std::slice::IterMut<'a, MarkdownFile>;

    fn into_iter(self) -> Self::IntoIter { self.files.iter_mut() }
}

impl MarkdownFiles {
    pub const fn new(files: Vec<MarkdownFile>, file_limit: Option<usize>) -> Self {
        Self { files, file_limit }
    }

    pub fn process_files_for_back_populate_matches(
        &mut self,
        config: &ValidatedConfig,
        sorted_wikilinks: &[&Wikilink],
        ac: &AhoCorasick,
    ) {
        // this use of rayon generally makes it go about 100ms faster
        self.par_iter_mut().for_each(|markdown_file| {
            if !cfg!(test)
                && let Some(filter) = config.back_populate_file_filter()
                && !markdown_file.path.ends_with(filter)
            {
                return;
            }

            markdown_file.process_file_for_back_populate_replacements(sorted_wikilinks, config, ac);
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
                    .is_some_and(super::frontmatter::FrontMatter::needs_persist)
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
                    .is_some_and(super::frontmatter::FrontMatter::needs_persist)
            })
            .cloned()
            .collect();

        files_to_persist.sort_by(|a, b| a.path.cmp(&b.path));

        let total_files = files_to_persist.len();
        let count = self.file_limit.unwrap_or(total_files);

        Self {
            files:      files_to_persist.into_iter().take(count).collect(),
            file_limit: self.file_limit,
        }
    }
}

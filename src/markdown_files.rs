use crate::markdown_file_info::{BackPopulateMatch, MarkdownFileInfo, PersistReason};
use crate::utils::{ColumnAlignment, Sha256Cache, ThreadSafeWriter};
use crate::validated_config::ValidatedConfig;
use crate::wikilink::format_wikilink;
use crate::wikilink::Wikilink;
use crate::{CACHE_FILE, CACHE_FOLDER, LEVEL1, LEVEL3};

use crate::obsidian_repository_info::ImageReferences;
use aho_corasick::AhoCorasick;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::io;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct MarkdownFiles {
    files: Vec<MarkdownFileInfo>,
}

impl Deref for MarkdownFiles {
    type Target = Vec<MarkdownFileInfo>;

    fn deref(&self) -> &Self::Target {
        &self.files
    }
}

impl DerefMut for MarkdownFiles {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.files
    }
}

// Add these implementations after the MarkdownFiles struct definition
impl Index<usize> for MarkdownFiles {
    type Output = MarkdownFileInfo;

    fn index(&self, index: usize) -> &Self::Output {
        &self.files[index]
    }
}

impl IndexMut<usize> for MarkdownFiles {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.files[index]
    }
}

impl MarkdownFiles {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    pub fn push(&mut self, file: MarkdownFileInfo) {
        // Note: now takes &mut self
        self.files.push(file);
    }

    pub fn iter(&self) -> impl Iterator<Item = &MarkdownFileInfo> {
        self.files.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut MarkdownFileInfo> {
        self.files.iter_mut()
    }

    pub fn par_iter(&self) -> impl ParallelIterator<Item = &MarkdownFileInfo> {
        self.files.par_iter()
    }

    pub fn process_files(
        &mut self,
        config: &ValidatedConfig,
        sorted_wikilinks: Vec<&Wikilink>,
        ac: &AhoCorasick,
    ) {
        self.par_iter_mut().for_each(|markdown_file_info| {
            if !cfg!(test) {
                if let Some(filter) = config.back_populate_file_filter() {
                    if !markdown_file_info.path.ends_with(filter) {
                        return;
                    }
                }
            }

            markdown_file_info.process_file(&sorted_wikilinks, config, ac);
        });
    }

    pub fn unambiguous_matches(&self) -> Vec<BackPopulateMatch> {
        self.iter()
            .flat_map(|file| file.matches.unambiguous.clone())
            .collect()
    }

    pub fn get_files_to_persist(&self, file_limit: Option<usize>) -> Vec<&MarkdownFileInfo> {
        let files_to_persist: Vec<_> = self
            .files
            .iter()
            .filter(|file_info| {
                file_info
                    .frontmatter
                    .as_ref()
                    .map_or(false, |fm| fm.needs_persist())
            })
            .collect();

        let total_files = files_to_persist.len();
        match file_limit {
            Some(limit) => files_to_persist.into_iter().take(limit).collect(),
            None => files_to_persist.into_iter().take(total_files).collect(),
        }
    }

    pub fn persist_all(
        &self,
        file_limit: Option<usize>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        for file_info in self.get_files_to_persist(file_limit) {
            file_info.persist()?;
        }
        Ok(())
    }

    pub fn update_modified_dates_for_cleanup_images(&mut self, paths: &[PathBuf]) {
        let paths_set: HashSet<_> = paths.iter().collect();

        self.files
            .iter_mut()
            .filter(|file_info| paths_set.contains(&file_info.path))
            .for_each(|file_info| {
                file_info.record_image_references_change();
            });
    }

    pub fn report_frontmatter_issues(
        &self,
        writer: &ThreadSafeWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let files_with_errors: Vec<_> = self
            .files
            .iter()
            .filter_map(|info| info.frontmatter_error.as_ref().map(|err| (&info.path, err)))
            .collect();

        writer.writeln(LEVEL1, "frontmatter")?;

        if files_with_errors.is_empty() {
            return Ok(());
        }

        writer.writeln(
            "",
            &format!(
                "found {} files with frontmatter parsing errors",
                files_with_errors.len()
            ),
        )?;

        for (path, err) in files_with_errors {
            writer.writeln(LEVEL3, &format!("in file {}", format_wikilink(path)))?;
            writer.writeln("", &format!("{}", err))?;
            writer.writeln("", "")?;
        }

        Ok(())
    }

    pub fn write_persist_reasons_table(&self, writer: &ThreadSafeWriter) -> io::Result<()> {
        let mut rows: Vec<Vec<String>> = Vec::new();

        for file in &self.files {
            if !file.persist_reasons.is_empty() {
                let file_name = file
                    .path
                    .file_name()
                    .and_then(|f| f.to_str())
                    .map(|s| s.trim_end_matches(".md"))
                    .unwrap_or_default();

                let wikilink = format!("[[{}]]", file_name);

                // Count instances of BackPopulated and ImageReferencesModified
                let back_populate_count = file.matches.unambiguous.len();

                let image_refs_count = file
                    .persist_reasons
                    .iter()
                    .filter(|&r| matches!(r, PersistReason::ImageReferencesModified))
                    .count();

                // Generate rows for each persist reason
                for reason in &file.persist_reasons {
                    let (before, after, reason_info) = match reason {
                        PersistReason::DateCreatedUpdated { reason } => (
                            file.date_validation_created
                                .frontmatter_date
                                .clone()
                                .unwrap_or_default(),
                            format!(
                                "[[{}]]",
                                file.date_validation_created
                                    .file_system_date
                                    .format("%Y-%m-%d")
                            ),
                            reason.to_string(),
                        ),
                        PersistReason::DateModifiedUpdated { reason } => (
                            file.date_validation_modified
                                .frontmatter_date
                                .clone()
                                .unwrap_or_default(),
                            format!(
                                "[[{}]]",
                                file.date_validation_modified
                                    .file_system_date
                                    .format("%Y-%m-%d")
                            ),
                            reason.to_string(),
                        ),
                        PersistReason::DateCreatedFixApplied => (
                            file.date_created_fix
                                .date_string
                                .clone()
                                .unwrap_or_default(),
                            file.date_created_fix
                                .fix_date
                                .map(|d| format!("[[{}]]", d.format("%Y-%m-%d")))
                                .unwrap_or_default(),
                            String::new(),
                        ),
                        PersistReason::BackPopulated => (
                            String::new(),
                            String::new(),
                            format!("{} instances", back_populate_count),
                        ),
                        PersistReason::ImageReferencesModified => (
                            String::new(),
                            String::new(),
                            format!("{} instances", image_refs_count),
                        ),
                    };

                    rows.push(vec![
                        wikilink.clone(),
                        reason.to_string(),
                        reason_info,
                        before,
                        after,
                    ]);
                }
            }
        }

        if !rows.is_empty() {
            rows.sort_by(|a, b| {
                let file_cmp = a[0].to_lowercase().cmp(&b[0].to_lowercase());
                if file_cmp == std::cmp::Ordering::Equal {
                    a[1].cmp(&b[1])
                } else {
                    file_cmp
                }
            });

            writer.writeln(LEVEL1, "files to be updated")?;
            writer.writeln("", "")?;

            let headers = &["file", "persist reason", "info", "before", "after"];
            let alignments = &[
                ColumnAlignment::Left,
                ColumnAlignment::Left,
                ColumnAlignment::Left,
                ColumnAlignment::Left,
                ColumnAlignment::Left,
            ];

            for (i, chunk) in rows.chunks(500).enumerate() {
                if i > 0 {
                    writer.writeln("", "")?;
                }
                writer.write_markdown_table(headers, chunk, Some(alignments))?;
            }
        }

        Ok(())
    }

    // map of image files to the markdown files that reference them
    pub(crate) fn get_image_info_map(
        &self,
        config: &ValidatedConfig,
        image_files: &[PathBuf],
    ) -> Result<HashMap<PathBuf, ImageReferences>, Box<dyn Error + Send + Sync>> {
        let cache_file_path = config.obsidian_path().join(CACHE_FOLDER).join(CACHE_FILE);

        // Create set of valid paths once
        let valid_paths: HashSet<_> = image_files.iter().map(|p| p.as_path()).collect();

        let cache = Arc::new(Mutex::new({
            let mut cache_instance = Sha256Cache::load_or_create(cache_file_path.clone())?.0;
            cache_instance.mark_deletions(&valid_paths);
            cache_instance
        }));

        // map of markdown_file_info paths to list of image link file names on that markdown file
        let markdown_refs: HashMap<String, HashSet<String>> = self
            .par_iter()
            .filter(|file_info| !file_info.image_links.is_empty())
            .map(|markdown_file_info| {
                let path = markdown_file_info.path.to_string_lossy().to_string();
                let images: HashSet<_> = markdown_file_info
                    .image_links
                    .iter()
                    .map(|link| link.filename.clone())
                    .collect();
                (path, images)
            })
            .collect();

        // Process each image file - for each, find all the markdown_file_info's that have
        // image links that reference that image
        let image_info_map: HashMap<_, _> = image_files
            .par_iter()
            .filter_map(|image_path| {
                let hash = cache.lock().ok()?.get_or_update(image_path).ok()?.0;

                let image_name = image_path.file_name()?.to_str()?;

                let references: Vec<String> = markdown_refs
                    .iter()
                    .filter_map(|(path, image_names)| {
                        if image_names.contains(image_name) {
                            Some(path.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                Some((
                    image_path.clone(),
                    ImageReferences {
                        hash,
                        markdown_file_references: references,
                    },
                ))
            })
            .collect();

        // Final cache operations
        if let Ok(cache) = Arc::try_unwrap(cache).unwrap().into_inner() {
            if cache.has_changes() {
                cache.save()?;
            }
        }

        Ok(image_info_map)
    }
}

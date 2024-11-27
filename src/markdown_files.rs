use crate::markdown_file_info::{MarkdownFileInfo, PersistReason};
use crate::utils::{ColumnAlignment, ThreadSafeWriter};
use crate::wikilink::format_wikilink;
use crate::{LEVEL1, LEVEL3};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use std::collections::HashSet;
use std::error::Error;
use std::io;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct MarkdownFiles {
    files: Vec<MarkdownFileInfo>, // Changed from Arc<Mutex<>>
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

    pub fn persist_all(
        &self,
        file_limit: Option<usize>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
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
        let iter = files_to_persist.iter();
        let files_to_process = match file_limit {
            Some(limit) => iter.take(limit),
            None => iter.take(total_files), // Match the Take type from Some branch
        };

        for file_info in files_to_process {
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
}

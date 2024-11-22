// In obsidian_repository_info.rs

use std::collections::HashSet;
use std::error::Error;
use std::io;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::path::PathBuf;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use crate::{LEVEL1, LEVEL3};
use crate::markdown_file_info::MarkdownFileInfo;
use crate::utils::{ColumnAlignment, ThreadSafeWriter};
use crate::wikilink::format_wikilink;

#[derive(Debug, Default)]
pub struct MarkdownFiles {
    files: Vec<MarkdownFileInfo> // Changed from Arc<Mutex<>>
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
        Self {
            files: Vec::new()
        }
    }

    pub fn push(&mut self, file: MarkdownFileInfo) { // Note: now takes &mut self
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


    pub fn persist_all(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        for file_info in &self.files {
            if let Some(frontmatter) = &file_info.frontmatter {
                if frontmatter.needs_persist() {
                    file_info.persist()?;
                }
            }
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

    pub fn write_date_validation_table(&self, writer: &ThreadSafeWriter) -> io::Result<()> {
        let mut rows: Vec<Vec<String>> = Vec::new();

        for file in &self.files {
            if file.date_validation_created.issue.is_some()
                || file.date_validation_modified.issue.is_some()
                || file.date_created_fix.date_string.is_some()
            {
                let file_name = file
                    .path
                    .file_name()
                    .and_then(|f| f.to_str())
                    .map(|s| s.trim_end_matches(".md"))
                    .unwrap_or_default();

                let wikilink = format!("[[{}]]", file_name);
                let created_status = file.date_validation_created.to_issue_string();
                let modified_status = file.date_validation_modified.to_issue_string();
                let fix_status = file.date_created_fix.to_issue_string();

                let persistence_status = match &file.frontmatter {
                    Some(fm) => {
                        if fm.needs_persist() {
                            "yes".to_string()
                        } else {
                            "no".to_string()
                        }
                    }
                    None => "no frontmatter".to_string(),
                };

                let mut actions = Vec::new();
                if let Some(action) = file.date_validation_created.to_action_string() {
                    actions.push(format!("date_created: {}", action));
                }
                if let Some(action) = file.date_validation_modified.to_action_string() {
                    actions.push(format!("date_modified: {}", action));
                }
                if let Some(action) = file.date_created_fix.to_action_string() {
                    actions.push(format!("date_created_fix: {}", action));
                }

                let action_column = actions.join("<br>");

                rows.push(vec![
                    wikilink,
                    created_status,
                    modified_status,
                    fix_status,
                    persistence_status,
                    action_column,
                ]);
            }
        }

        if !rows.is_empty() {
            rows.sort_by(|a, b| a[0].to_lowercase().cmp(&b[0].to_lowercase()));

            writer.writeln(LEVEL1, "date info from markdown file info")?;
            writer.writeln("", "if date is valid, do nothing")?;
            writer.writeln(
                "",
                "if date is missing, invalid format, or invalid wikilink, pull the date from the file",
            )?;
            writer.writeln("", "")?;

            let headers = &[
                "file",
                "date_created",
                "date_modified",
                "date_created_fix",
                "persist",
                "actions",
            ];

            let alignments = &[
                ColumnAlignment::Left,
                ColumnAlignment::Center,
                ColumnAlignment::Center,
                ColumnAlignment::Center,
                ColumnAlignment::Center,
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

    pub fn report_frontmatter_issues(&self, writer: &ThreadSafeWriter) -> Result<(), Box<dyn Error + Send + Sync>> {
        let files_with_errors: Vec<_> = self.files
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
}

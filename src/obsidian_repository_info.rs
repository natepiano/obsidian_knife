#[cfg(test)]
mod file_process_limit_tests;
#[cfg(test)]
mod persist_file_tests;
#[cfg(test)]
mod update_modified_tests;

use crate::markdown_files::MarkdownFiles;
use crate::scan::ImageInfo;
use crate::utils::{escape_brackets, escape_pipe, ColumnAlignment, ThreadSafeWriter};
use crate::validated_config::ValidatedConfig;
use crate::wikilink_types::{InvalidWikilinkReason, ToWikilink, Wikilink};
use crate::LEVEL2;
use aho_corasick::AhoCorasick;
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
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
    pub fn persist(
        &mut self,
        config: &ValidatedConfig,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.markdown_files.persist_all(config.file_process_limit())
    }

    pub fn update_modified_dates_for_cleanup_images(&mut self, paths: &[PathBuf]) {
        self.markdown_files
            .update_modified_dates_for_cleanup_images(paths);
    }

    pub fn write_invalid_wikilinks_table(
        &self,
        writer: &ThreadSafeWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Collect all invalid wikilinks from all files
        let invalid_wikilinks = self
            .markdown_files
            .iter()
            .flat_map(|markdown_file_info| {
                markdown_file_info
                    .invalid_wikilinks
                    .iter()
                    .filter(|wikilink| {
                        !matches!(
                            wikilink.reason,
                            InvalidWikilinkReason::EmailAddress | InvalidWikilinkReason::Tag
                        )
                    })
                    .map(move |wikilink| (&markdown_file_info.path, wikilink))
            })
            .collect::<Vec<_>>()
            .into_iter()
            .sorted_by(|a, b| {
                let file_a = a.0.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let file_b = b.0.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                file_a
                    .cmp(file_b)
                    .then(a.1.line_number.cmp(&b.1.line_number))
            })
            .collect::<Vec<_>>();

        if invalid_wikilinks.is_empty() {
            return Ok(());
        }

        writer.writeln(LEVEL2, "invalid wikilinks")?;

        // Write header describing the count
        writer.writeln(
            "",
            &format!(
                "found {} invalid wikilinks in {} files\n",
                invalid_wikilinks.len(),
                invalid_wikilinks
                    .iter()
                    .map(|(p, _)| p)
                    .collect::<HashSet<_>>()
                    .len()
            ),
        )?;

        // Prepare headers and alignments for the table
        let headers = vec![
            "file name",
            "line",
            "line text",
            "invalid reason",
            "source text",
        ];

        let alignments = vec![
            ColumnAlignment::Left,
            ColumnAlignment::Right,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ];

        // Prepare rows
        let rows: Vec<Vec<String>> = invalid_wikilinks
            .iter()
            .map(|(file_path, invalid_wikilink)| {
                vec![
                    file_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_wikilink(),
                    invalid_wikilink.line_number.to_string(),
                    escape_pipe(&invalid_wikilink.line),
                    invalid_wikilink.reason.to_string(),
                    escape_brackets(&invalid_wikilink.content),
                ]
            })
            .collect();

        // Write the table
        writer.write_markdown_table(&headers, &rows, Some(&alignments))?;
        writer.writeln("", "\n---\n")?;

        Ok(())
    }
}

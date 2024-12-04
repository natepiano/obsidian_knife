use crate::constants::*;
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::report::{ReportTable, TableBuilder};
use crate::utils::escape_brackets;
use crate::utils::escape_pipe;
use crate::utils::{ColumnAlignment, ReportWriter};
use crate::wikilink::{InvalidWikilink, InvalidWikilinkReason, ToWikilink};
use itertools::Itertools;
use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;

pub struct InvalidWikilinksTable;

impl TableBuilder for InvalidWikilinksTable {
    type Item = (PathBuf, InvalidWikilink);

    fn headers(&self) -> Vec<&str> {
        vec![
            "file name",
            "line",
            "line text",
            "invalid reason",
            "source text",
        ]
    }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![
            ColumnAlignment::Left,
            ColumnAlignment::Right,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]
    }

    fn build_rows(&self, items: &[Self::Item]) -> Vec<Vec<String>> {
        items
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
            .collect()
    }

    fn title(&self) -> Option<&str> {
        Some(INVALID_WIKILINKS)
    }

    fn description(&self, items: &[Self::Item]) -> Option<String> {
        let unique_files = items
            .iter()
            .map(|(p, _)| p)
            .collect::<std::collections::HashSet<_>>()
            .len();

        Some(format!(
            "found {} invalid wikilinks in {} files\n",
            items.len(),
            unique_files
        ))
    }
}

impl ObsidianRepositoryInfo {
    pub fn write_invalid_wikilinks_table(
        &self,
        writer: &ReportWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let invalid_wikilinks: Vec<(PathBuf, InvalidWikilink)> = self
            .markdown_files
            .iter()
            .flat_map(|markdown_file_info| {
                markdown_file_info
                    .invalid_wikilinks
                    .iter()
                    .filter(|wikilink| {
                        !matches!(
                            wikilink.reason,
                            InvalidWikilinkReason::EmailAddress
                                | InvalidWikilinkReason::Tag
                                | InvalidWikilinkReason::RawHttpLink
                        )
                    })
                    .map(move |wikilink| (markdown_file_info.path.clone(), (*wikilink).clone()))
            })
            .collect::<Vec<_>>()
            .into_iter()
            .sorted_by(|a, b| {
                let file_a = a.0.file_stem().and_then(OsStr::to_str).unwrap_or_default();
                let file_b = b.0.file_stem().and_then(OsStr::to_str).unwrap_or_default();
                file_a
                    .cmp(file_b)
                    .then(a.1.line_number.cmp(&b.1.line_number))
            })
            .collect();

        if invalid_wikilinks.is_empty() {
            return Ok(());
        }

        let table = ReportTable::new(invalid_wikilinks, LEVEL2);
        table.write(&InvalidWikilinksTable, writer)?;

        writer.writeln("", "---\n")?;

        Ok(())
    }
}

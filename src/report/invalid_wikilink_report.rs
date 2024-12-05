use crate::constants::*;
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::report::{DescriptionBuilder, ReportDefinition, ReportWriter};
use crate::utils::escape_brackets;
use crate::utils::escape_pipe;
use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use crate::wikilink::{InvalidWikilink, InvalidWikilinkReason, ToWikilink};
use itertools::Itertools;
use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;

pub struct InvalidWikilinksTable;

impl ReportDefinition for InvalidWikilinksTable {
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

    fn build_rows(&self, items: &[Self::Item], _: Option<&ValidatedConfig>) -> Vec<Vec<String>> {
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

    fn title(&self) -> Option<String> {
        Some(INVALID_WIKILINKS.to_string())
    }

    fn description(&self, items: &[Self::Item]) -> String {
        let unique_files = items
            .iter()
            .map(|(p, _)| p)
            .collect::<std::collections::HashSet<_>>()
            .len();

        DescriptionBuilder::new()
            .text(FOUND)
            .number(items.len())
            .text(INVALID)
            .pluralize(Phrase::Wikilink(items.len()))
            .text(IN)
            .pluralize_with_count(Phrase::File(unique_files))
            .text_with_newline("")
            .no_space(YOU_HAVE_TO_FIX_THESE_YOURSELF)
            .build()
    }

    fn level(&self) -> &'static str {
        LEVEL2
    }
}

impl ObsidianRepositoryInfo {
    pub fn write_invalid_wikilinks_report(
        &self,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let report = ReportWriter::new(self.collect_invalid_wikilinks());
        report.write(&InvalidWikilinksTable, writer)
    }

    fn collect_invalid_wikilinks(&self) -> Vec<(PathBuf, InvalidWikilink)> {
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
        invalid_wikilinks
    }
}

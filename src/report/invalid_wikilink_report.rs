use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;

use itertools::Itertools;

use super::constants::TABLE_HEADER_FILE_NAME;
use super::constants::TABLE_HEADER_INVALID_REASON;
use super::constants::TABLE_HEADER_LINE;
use super::constants::TABLE_HEADER_LINE_TEXT;
use super::constants::TABLE_HEADER_SOURCE_TEXT;
use super::report_writer::ReportDefinition;
use super::report_writer::ReportWriter;
use crate::constants::FOUND;
use crate::constants::IN;
use crate::constants::INVALID;
use crate::constants::INVALID_WIKILINKS;
use crate::constants::LEVEL2;
use crate::constants::YOU_HAVE_TO_FIX_THESE_YOURSELF;
use crate::description_builder::DescriptionBuilder;
use crate::description_builder::Phrase;
use crate::obsidian_repository::ObsidianRepository;
use crate::utils;
use crate::utils::ColumnAlignment;
use crate::utils::OutputFileWriter;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::InvalidWikilink;
use crate::wikilink::InvalidWikilinkReason;
use crate::wikilink::ToWikilink;

pub(super) struct InvalidWikilinksTable;

impl ReportDefinition for InvalidWikilinksTable {
    type Item = (PathBuf, InvalidWikilink);

    fn headers(&self) -> Vec<&str> {
        vec![
            TABLE_HEADER_FILE_NAME,
            TABLE_HEADER_LINE,
            TABLE_HEADER_LINE_TEXT,
            TABLE_HEADER_INVALID_REASON,
            TABLE_HEADER_SOURCE_TEXT,
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

    fn build_rows(
        &self,
        items: &[Self::Item],
        _: Option<&ValidatedConfig>,
    ) -> anyhow::Result<Vec<Vec<String>>> {
        Ok(items
            .iter()
            .map(|(file_path, invalid_wikilink)| {
                vec![
                    file_path
                        .file_stem()
                        .and_then(OsStr::to_str)
                        .unwrap_or("")
                        .to_wikilink(),
                    invalid_wikilink.line_number.to_string(),
                    utils::escape_pipe(&invalid_wikilink.line),
                    invalid_wikilink.reason.to_string(),
                    utils::escape_brackets(&invalid_wikilink.content),
                ]
            })
            .collect())
    }

    fn title(&self) -> Option<String> { Some(INVALID_WIKILINKS.to_string()) }

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

    fn level(&self) -> &'static str { LEVEL2 }
}

impl ObsidianRepository {
    pub(super) fn write_invalid_wikilinks_report(
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
            .flat_map(|markdown_file| {
                markdown_file
                    .wikilinks
                    .invalid
                    .iter()
                    .filter(|wikilink| {
                        !matches!(
                            wikilink.reason,
                            InvalidWikilinkReason::EmailAddress
                                | InvalidWikilinkReason::Tag
                                | InvalidWikilinkReason::RawHttpLink
                        )
                    })
                    .map(move |wikilink| (markdown_file.path.clone(), (*wikilink).clone()))
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

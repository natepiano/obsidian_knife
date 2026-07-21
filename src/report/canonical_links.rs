use std::cmp::Ordering;
use std::collections::HashSet;
use std::error::Error;
use std::ffi::OsStr;
use std::path::Path;

use anyhow::Result as AnyhowResult;

use super::constants::FILE_COLUMN_INDEX;
use super::constants::LINE_NUMBER_COLUMN_INDEX;
use super::constants::TABLE_HEADER_FILE_NAME;
use super::constants::TABLE_HEADER_LINE;
use super::constants::UNPARSABLE_LINE_NUMBER_SORT_KEY;
use super::support;
use super::writer::ReportDefinition;
use super::writer::ReportWriter;
use crate::constants::ESCAPED_PIPE;
use crate::constants::FOUND;
use crate::constants::IN;
use crate::constants::LEVEL2;
use crate::constants::NON_CANONICAL_LINK;
use crate::constants::NON_CANONICAL_LINKS;
use crate::constants::NON_CANONICAL_LINKS_DESCRIPTION;
use crate::constants::SOURCE_TEXT;
use crate::constants::WILL_REPLACE_WITH;
use crate::description_builder::DescriptionBuilder;
use crate::markdown_file::CanonicalLinkMatch;
use crate::obsidian_repository::ObsidianRepository;
use crate::output_file_writer::ColumnAlignment;
use crate::output_file_writer::OutputFileWriter;
use crate::phrase::Phrase;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::ToWikilink;

struct CanonicalLinksTable;

impl ReportDefinition for CanonicalLinksTable {
    type Item = CanonicalLinkMatch;

    fn headers(&self) -> Vec<&str> {
        vec![
            TABLE_HEADER_FILE_NAME,
            TABLE_HEADER_LINE,
            NON_CANONICAL_LINK,
            WILL_REPLACE_WITH,
            SOURCE_TEXT,
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
    ) -> AnyhowResult<Vec<Vec<String>>> {
        let mut rows: Vec<Vec<String>> = items
            .iter()
            .map(|canonical_link_match| {
                let file_stem = Path::new(&canonical_link_match.relative_path)
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .unwrap_or_default();

                // Table-sourced replacements already carry `ESCAPED_PIPE`.
                let replacement = if canonical_link_match.replacement.contains(ESCAPED_PIPE) {
                    canonical_link_match.replacement.clone()
                } else {
                    support::escape_pipe(&canonical_link_match.replacement)
                };

                vec![
                    file_stem.to_wikilink(),
                    canonical_link_match.line_number.to_string(),
                    support::escape_pipe(&support::escape_brackets(
                        &canonical_link_match.found_text,
                    )),
                    replacement.clone(),
                    support::escape_brackets(&replacement),
                ]
            })
            .collect();

        rows.sort_by(|a, b| {
            let file_cmp = a[FILE_COLUMN_INDEX]
                .to_lowercase()
                .cmp(&b[FILE_COLUMN_INDEX].to_lowercase());
            if file_cmp == Ordering::Equal {
                a[LINE_NUMBER_COLUMN_INDEX]
                    .parse::<usize>()
                    .unwrap_or(UNPARSABLE_LINE_NUMBER_SORT_KEY)
                    .cmp(
                        &b[LINE_NUMBER_COLUMN_INDEX]
                            .parse::<usize>()
                            .unwrap_or(UNPARSABLE_LINE_NUMBER_SORT_KEY),
                    )
            } else {
                file_cmp
            }
        });

        Ok(rows)
    }

    fn title(&self) -> Option<String> { Some(NON_CANONICAL_LINKS.to_string()) }

    fn description(&self, items: &[Self::Item]) -> String {
        let unique_files: HashSet<&String> = items.iter().map(|m| &m.relative_path).collect();

        DescriptionBuilder::new()
            .text(FOUND)
            .pluralize_with_count(Phrase::Wikilink(items.len()))
            .text(IN)
            .pluralize_with_count(Phrase::File(unique_files.len()))
            .text_with_newline("")
            .no_space(NON_CANONICAL_LINKS_DESCRIPTION)
            .build()
    }

    fn level(&self) -> &'static str { LEVEL2 }
}

impl ObsidianRepository {
    pub(super) fn write_canonical_links_report(
        &self,
        output_file_writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let matches: Vec<CanonicalLinkMatch> = self
            .markdown_files
            .files_to_persist()
            .iter()
            .flat_map(|file| file.canonical_link_matches.clone())
            .collect();

        let report_writer = ReportWriter::new(matches);
        report_writer.write(&CanonicalLinksTable, output_file_writer)
    }
}

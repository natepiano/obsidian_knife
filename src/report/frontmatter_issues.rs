use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;

use anyhow::Result as AnyhowResult;

use super::constants::TABLE_HEADER_ERROR_MESSAGE;
use super::constants::TABLE_HEADER_FILE_NAME;
use super::writer::ReportDefinition;
use super::writer::ReportWriter;
use crate::constants::FOUND;
use crate::constants::FRONTMATTER;
use crate::constants::DATE_CREATED_MISSING;
use crate::constants::FRONTMATTER_ISSUES;
use crate::constants::LEVEL1;
use crate::constants::YOU_HAVE_TO_FIX_THESE_YOURSELF;
use crate::description_builder::DescriptionBuilder;
use crate::markdown_file::MarkdownFile;
use crate::obsidian_repository::ObsidianRepository;
use crate::output_file_writer::ColumnAlignment;
use crate::output_file_writer::OutputFileWriter;
use crate::phrase::Phrase;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::ToWikilink;

pub(super) struct FrontmatterIssuesTable;

impl ReportDefinition for FrontmatterIssuesTable {
    type Item = (PathBuf, String); // (file_path, error_message)

    fn headers(&self) -> Vec<&str> { vec![TABLE_HEADER_FILE_NAME, TABLE_HEADER_ERROR_MESSAGE] }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![ColumnAlignment::Left, ColumnAlignment::Left]
    }

    fn build_rows(
        &self,
        items: &[Self::Item],
        _: Option<&ValidatedConfig>,
    ) -> AnyhowResult<Vec<Vec<String>>> {
        Ok(items
            .iter()
            .map(|(file_path, error_message)| {
                vec![
                    file_path
                        .file_stem()
                        .and_then(OsStr::to_str)
                        .unwrap_or("")
                        .to_wikilink(),
                    error_message.clone(),
                ]
            })
            .collect())
    }

    fn title(&self) -> Option<String> { Some(FRONTMATTER_ISSUES.to_string()) }

    fn description(&self, items: &[Self::Item]) -> String {
        DescriptionBuilder::new()
            .text(FOUND)
            .pluralize_with_count(Phrase::File(items.len()))
            .pluralize(Phrase::With(items.len()))
            .text(FRONTMATTER)
            .pluralize(Phrase::Issue(items.len()))
            .text_with_newline("")
            .no_space(YOU_HAVE_TO_FIX_THESE_YOURSELF)
            .build()
    }

    fn level(&self) -> &'static str { LEVEL1 }
}

impl ObsidianRepository {
    pub(super) fn write_frontmatter_issues_report(
        &self,
        output_file_writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let report_writer = ReportWriter::new(self.collect_frontmatter_issues());
        report_writer.write(&FrontmatterIssuesTable, output_file_writer)
    }

    fn collect_frontmatter_issues(&self) -> Vec<(PathBuf, String)> {
        self.markdown_files
            .iter()
            .filter_map(|markdown_file| {
                Self::frontmatter_issue(markdown_file)
                    .map(|issue| (markdown_file.path.clone(), issue))
            })
            .collect()
    }

    /// The vault's linter owns `date_created`: it stamps the property when a note is made,
    /// and this tool never writes one, so a note without it is reported for the user to fix
    /// rather than dated from a filesystem timestamp that any copy or checkout would reset.
    fn frontmatter_issue(markdown_file: &MarkdownFile) -> Option<String> {
        if let Some(error) = markdown_file.frontmatter_error.as_ref() {
            return Some(error.to_string());
        }

        markdown_file
            .front_matter
            .as_ref()
            .filter(|front_matter| front_matter.date_created().is_none())
            .map(|_| DATE_CREATED_MISSING.to_string())
    }
}

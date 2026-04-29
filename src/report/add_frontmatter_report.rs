use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;

use super::report_writer::ReportDefinition;
use super::report_writer::ReportWriter;
use crate::constants::ADD_FRONTMATTER;
use crate::constants::FILE;
use crate::constants::FRONTMATTER;
use crate::constants::LEVEL2;
use crate::description_builder::DescriptionBuilder;
use crate::description_builder::Phrase;
use crate::markdown_file::PersistReason;
use crate::obsidian_repository::ObsidianRepository;
use crate::utils::ColumnAlignment;
use crate::utils::OutputFileWriter;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::ToWikilink;

pub(super) struct AddFrontmatterTable;

impl ReportDefinition for AddFrontmatterTable {
    type Item = PathBuf;

    fn headers(&self) -> Vec<&str> { vec![FILE] }

    fn alignments(&self) -> Vec<ColumnAlignment> { vec![ColumnAlignment::Left] }

    fn build_rows(
        &self,
        items: &[Self::Item],
        _: Option<&ValidatedConfig>,
    ) -> anyhow::Result<Vec<Vec<String>>> {
        Ok(items
            .iter()
            .map(|file_path| {
                vec![
                    file_path
                        .file_stem()
                        .and_then(OsStr::to_str)
                        .unwrap_or("")
                        .to_wikilink(),
                ]
            })
            .collect())
    }

    fn title(&self) -> Option<String> { Some(ADD_FRONTMATTER.to_string()) }

    fn description(&self, items: &[Self::Item]) -> String {
        DescriptionBuilder::new()
            .text("created")
            .text(FRONTMATTER)
            .text("for")
            .pluralize_with_count(Phrase::File(items.len()))
            .build()
    }

    fn level(&self) -> &'static str { LEVEL2 }
}

impl ObsidianRepository {
    pub(super) fn write_add_frontmatter_report(
        &self,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let report = ReportWriter::new(self.collect_add_frontmatter_files());
        report.write(&AddFrontmatterTable, writer)
    }

    fn collect_add_frontmatter_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self
            .markdown_files
            .iter()
            .filter(|file| {
                file.persist_reasons
                    .iter()
                    .any(|r| matches!(r, PersistReason::FrontmatterCreated))
            })
            .map(|file| file.path.clone())
            .collect();

        files.sort();
        files
    }
}

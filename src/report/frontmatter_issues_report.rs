use crate::constants::*;
use crate::obsidian_repository::ObsidianRepository;
use crate::report::{DescriptionBuilder, ReportDefinition, ReportWriter};
use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use crate::wikilink::ToWikilink;
use std::error::Error;
use std::path::PathBuf;

pub struct FrontmatterIssuesTable;

impl ReportDefinition for FrontmatterIssuesTable {
    type Item = (PathBuf, String); // (file_path, error_message)

    fn headers(&self) -> Vec<&str> {
        vec!["file name", "error message"]
    }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![ColumnAlignment::Left, ColumnAlignment::Left]
    }

    fn build_rows(&self, items: &[Self::Item], _: Option<&ValidatedConfig>) -> Vec<Vec<String>> {
        items
            .iter()
            .map(|(file_path, error_message)| {
                vec![
                    file_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_wikilink(),
                    error_message.clone(),
                ]
            })
            .collect()
    }

    fn title(&self) -> Option<String> {
        Some(FRONTMATTER_ISSUES.to_string())
    }

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

    fn level(&self) -> &'static str {
        LEVEL1
    }
}

impl ObsidianRepository {
    pub fn write_frontmatter_issues_report(
        &self,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let report = ReportWriter::new(self.collect_frontmatter_issues());
        report.write(&FrontmatterIssuesTable, writer)
    }

    fn collect_frontmatter_issues(&self) -> Vec<(PathBuf, String)> {
        self.markdown_files
            .iter()
            .filter_map(|info| {
                info.frontmatter_error
                    .as_ref()
                    .map(|err| (info.path.clone(), err.to_string()))
            })
            .collect()
    }
}

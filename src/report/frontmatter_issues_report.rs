use crate::constants::*;
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::report::{ReportWriter, TableDefinition};
use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::wikilink::ToWikilink;
use std::error::Error;
use std::path::PathBuf;

pub struct FrontmatterIssuesTable;

impl TableDefinition for FrontmatterIssuesTable {
    type Item = (PathBuf, String);  // (file_path, error_message)

    fn headers(&self) -> Vec<&str> {
        vec!["file name", "error message"]
    }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![ColumnAlignment::Left, ColumnAlignment::Left]
    }

    fn build_rows(&self, items: &[Self::Item]) -> Vec<Vec<String>> {
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

    fn title(&self) -> Option<&str> {
        Some(FRONTMATTER_ISSUES)
    }

    fn description(&self, items: &[Self::Item]) -> Option<String> {
        Some(format!(
            "found {} files with frontmatter parsing errors\n",
            items.len()
        ))
    }

    fn level(&self) -> &'static str {
        LEVEL1
    }
}

impl ObsidianRepositoryInfo {
    pub fn write_frontmatter_issues_report(
        &self,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let report = ReportWriter::new(self.collect_frontmatter_issues());
        report.write(&FrontmatterIssuesTable, writer)
    }

    fn collect_frontmatter_issues(&self) -> Vec<(PathBuf, String)> {
        self.markdown_files
            .files
            .iter()
            .filter_map(|info| {
                info.frontmatter_error
                    .as_ref()
                    .map(|err| (info.path.clone(), err.to_string()))
            })
            .collect()
    }
}

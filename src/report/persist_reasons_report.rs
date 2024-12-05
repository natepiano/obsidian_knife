use crate::constants::*;
use crate::markdown_file_info::PersistReason;
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::report::{ReportDefinition, ReportWriter};
use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use std::error::Error;

pub struct PersistReasonsTable;

#[derive(Clone)]
pub struct PersistReasonData {
    wikilink: String,
    reason: PersistReason,
    back_populate_count: usize,
    image_refs_count: usize,
    date_validation_created: Option<(String, String)>, // (before, after)
    date_validation_modified: Option<(String, String)>,
    date_created_fix: Option<(String, String)>,
}

impl ReportDefinition for PersistReasonsTable {
    type Item = PersistReasonData;

    fn headers(&self) -> Vec<&str> {
        vec!["file", "persist reason", "info", "before", "after"]
    }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]
    }

    fn build_rows(&self, items: &[Self::Item], _: Option<&ValidatedConfig>) -> Vec<Vec<String>> {
        items
            .iter()
            .map(|item| {
                let (before, after, reason_info) = match &item.reason {
                    PersistReason::DateCreatedUpdated { reason } => {
                        let (before, after) =
                            item.date_validation_created.clone().unwrap_or_default();
                        (before, after, reason.to_string())
                    }
                    PersistReason::DateModifiedUpdated { reason } => {
                        let (before, after) =
                            item.date_validation_modified.clone().unwrap_or_default();
                        (before, after, reason.to_string())
                    }
                    PersistReason::DateCreatedFixApplied => {
                        let (before, after) = item.date_created_fix.clone().unwrap_or_default();
                        (before, after, String::new())
                    }
                    PersistReason::BackPopulated => (
                        String::new(),
                        String::new(),
                        format!("{} instances", item.back_populate_count),
                    ),
                    PersistReason::ImageReferencesModified => (
                        String::new(),
                        String::new(),
                        format!("{} instances", item.image_refs_count),
                    ),
                };

                vec![
                    item.wikilink.clone(),
                    item.reason.to_string(),
                    reason_info,
                    before,
                    after,
                ]
            })
            .collect()
    }

    fn title(&self) -> Option<String> {
        None
    }

    fn description(&self, _items: &[Self::Item]) -> String {
        String::new()
    }

    fn level(&self) -> &'static str {
        LEVEL1
    }
}

impl ObsidianRepositoryInfo {
    pub fn write_persist_reasons_report(
        &self,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut persist_data = Vec::new();

        for file in &self.markdown_files.files {
            if !file.persist_reasons.is_empty() {
                let file_name = file
                    .path
                    .file_name()
                    .and_then(|f| f.to_str())
                    .map(|s| s.trim_end_matches(".md"))
                    .unwrap_or_default();

                let wikilink = format!("[[{}]]", file_name);
                let back_populate_count = file.matches.unambiguous.len();
                let image_refs_count = file
                    .persist_reasons
                    .iter()
                    .filter(|&r| matches!(r, PersistReason::ImageReferencesModified))
                    .count();

                for reason in &file.persist_reasons {
                    let data = PersistReasonData {
                        wikilink: wikilink.clone(),
                        reason: reason.clone(),
                        back_populate_count,
                        image_refs_count,
                        date_validation_created: file
                            .date_validation_created
                            .frontmatter_date
                            .clone()
                            .map(|d| {
                                (
                                    d,
                                    format!(
                                        "[[{}]]",
                                        file.date_validation_created
                                            .file_system_date
                                            .format("%Y-%m-%d")
                                    ),
                                )
                            }),
                        date_validation_modified: file
                            .date_validation_modified
                            .frontmatter_date
                            .clone()
                            .map(|d| {
                                (
                                    d,
                                    format!(
                                        "[[{}]]",
                                        file.date_validation_modified
                                            .file_system_date
                                            .format("%Y-%m-%d")
                                    ),
                                )
                            }),
                        date_created_fix: file.date_created_fix.date_string.clone().zip(
                            file.date_created_fix
                                .fix_date
                                .map(|d| format!("[[{}]]", d.format("%Y-%m-%d"))),
                        ),
                    };
                    persist_data.push(data);
                }
            }
        }

        if !persist_data.is_empty() {
            persist_data.sort_by(|a, b| {
                let file_cmp = a.wikilink.to_lowercase().cmp(&b.wikilink.to_lowercase());
                if file_cmp == std::cmp::Ordering::Equal {
                    a.reason.to_string().cmp(&b.reason.to_string())
                } else {
                    file_cmp
                }
            });

            writer.writeln(LEVEL1, "files to be updated")?;
            writer.writeln("", "")?;

            // Process rows in chunks of 500
            for (i, chunk) in persist_data.chunks(500).enumerate() {
                let table = PersistReasonsTable;
                let report = ReportWriter::new(chunk.to_vec());

                if i == 0 {
                    report.write(&table, writer)?;
                } else {
                    writer.writeln("", "")?;
                    let rows = table.build_rows(chunk, None);
                    writer.write_markdown_table(
                        &table.headers(),
                        &rows,
                        Some(&table.alignments()),
                    )?;
                }
            }
        }

        Ok(())
    }
}

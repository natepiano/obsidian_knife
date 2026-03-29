use std::error::Error;
use std::path::PathBuf;

use super::report_writer::ReportDefinition;
use super::report_writer::ReportWriter;
use crate::constants::AFTER;
use crate::constants::BEFORE;
use crate::constants::DescriptionBuilder;
use crate::constants::FILE;
use crate::constants::INFO;
use crate::constants::LEVEL1;
use crate::constants::LEVEL2;
use crate::constants::PATH;
use crate::constants::Phrase;
use crate::constants::REASON;
use crate::constants::UPDATE;
use crate::markdown_file::DateValidation;
use crate::markdown_file::MarkdownFile;
use crate::markdown_file::PersistReason;
use crate::obsidian_repository::ObsidianRepository;
use crate::utils;
use crate::utils::ColumnAlignment;
use crate::utils::OutputFileWriter;
use crate::validated_config::ValidatedConfig;

pub(super) struct PersistReasonsTable;

#[derive(Clone)]
pub(super) struct PersistReasonData {
    back_populate_count:      usize,
    date_created_fix:         Option<(String, String)>,
    date_validation_created:  Option<(String, String)>, // (before, after)
    date_validation_modified: Option<(String, String)>,
    full_path:                PathBuf, //for sorting
    image_refs_count:         usize,
    parent_path:              String,
    reason:                   PersistReason,
    wikilink:                 String,
}

impl ReportDefinition for PersistReasonsTable {
    type Item = PersistReasonData;

    fn headers(&self) -> Vec<&str> { vec![FILE, PATH, REASON, INFO, BEFORE, AFTER] }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![
            ColumnAlignment::Left,
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
                    },
                    PersistReason::DateModifiedUpdated { reason } => {
                        let (before, after) =
                            item.date_validation_modified.clone().unwrap_or_default();
                        (before, after, reason.to_string())
                    },
                    PersistReason::DateCreatedFixApplied => {
                        let (before, after) = item.date_created_fix.clone().unwrap_or_default();
                        (before, after, String::new())
                    },
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
                    PersistReason::FrontmatterCreated => {
                        (String::new(), String::new(), String::new())
                    },
                };

                vec![
                    item.wikilink.clone(),
                    item.parent_path.clone(),
                    item.reason.to_string(),
                    reason_info,
                    before,
                    after,
                ]
            })
            .collect()
    }

    fn title(&self) -> Option<String> { None }

    fn description(&self, items: &[Self::Item]) -> String {
        DescriptionBuilder::new()
            .number(items.len())
            .text(UPDATE)
            .pluralize(Phrase::Reason(items.len()))
            .build()
    }

    fn level(&self) -> &'static str { LEVEL2 }
}

impl ObsidianRepository {
    pub(super) fn write_persist_reasons_report(
        &self,
        config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut persist_data: Vec<PersistReasonData> = self
            .markdown_files
            .files_to_persist()
            .iter()
            .filter(|file| !file.persist_reasons.is_empty())
            .flat_map(|file| Self::build_persist_data_for_file(file, config))
            .collect();

        if persist_data.is_empty() {
            return Ok(());
        }

        persist_data.sort_by(|a, b| {
            let file_cmp = a
                .full_path
                .to_string_lossy()
                .to_lowercase()
                .cmp(&b.full_path.to_string_lossy().to_lowercase());
            if file_cmp == std::cmp::Ordering::Equal {
                a.reason.to_string().cmp(&b.reason.to_string())
            } else {
                file_cmp
            }
        });

        writer.writeln(LEVEL1, "files to be updated")?;
        writer.writeln("", "")?;

        for chunk in persist_data.chunks(500) {
            let table = PersistReasonsTable;
            let report = ReportWriter::new(chunk.to_vec());
            report.write(&table, writer)?;
        }

        Ok(())
    }

    fn build_persist_data_for_file(
        file: &MarkdownFile,
        config: &ValidatedConfig,
    ) -> Vec<PersistReasonData> {
        let relative_path = file
            .path
            .strip_prefix(config.obsidian_path())
            .unwrap_or(&file.path)
            .to_string_lossy()
            .trim_end_matches(".md")
            .to_string();

        let file_name = file
            .path
            .file_stem()
            .and_then(|f| f.to_str())
            .unwrap_or_default();

        let parent_path = file
            .path
            .strip_prefix(config.obsidian_path())
            .unwrap_or(&file.path)
            .parent()
            .map_or_else(|| "/".to_string(), |p| p.to_string_lossy().to_string());

        let wikilink = if relative_path == file_name {
            format!("[[{file_name}]]")
        } else {
            format!("[[{relative_path}|{file_name}]]")
        };

        let back_populate_count = file.matches.unambiguous.len();
        let image_refs_count = file
            .persist_reasons
            .iter()
            .filter(|&r| matches!(r, PersistReason::ImageReferencesModified))
            .count();

        let date_validation_created =
            Some(Self::format_date_validation(&file.date_validation_created));
        let date_validation_modified =
            Some(Self::format_date_validation(&file.date_validation_modified));
        let date_created_fix = Some((
            format!(
                "[[{}]]",
                file.date_validation_created
                    .operational_file_system_date()
                    .format("%Y-%m-%d")
            ),
            file.date_created_fix
                .fix_date
                .map(|d| format!("[[{}]]", d.format("%Y-%m-%d")))
                .unwrap_or_default(),
        ));

        file.persist_reasons
            .iter()
            .map(|reason| PersistReasonData {
                full_path: file.path.clone(),
                wikilink: utils::escape_pipe(&wikilink),
                reason: reason.clone(),
                back_populate_count,
                image_refs_count,
                parent_path: parent_path.clone(),
                date_validation_created: date_validation_created.clone(),
                date_validation_modified: date_validation_modified.clone(),
                date_created_fix: date_created_fix.clone(),
            })
            .collect()
    }

    fn format_date_validation(validation: &DateValidation) -> (String, String) {
        (
            validation.frontmatter_date.clone().unwrap_or_default(),
            format!(
                "[[{}]]",
                validation.operational_file_system_date().format("%Y-%m-%d")
            ),
        )
    }
}

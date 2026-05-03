use std::error::Error;
use std::path::PathBuf;

use super::orchestration;
use super::report_writer::ReportDefinition;
use super::report_writer::ReportWriter;
use crate::constants::ACTION;
use crate::constants::FILE;
use crate::constants::LEVEL2;
use crate::constants::LINE;
use crate::constants::MISSING_IMAGE;
use crate::constants::MISSING_IMAGE_REFERENCES;
use crate::constants::POSITION;
use crate::constants::REFERENCE_REMOVED;
use crate::constants::REFERENCE_WILL_BE_REMOVED;
use crate::description_builder::DescriptionBuilder;
use crate::description_builder::Phrase;
use crate::markdown_file::ImageLinkState;
use crate::obsidian_repository::ObsidianRepository;
use crate::output_file_writer::ColumnAlignment;
use crate::output_file_writer::OutputFileWriter;
use crate::support;
use crate::validated_config::ChangeMode;
use crate::validated_config::ValidatedConfig;
use crate::vec_enum_filter::VecEnumFilter;

pub(super) struct MissingReferencesTable;

impl ReportDefinition for MissingReferencesTable {
    type Item = (PathBuf, String, usize, usize); // (markdown_path, extracted_filename, line, position)

    fn headers(&self) -> Vec<&str> { vec![FILE, LINE, POSITION, MISSING_IMAGE_REFERENCES, ACTION] }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![
            ColumnAlignment::Left,
            ColumnAlignment::Right,
            ColumnAlignment::Right,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]
    }

    fn build_rows(
        &self,
        items: &[Self::Item],
        config: Option<&ValidatedConfig>,
    ) -> anyhow::Result<Vec<Vec<String>>> {
        let validated_config = config.ok_or_else(|| {
            anyhow::anyhow!("ValidatedConfig required for missing-references report")
        })?;

        let mut rows: Vec<Vec<String>> = items
            .iter()
            .map(
                |(markdown_path, extracted_filename, line_number, position)| {
                    let markdown_link = orchestration::format_wikilink(
                        markdown_path,
                        validated_config.obsidian_path(),
                    );

                    let image_link = support::escape_pipe(&support::escape_brackets(
                        &extracted_filename.clone(),
                    ));

                    let action = match validated_config.change_mode() {
                        ChangeMode::Apply => REFERENCE_REMOVED,
                        ChangeMode::DryRun => REFERENCE_WILL_BE_REMOVED,
                    };

                    vec![
                        markdown_link,
                        line_number.to_string(),
                        position.to_string(),
                        image_link,
                        action.to_string(),
                    ]
                },
            )
            .collect();

        // Sort rows by markdown link (first column)
        rows.sort_by(|a, b| a[0].cmp(&b[0]));
        Ok(rows)
    }

    fn title(&self) -> Option<String> { Some(MISSING_IMAGE_REFERENCES.to_string()) }

    fn description(&self, items: &[Self::Item]) -> String {
        DescriptionBuilder::new()
            .pluralize_with_count(Phrase::File(items.len()))
            .pluralize(Phrase::Has(items.len()))
            .text(MISSING_IMAGE)
            .pluralize(Phrase::Reference(items.len()))
            .build()
    }

    fn level(&self) -> &'static str { LEVEL2 }
}

impl ObsidianRepository {
    pub(super) fn write_missing_references_report(
        &self,
        config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let missing_refs: Vec<(PathBuf, String, usize, usize)> = self
            .markdown_files
            .files_to_persist()
            .iter()
            .flat_map(|file| {
                let missing_links = file.image_links.filter_by_variant(ImageLinkState::Missing);
                missing_links.into_iter().map(move |missing| {
                    (
                        file.path.clone(),
                        missing.filename.clone(),
                        missing.line_number,
                        missing.position,
                    )
                })
            })
            .collect();

        let report = ReportWriter::new(missing_refs).with_validated_config(config);
        report.write(&MissingReferencesTable, writer)
    }
}

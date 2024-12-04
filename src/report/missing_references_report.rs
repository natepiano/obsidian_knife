use crate::constants::*;
use crate::obsidian_repository_info::obsidian_repository_info_types::ImageGroup;
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::report::{format_references, ReportContext, ReportDefinition, ReportWriter};
use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

pub struct MissingReferencesTable;

impl ReportDefinition<ReportContext> for MissingReferencesTable {
    type Item = (PathBuf, String); // (markdown_path, extracted_filename)

    fn headers(&self) -> Vec<&str> {
        vec!["markdown file", "missing image reference", "action"]
    }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]
    }

    fn build_rows(&self, items: &[Self::Item], context: &ReportContext) -> Vec<Vec<String>> {
        // Group missing references by markdown file
        let mut grouped_references: HashMap<&PathBuf, Vec<ImageGroup>> = HashMap::new();
        for (markdown_path, extracted_filename) in items {
            grouped_references
                .entry(markdown_path)
                .or_default()
                .push(ImageGroup {
                    path: PathBuf::from(extracted_filename),
                    info: crate::obsidian_repository_info::obsidian_repository_info_types::ImageReferences {
                        hash: String::new(),
                        markdown_file_references: vec![markdown_path.to_string_lossy().to_string()],
                    },
                });
        }

        grouped_references
            .iter()
            .map(|(markdown_path, image_groups)| {
                let markdown_link =
                    crate::report::format_wikilink(markdown_path, context.obsidian_path(), false);
                let image_links = image_groups
                    .iter()
                    .map(|group| {
                        crate::utils::escape_pipe(&crate::utils::escape_brackets(
                            &group.path.to_string_lossy().to_string(),
                        ))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let actions = format_references(
                    context.apply_changes(),
                    context.obsidian_path(),
                    image_groups,
                    None,
                );
                vec![markdown_link, image_links, actions]
            })
            .collect()
    }

    fn title(&self) -> Option<&str> {
        Some(MISSING_IMAGE_REFERENCES)
    }

    fn description(&self, items: &[Self::Item]) -> String {
        DescriptionBuilder::new()
            .pluralize_with_count(Phrase::File(items.len()))
            .pluralize(Phrase::Has(items.len()))
            .text(MISSING_IMAGE)
            .pluralize(Phrase::Reference(items.len()))
            .build()
    }

    fn level(&self) -> &'static str {
        LEVEL2
    }
}

impl ObsidianRepositoryInfo {
    pub fn write_missing_references_report(
        &self,
        config: &ValidatedConfig,
        markdown_references_to_missing_image_files: &[(PathBuf, String)],
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let report = ReportWriter::new(markdown_references_to_missing_image_files.to_vec())
            .with_validated_config(config);

        report.write(&MissingReferencesTable, writer)
    }
}

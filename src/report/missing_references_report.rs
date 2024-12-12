use crate::constants::*;
use crate::obsidian_repository::obsidian_repository_types::{ImageGroup, ImageReferences};
use crate::obsidian_repository::ObsidianRepository;
use crate::report::{ReportDefinition, ReportWriter};
use crate::utils;
use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

pub struct MissingReferencesTable;

impl ReportDefinition for MissingReferencesTable {
    type Item = (PathBuf, String, usize); // (markdown_path, extracted_filename)

    fn headers(&self) -> Vec<&str> {
        vec![FILE, LINE, MISSING_IMAGE_REFERENCES, ACTION]
    }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![
            ColumnAlignment::Left,
            ColumnAlignment::Right,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]
    }

    fn build_rows(
        &self,
        items: &[Self::Item],
        config: Option<&ValidatedConfig>,
    ) -> Vec<Vec<String>> {
        let mut grouped_references: HashMap<(&PathBuf, usize), Vec<ImageGroup>> = HashMap::new(); // Changed key to include line number

        for (markdown_path, extracted_filename, line_number) in items {
            grouped_references
                .entry((markdown_path, *line_number))
                .or_default()
                .push(ImageGroup {
                    path: PathBuf::from(extracted_filename),
                    image_references: ImageReferences {
                        hash: String::new(),
                        markdown_file_references: vec![markdown_path.to_string_lossy().to_string()],
                    },
                });
        }

        let config = config.expect(CONFIG_EXPECT);
        grouped_references
            .iter()
            .map(|((markdown_path, line_number), image_groups)| {
                let markdown_link =
                    crate::report::format_wikilink(markdown_path, config.obsidian_path(), false);
                let image_links = image_groups
                    .iter()
                    .map(|group| {
                        utils::escape_pipe(&crate::utils::escape_brackets(
                            &group.path.to_string_lossy(),
                        ))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let action = if config.apply_changes() {
                    "reference removed"
                } else {
                    "reference will be removed"
                };
                vec![
                    markdown_link,
                    line_number.to_string(),
                    image_links,
                    action.to_string(),
                ]
            })
            .collect()
    }

    fn title(&self) -> Option<String> {
        Some(MISSING_IMAGE_REFERENCES.to_string())
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

impl ObsidianRepository {
    pub fn write_missing_references_report(
        &self,
        config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Collect missing references data in the format the report expects
        let missing_refs: Vec<(PathBuf, String, usize)> = self
            .markdown_files_to_persist
            .iter()
            .flat_map(|file| {
                // Collect missing links into a local variable
                let missing_links: Vec<_> = file.image_links.missing().links;
                missing_links.into_iter().map(move |missing| {
                    (
                        file.path.clone(),
                        missing.filename.clone(),
                        missing.line_number,
                    )
                })
            })
            .collect();

        let report = ReportWriter::new(missing_refs).with_validated_config(config);

        report.write(&MissingReferencesTable, writer)
    }
}

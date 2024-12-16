use crate::constants::*;
use crate::image_file::{ImageFile, ImageFileState};
use crate::obsidian_repository::ObsidianRepository;
use crate::report::{ReportDefinition, ReportWriter};
use crate::utils::{ColumnAlignment, OutputFileWriter, VecEnumFilter};
use crate::validated_config::ValidatedConfig;
use std::collections::HashMap;
use std::error::Error;

pub struct DuplicateImagesTable {
    hash: String,
}

impl ReportDefinition for DuplicateImagesTable {
    type Item = ImageFile;

    fn headers(&self) -> Vec<&str> {
        vec![IMAGE, FILE, REFERENCED_BY, ACTION, REFERENCE_CHANGE]
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

    fn build_rows(
        &self,
        items: &[Self::Item],
        config: Option<&ValidatedConfig>,
    ) -> Vec<Vec<String>> {
        let config = config.expect(CONFIG_EXPECT);

        items.iter().map(|image| {
            let filename = image.path.file_name().unwrap().to_string_lossy();
            let referenced_by = if image.references.is_empty() {
                NOT_REFERENCED.to_string()
            } else {
                image.references.iter()
                    .map(|ref_path| format!("[[{}]]", ref_path.file_name().unwrap().to_string_lossy()))
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            let (action, reference_update) = match &image.image_state {
                ImageFileState::DuplicateKeeper { .. } => (NO_CHANGE.to_string(), NO_CHANGE.to_string()),
                ImageFileState::Duplicate { hash: _ } => {
                    let action = if config.apply_changes() { DELETE } else { WILL_DELETE };
                    (action.to_string(), format!("![[{}]]", filename))
                }
                _ => (UNKNOWN.to_string(), UNKNOWN.to_string()),
            };

            vec![
                format!("![[{}\\|{}]]", filename, THUMBNAIL_WIDTH),
                format!("[[{}]]", filename),
                referenced_by,
                action,
                reference_update,
            ]
        }).collect()
    }

    fn title(&self) -> Option<String> {
        Some(format!("{}{} {}", IMAGE_FILE_HASH, COLON,  &self.hash))
    }

    fn description(&self, items: &[Self::Item]) -> String {
        let unique_references: std::collections::HashSet<_> =
            items.iter().flat_map(|img| &img.references).collect();

        DescriptionBuilder::new()
            .text(FOUND)
            .number(items.len())
            .text(DUPLICATE)
            .pluralize(Phrase::Image(items.len()))
            .text(REFERENCED_BY)
            .pluralize_with_count(Phrase::File(unique_references.len()))
            .build()
    }

    fn level(&self) -> &'static str {
        LEVEL3
    }
}

impl ObsidianRepository {
    pub fn write_duplicate_images_report(
        &self,
        config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        writer.writeln(LEVEL2, DUPLICATE_IMAGES)?;

        // Debug: Print total counts of duplicates and keepers
        let duplicates = self.image_files.filter_by_predicate(|state| matches!(state, ImageFileState::Duplicate { .. }));
        let keepers = self.image_files.filter_by_predicate(|state| matches!(state, ImageFileState::DuplicateKeeper { .. }));

        // Collect both duplicates and keepers by hash
        let mut grouped_by_hash: HashMap<String, Vec<ImageFile>> = HashMap::new();

        // Add duplicates
        for img in duplicates {
            if let ImageFileState::Duplicate { hash } = &img.image_state {
                grouped_by_hash.entry(hash.clone()).or_default().push(img);
            }
        }

        // Add keepers to their respective groups
        for img in keepers {
            if let ImageFileState::DuplicateKeeper { hash } = &img.image_state {
                grouped_by_hash.entry(hash.clone()).or_default().push(img);
            }
        }

        // Write report for each group that has both duplicates and keepers
        for (hash, images) in grouped_by_hash {
            // Only report groups that have both duplicates and keepers
            if images.iter().any(|img| matches!(img.image_state, ImageFileState::DuplicateKeeper { .. }))
                && images.iter().any(|img| matches!(img.image_state, ImageFileState::Duplicate { .. })) {

                let report = ReportWriter::new(images.to_vec()).with_validated_config(config);

                let table = DuplicateImagesTable {
                    hash: hash.to_string(),
                };

                report.write(&table, writer)?;
                writer.writeln("", "")?; // Add spacing between tables
            }
        }

        Ok(())
    }
}

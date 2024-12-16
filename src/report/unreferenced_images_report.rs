use crate::constants::*;
use crate::image_file::ImageFile;
use crate::image_file::ImageFileState;
use crate::obsidian_repository::ObsidianRepository;
use crate::report::{ReportDefinition, ReportWriter};
use crate::utils;
use crate::utils::{ColumnAlignment, OutputFileWriter, VecEnumFilter};
use crate::validated_config::ValidatedConfig;
use std::error::Error;

pub struct UnreferencedImagesReport;

impl ReportDefinition for UnreferencedImagesReport {
    type Item = ImageFile;

    fn headers(&self) -> Vec<&str> {
        vec![THUMBNAIL, IMAGE_FILE, ACTION]
    }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]
    }

    fn build_rows(&self, items: &[Self::Item], config: Option<&ValidatedConfig>) -> Vec<Vec<String>> {
        items
            .iter()
            .map(|image| {
                let file_name = image.path.file_name().unwrap().to_string_lossy();
                let sample =
                    utils::escape_pipe(format!("![[{}|{}]]", file_name, THUMBNAIL_WIDTH).as_str());
                let file_link = format!("[[{}]]", file_name);
                let action = if config.map_or(false, |c| c.apply_changes()) {
                    DELETED
                } else {
                    WILL_DELETE
                };

                vec![sample, file_link, action.to_string()]
            })
            .collect()
    }

    fn title(&self) -> Option<String> {
        Some(UNREFERENCED_IMAGES.to_string())
    }

    fn description(&self, items: &[Self::Item]) -> String {
        DescriptionBuilder::new()
            .pluralize_with_count(Phrase::Image(items.len()))
            .pluralize(Phrase::Is(items.len()))
            .text(NOT_REFERENCED)
            .build()
    }

    fn level(&self) -> &'static str {
        LEVEL2
    }
}

impl ObsidianRepository {
    pub fn write_unreferenced_images_report(
        &self,
        config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let unreferenced_images = self
            .image_files
            .filter_by_predicate(|state| matches!(state, ImageFileState::Unreferenced));

        if !unreferenced_images.is_empty() {
            let report =
                ReportWriter::new(unreferenced_images.to_owned()).with_validated_config(config);
            report.write(&UnreferencedImagesReport, writer)?;
        }

        Ok(())
    }
}

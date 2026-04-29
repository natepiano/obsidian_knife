use std::error::Error;

use super::report_writer::ReportDefinition;
use super::report_writer::ReportWriter;
use crate::constants::ACTION;
use crate::constants::DELETED;
use crate::constants::IMAGE_FILE;
use crate::constants::LEVEL2;
use crate::constants::NOT_REFERENCED;
use crate::constants::THUMBNAIL;
use crate::constants::THUMBNAIL_WIDTH;
use crate::constants::UNREFERENCED_IMAGES;
use crate::constants::WILL_DELETE;
use crate::description_builder::DescriptionBuilder;
use crate::description_builder::Phrase;
use crate::image_file::ImageFile;
use crate::image_file::ImageFileState;
use crate::obsidian_repository::ObsidianRepository;
use crate::utils;
use crate::utils::ColumnAlignment;
use crate::utils::OutputFileWriter;
use crate::utils::VecEnumFilter;
use crate::validated_config::ChangeMode;
use crate::validated_config::ValidatedConfig;

pub(super) struct UnreferencedImagesReport;

impl ReportDefinition for UnreferencedImagesReport {
    type Item = ImageFile;

    fn headers(&self) -> Vec<&str> { vec![THUMBNAIL, IMAGE_FILE, ACTION] }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]
    }

    fn build_rows(
        &self,
        items: &[Self::Item],
        config: Option<&ValidatedConfig>,
    ) -> anyhow::Result<Vec<Vec<String>>> {
        Ok(items
            .iter()
            .map(|image| {
                let file_name = image.path.file_name().unwrap_or_default().to_string_lossy();
                let sample =
                    utils::escape_pipe(format!("![[{file_name}|{THUMBNAIL_WIDTH}]]").as_str());
                let file_link = format!("[[{file_name}]]");
                let action = if config
                    .is_some_and(|config| matches!(config.change_mode(), ChangeMode::Apply))
                {
                    DELETED
                } else {
                    WILL_DELETE
                };

                vec![sample, file_link, action.to_string()]
            })
            .collect())
    }

    fn title(&self) -> Option<String> { Some(UNREFERENCED_IMAGES.to_string()) }

    fn description(&self, items: &[Self::Item]) -> String {
        DescriptionBuilder::new()
            .pluralize_with_count(Phrase::Image(items.len()))
            .pluralize(Phrase::Is(items.len()))
            .text(NOT_REFERENCED)
            .build()
    }

    fn level(&self) -> &'static str { LEVEL2 }
}

impl ObsidianRepository {
    pub(super) fn write_unreferenced_images_report(
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

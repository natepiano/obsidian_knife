use crate::constants::*;
use crate::obsidian_repository::obsidian_repository_types::{
    GroupedImages, ImageGroup, ImageGroupType,
};
use crate::obsidian_repository::ObsidianRepository;
use crate::report::{ReportDefinition, ReportWriter};
use crate::utils::{escape_pipe, ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use std::error::Error;

pub struct UnreferencedImagesReport;

impl ReportDefinition for UnreferencedImagesReport {
    type Item = ImageGroup;

    fn headers(&self) -> Vec<&str> {
        vec!["sample", "file"]
    }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![ColumnAlignment::Left, ColumnAlignment::Left]
    }

    fn build_rows(&self, items: &[Self::Item], _: Option<&ValidatedConfig>) -> Vec<Vec<String>> {
        items
            .iter()
            .map(|group| {
                let file_name = group.path.file_name().unwrap().to_string_lossy();
                let sample = escape_pipe(format!("![[{}|400]]", file_name).as_str());
                let file_link = format!("[[{}]]", file_name);

                vec![sample, file_link]
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
            .text(NOT_REFERENCED_BY_ANY_FILE)
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
        grouped_images: &GroupedImages,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(unreferenced_images) = grouped_images.get(&ImageGroupType::UnreferencedImage) {
            let report =
                ReportWriter::new(unreferenced_images.to_vec()).with_validated_config(config);
            report.write(&UnreferencedImagesReport, writer)?;
        };

        Ok(())
    }
}

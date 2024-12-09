use crate::constants::*;
use crate::obsidian_repository::obsidian_repository_types::{
    GroupedImages, ImageGroup, ImageGroupType,
};
use crate::obsidian_repository::ObsidianRepository;
use crate::report::{format_references, ReportDefinition, ReportWriter};
use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use std::error::Error;

impl ImageGroupType {
    fn title(&self) -> &'static str {
        match self {
            ImageGroupType::TiffImage => TIFF,
            ImageGroupType::ZeroByteImage => ZERO_BYTE,
            _ => "",
        }
    }

    fn description(&self, count: usize) -> String {
        let (incompatible_type_string, incompatible_message) = match self {
            ImageGroupType::TiffImage => (TIFF, NO_RENDER),
            ImageGroupType::ZeroByteImage => (ZERO_BYTE, NOT_VALID),
            _ => ("", ""),
        };

        DescriptionBuilder::new()
            .text(FOUND)
            .number(count)
            .text(incompatible_type_string)
            .pluralize(Phrase::Image(count))
            .text(incompatible_message)
            .build()
    }
}

impl ReportDefinition for ImageGroupType {
    type Item = ImageGroup;

    fn headers(&self) -> Vec<&str> {
        vec![FILE, REFERENCED_BY]
    }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![ColumnAlignment::Left, ColumnAlignment::Left]
    }

    fn build_rows(
        &self,
        items: &[Self::Item],
        config: Option<&ValidatedConfig>,
    ) -> Vec<Vec<String>> {
        items
            .iter()
            .map(|group| {
                let file_link =
                    format!("[[{}]]", group.path.file_name().unwrap().to_string_lossy());

                let config = config.expect(CONFIG_EXPECT);

                let references = if group.image_references.markdown_file_references.is_empty() {
                    String::from("not referenced by any file")
                } else {
                    format_references(
                        config.apply_changes(),
                        config.obsidian_path(),
                        &[group.clone()],
                        None,
                    )
                };

                vec![file_link, references]
            })
            .collect()
    }

    fn title(&self) -> Option<String> {
        Some(self.title().to_string())
    }

    fn description(&self, items: &[Self::Item]) -> String {
        self.description(items.len())
    }

    fn level(&self) -> &'static str {
        LEVEL2
    }
}

impl ObsidianRepository {
    pub fn write_tiff_images_report(
        &self,
        config: &ValidatedConfig,
        grouped_images: &GroupedImages,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Self::write_incompatible_image_report(
            config,
            grouped_images,
            writer,
            &ImageGroupType::TiffImage,
        )?;

        Ok(())
    }

    pub fn write_zero_byte_images_report(
        &self,
        config: &ValidatedConfig,
        grouped_images: &GroupedImages,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Self::write_incompatible_image_report(
            config,
            grouped_images,
            writer,
            &ImageGroupType::ZeroByteImage,
        )?;

        Ok(())
    }

    fn write_incompatible_image_report(
        config: &ValidatedConfig,
        grouped_images: &GroupedImages,
        writer: &OutputFileWriter,
        group_type: &ImageGroupType,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(zero_byte_images) = grouped_images.get(group_type) {
            let report = ReportWriter::new(zero_byte_images.to_vec()).with_validated_config(config);

            report.write(group_type, writer)?;
        };
        Ok(())
    }
}

use crate::constants::*;
use crate::image_file::{ImageFile, ImageFiles};
use crate::image_file::{ImageFileState, IncompatibilityReason};
use crate::obsidian_repository::ObsidianRepository;
use crate::report;
use crate::report::{ReportDefinition, ReportWriter};
use crate::utils::{ColumnAlignment, OutputFileWriter, VecEnumFilter};
use crate::validated_config::ValidatedConfig;
use std::error::Error;
use std::path::Path;

pub struct IncompatibleImagesReport {
    incompatibility_reason: IncompatibilityReason,
}

impl ReportDefinition for IncompatibleImagesReport {
    type Item = ImageFile;

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
            .map(|image| {
                let file_link =
                    format!("[[{}]]", image.path.file_name().unwrap().to_string_lossy());

                let config = config.expect(CONFIG_EXPECT);
                let references = if image.references.is_empty() {
                    String::from("not referenced by any file")
                } else {
                    format_image_file_references(
                        config.apply_changes(),
                        config.obsidian_path(),
                        &[image.clone()],
                    )
                };

                vec![file_link, references]
            })
            .collect()
    }

    fn title(&self) -> Option<String> {
        Some(match self.incompatibility_reason {
            IncompatibilityReason::TiffFormat => TIFF.to_string(),
            IncompatibilityReason::ZeroByte => ZERO_BYTE.to_string(),
        })
    }

    fn description(&self, items: &[Self::Item]) -> String {
        let (incompatible_type_string, incompatible_message) = match self.incompatibility_reason {
            IncompatibilityReason::TiffFormat => (TIFF, NO_RENDER),
            IncompatibilityReason::ZeroByte => (ZERO_BYTE, NOT_VALID),
        };

        DescriptionBuilder::new()
            .text(FOUND)
            .number(items.len())
            .text(incompatible_type_string)
            .pluralize(Phrase::Image(items.len()))
            .text(incompatible_message)
            .build()
    }

    fn level(&self) -> &'static str {
        LEVEL2
    }
}

impl ObsidianRepository {
    pub fn write_tiff_images_report(
        &self,
        config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Self::write_incompatible_image_report(
            config,
            &self.image_files,
            writer,
            IncompatibilityReason::TiffFormat,
        )
    }

    pub fn write_zero_byte_images_report(
        &self,
        config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Self::write_incompatible_image_report(
            config,
            &self.image_files,
            writer,
            IncompatibilityReason::ZeroByte,
        )
    }

    fn write_incompatible_image_report(
        config: &ValidatedConfig,
        images: &ImageFiles,
        writer: &OutputFileWriter,
        incompatibility_reason: IncompatibilityReason,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let incompatible_images = images.filter_by_predicate(|state| {
            matches!(
                state,
                ImageFileState::Incompatible { reason } if *reason == incompatibility_reason
            )
        });

        if !incompatible_images.is_empty() {
            let report =
                ReportWriter::new(incompatible_images.to_owned()).with_validated_config(config);
            report.write(
                &IncompatibleImagesReport {
                    incompatibility_reason,
                },
                writer,
            )?;
        }
        Ok(())
    }
}

// Updated version of format_references for ImageFile
fn format_image_file_references(
    apply_changes: bool,
    obsidian_path: &Path,
    images: &[ImageFile],
) -> String {
    let references: Vec<String> = images
        .iter()
        .flat_map(|image| &image.references)
        .map(|ref_path| {
            let mut link = format!(
                "{}",
                report::format_wikilink(Path::new(ref_path), obsidian_path, false)
            );

            if apply_changes {
                link.push_str(REFERENCE_REMOVED);
            } else {
                link.push_str(REFERENCE_WILL_BE_REMOVED);
            }
            link
        })
        .collect();

    references.join("<br>")
}

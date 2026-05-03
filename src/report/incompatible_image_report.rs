use std::error::Error;
use std::path::Path;

use super::orchestration;
use super::report_writer::ReportDefinition;
use super::report_writer::ReportWriter;
use crate::constants::ACTION;
use crate::constants::FILE;
use crate::constants::FOUND;
use crate::constants::IMAGE_FILE;
use crate::constants::INCOMPATIBLE_IMAGES;
use crate::constants::LEVEL2;
use crate::constants::LINE;
use crate::constants::NOT_REFERENCED;
use crate::constants::POSITION;
use crate::constants::REFERENCE_WILL_BE_REMOVED;
use crate::constants::TIFF;
use crate::constants::TYPE;
use crate::constants::WILL_DELETE;
use crate::constants::ZERO_BYTE;
use crate::description_builder::DescriptionBuilder;
use crate::description_builder::Phrase;
use crate::image_file::ImageFile;
use crate::image_file::ImageFileState;
use crate::image_file::IncompatibilityReason;
use crate::markdown_file::ImageLinkState;
use crate::markdown_files::MarkdownFiles;
use crate::obsidian_repository;
use crate::obsidian_repository::ObsidianRepository;
use crate::output_file_writer::ColumnAlignment;
use crate::output_file_writer::OutputFileWriter;
use crate::validated_config::ValidatedConfig;
use crate::vec_enum_filter::VecEnumFilter;

pub(super) struct IncompatibleImagesReport<'a> {
    markdown_files: &'a MarkdownFiles,
}

impl ReportDefinition for IncompatibleImagesReport<'_> {
    type Item = ImageFile;

    fn headers(&self) -> Vec<&str> { vec![IMAGE_FILE, TYPE, ACTION, FILE, LINE, POSITION, ACTION] }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Right,
            ColumnAlignment::Right,
            ColumnAlignment::Left,
        ]
    }

    fn build_rows(
        &self,
        items: &[Self::Item],
        config: Option<&ValidatedConfig>,
    ) -> anyhow::Result<Vec<Vec<String>>> {
        let validated_config = config.ok_or_else(|| {
            anyhow::anyhow!("ValidatedConfig required for incompatible-images report")
        })?;

        let mut rows = Vec::new();
        for image in items {
            let ImageFileState::Incompatible { reason } = &image.state else {
                // items are pre-filtered to incompatible images; skip if invariant is violated
                debug_assert!(false, "Only incompatible images should be in this report");
                continue;
            };

            let relative_path = obsidian_repository::format_relative_path(
                &image.path,
                validated_config.obsidian_path(),
            );
            let image_file_link = format!(
                "[{}]({})",
                image.path.file_name().unwrap_or_default().to_string_lossy(),
                relative_path
            );

            let incompatibility_type = match reason {
                IncompatibilityReason::TiffFormat => TIFF,
                IncompatibilityReason::ZeroByte => ZERO_BYTE,
            };

            if image.references.is_empty() {
                rows.push(vec![
                    image_file_link.clone(),
                    incompatibility_type.to_string(),
                    WILL_DELETE.to_string(),
                    NOT_REFERENCED.to_string(),
                    String::new(),
                    String::new(),
                    String::new(),
                ]);
            } else {
                for ref_path in &image.references {
                    // Only output the row if we can find the markdown file
                    if let Some(markdown_file) = self
                        .markdown_files
                        .iter()
                        .find(|f| f.path == Path::new(ref_path))
                    {
                        let file_link = orchestration::format_wikilink(
                            Path::new(ref_path),
                            validated_config.obsidian_path(),
                        );

                        // Find line number and position for this reference
                        let (line_number, position) = markdown_file.image_links.iter()
                            .find(|l| {
                                matches!(l.state, ImageLinkState::Incompatible { reason: ref link_reason } if link_reason == reason)
                            })
                            .map_or_else(
                                || (String::new(), String::new()),
                                |image_link| (image_link.line_number.to_string(), image_link.position.to_string()),
                            );

                        rows.push(vec![
                            image_file_link.clone(),
                            incompatibility_type.to_string(),
                            WILL_DELETE.to_string(),
                            file_link,
                            line_number,
                            position,
                            REFERENCE_WILL_BE_REMOVED.to_string(),
                        ]);
                    }
                }
            }
        }

        Ok(rows)
    }

    fn title(&self) -> Option<String> { Some(INCOMPATIBLE_IMAGES.to_string()) }

    fn description(&self, items: &[Self::Item]) -> String {
        let tiff_count = items.iter().filter(|i| {
            matches!(&i.state, ImageFileState::Incompatible { reason } if matches!(reason, IncompatibilityReason::TiffFormat))
        }).count();
        let zero_byte_count = items.iter().filter(|i| {
            matches!(&i.state, ImageFileState::Incompatible { reason } if matches!(reason, IncompatibilityReason::ZeroByte))
        }).count();

        DescriptionBuilder::new()
            .text(FOUND)
            .number(items.len())
            .text("incompatible")
            .pluralize(Phrase::Image(items.len()))
            .text("(")
            .number(tiff_count)
            .text(TIFF)
            .text("and")
            .number(zero_byte_count)
            .text(ZERO_BYTE)
            .text(")")
            .build()
    }

    fn level(&self) -> &'static str { LEVEL2 }
}

impl ObsidianRepository {
    pub(super) fn write_incompatible_image_report(
        &self,
        config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let incompatible_images = self
            .image_files
            .filter_by_predicate(|state| matches!(state, ImageFileState::Incompatible { .. }));

        if !incompatible_images.is_empty() {
            // Create the report instance first
            let report = IncompatibleImagesReport {
                markdown_files: &self.markdown_files.files_to_persist(),
            };

            // Check if there would be any rows after filtering
            let would_have_rows = incompatible_images.iter().any(|image| {
                image.references.is_empty()
                    || image.references.iter().any(|ref_path| {
                        self.markdown_files
                            .files_to_persist()
                            .iter()
                            .any(|f| f.path == Path::new(ref_path))
                    })
            });

            if would_have_rows {
                let report_writer =
                    ReportWriter::new(incompatible_images.to_owned()).with_validated_config(config);
                report_writer.write(&report, writer)?;
            }
        }
        Ok(())
    }
}

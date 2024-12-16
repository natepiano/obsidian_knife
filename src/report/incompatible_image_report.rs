use crate::constants::*;
use crate::image_file::{ImageFile, ImageFileState, IncompatibilityReason};
use crate::markdown_file::ImageLinkState;
use crate::markdown_files::MarkdownFiles;
use crate::obsidian_repository::ObsidianRepository;
use crate::report::{ReportDefinition, ReportWriter};
use crate::utils::{ColumnAlignment, OutputFileWriter, VecEnumFilter};
use crate::validated_config::ValidatedConfig;
use crate::{obsidian_repository, report};
use std::error::Error;
use std::path::Path;

pub struct IncompatibleImagesReport<'a> {
    markdown_files: &'a MarkdownFiles,
}

impl<'a> ReportDefinition for IncompatibleImagesReport<'a> {
    type Item = ImageFile;

    fn headers(&self) -> Vec<&str> {
        vec![IMAGE_FILE, TYPE, ACTION, FILE, LINE, POSITION, ACTION]
    }

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
    ) -> Vec<Vec<String>> {
        let config = config.expect(CONFIG_EXPECT);

        let mut rows = Vec::new();
        for image in items {
            let reason = match &image.image_state {
                ImageFileState::Incompatible { reason } => reason,
                _ => unreachable!("Only incompatible images should be in this report"),
            };

            let relative_path =
                obsidian_repository::format_relative_path(&image.path, config.obsidian_path());
            let image_file_link = format!(
                "[{}]({})",
                image.path.file_name().unwrap().to_string_lossy(),
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
                    "".to_string(),
                    "".to_string(),
                    "".to_string(),
                ]);
            } else {
                for ref_path in &image.references {
                    let file_link =
                        report::format_wikilink(Path::new(ref_path), config.obsidian_path(), false);

                    // Find line number and position for this reference
                    let (line_number, position) = self.markdown_files.iter()
                        .find(|f| f.path == Path::new(ref_path))
                        .and_then(|markdown_file| {
                            markdown_file.image_links.links.iter().find(|l| {
                                matches!(l.state, ImageLinkState::Incompatible { reason: ref link_reason } if link_reason == reason)
                            })
                        })
                        .map(|image_link| (image_link.line_number.to_string(), image_link.position.to_string()))
                        .unwrap_or(("".to_string(), "".to_string()));

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

        rows
    }

    fn title(&self) -> Option<String> {
        Some("incompatible images".to_string())
    }

    fn description(&self, items: &[Self::Item]) -> String {
        let tiff_count = items.iter().filter(|i| {
            matches!(&i.image_state, ImageFileState::Incompatible { reason } if matches!(reason, IncompatibilityReason::TiffFormat))
        }).count();
        let zero_byte_count = items.iter().filter(|i| {
            matches!(&i.image_state, ImageFileState::Incompatible { reason } if matches!(reason, IncompatibilityReason::ZeroByte))
        }).count();

        DescriptionBuilder::new()
            .text("found")
            .number(items.len())
            .text("incompatible")
            .pluralize(Phrase::Image(items.len()))
            .text("(")
            .number(tiff_count)
            .text("TIFF")
            .text("and")
            .number(zero_byte_count)
            .text("zero-byte")
            .text(")")
            .build()
    }

    fn level(&self) -> &'static str {
        LEVEL2
    }
}

impl ObsidianRepository {
    pub fn write_incompatible_image_report(
        &self,
        config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let incompatible_images = self
            .image_files
            .filter_by_predicate(|state| matches!(state, ImageFileState::Incompatible { .. }));

        if !incompatible_images.is_empty() {
            let report =
                ReportWriter::new(incompatible_images.to_owned()).with_validated_config(config);
            report.write(
                &IncompatibleImagesReport {
                    markdown_files: &self.markdown_files,
                },
                writer,
            )?;
        }
        Ok(())
    }
}

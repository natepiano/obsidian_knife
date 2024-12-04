use crate::constants::*;
use crate::obsidian_repository_info::obsidian_repository_info_types::ImageGroup;
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::report::{format_references, ReportContext, ReportDefinition, ReportWriter};
use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use std::error::Error;

pub enum IncompatibleImageType {
    Tiff,
    ZeroByte,
}

impl IncompatibleImageType {
    fn title(&self) -> &'static str {
        match self {
            IncompatibleImageType::Tiff => TIFF,
            IncompatibleImageType::ZeroByte => ZERO_BYTE,
        }
    }

    fn description(&self, count: usize) -> String {
        let (incompatible_type_string, incompatible_message) = match self {
            IncompatibleImageType::Tiff => (TIFF, NO_RENDER),
            IncompatibleImageType::ZeroByte => (ZERO_BYTE, NOT_VALID),
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

pub struct IncompatibleImageReport {
    report_type: IncompatibleImageType,
}

impl ReportDefinition<ReportContext> for IncompatibleImageReport {
    type Item = ImageGroup;

    fn headers(&self) -> Vec<&str> {
        vec!["file", "referenced by"]
    }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![ColumnAlignment::Left, ColumnAlignment::Left]
    }

    fn build_rows(&self, items: &[Self::Item], context: &ReportContext) -> Vec<Vec<String>> {
        items
            .iter()
            .map(|group| {
                let file_link =
                    format!("[[{}]]", group.path.file_name().unwrap().to_string_lossy());

                let references = if group.info.markdown_file_references.is_empty() {
                    String::from("not referenced by any file")
                } else {
                    format_references(
                        context.apply_changes(),
                        context.obsidian_path(),
                        &[group.clone()],
                        None,
                    )
                };

                vec![file_link, references]
            })
            .collect()
    }

    fn title(&self) -> Option<&str> {
        Some(self.report_type.title())
    }

    fn description(&self, items: &[Self::Item]) -> String {
        self.report_type.description(items.len())
    }

    fn level(&self) -> &'static str {
        LEVEL2
    }
}

impl ObsidianRepositoryInfo {
    pub fn write_tiff_images_report(
        &self,
        config: &ValidatedConfig,
        tiff_images: &[ImageGroup],
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let report = ReportWriter::new(tiff_images.to_vec()).with_validated_config(config);

        report.write(
            &IncompatibleImageReport {
                report_type: IncompatibleImageType::Tiff,
            },
            writer,
        )
    }

    pub fn write_zero_byte_images_report(
        &self,
        config: &ValidatedConfig,
        zero_byte_images: &[ImageGroup],
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let report = ReportWriter::new(zero_byte_images.to_vec()).with_validated_config(config);

        report.write(
            &IncompatibleImageReport {
                report_type: IncompatibleImageType::ZeroByte,
            },
            writer,
        )
    }
}

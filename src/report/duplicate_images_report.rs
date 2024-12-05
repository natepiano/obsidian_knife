use crate::constants::*;
use crate::obsidian_repository_info::obsidian_repository_info_types::{GroupedImages, ImageGroup};
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::report::{format_duplicates, format_references, ReportDefinition, ReportWriter};
use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use std::error::Error;

pub struct DuplicateImagesTable {
    hash: String,
    groups: Vec<ImageGroup>,
}

impl ReportDefinition for DuplicateImagesTable {
    type Item = ImageGroup;

    fn headers(&self) -> Vec<&str> {
        vec![SAMPLE, DUPLICATES, REFERENCED_BY]
    }

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
    ) -> Vec<Vec<String>> {
        let config = config.expect(CONFIG_EXPECT);
        let keeper_path = Some(&items[0].path);

        vec![vec![
            // Sample column with first image
            format!(
                "![[{}\\|400]]",
                items[0].path.file_name().unwrap().to_string_lossy()
            ),
            // Duplicates column with keeper/deletion status
            format_duplicates(config, items, keeper_path, false),
            // References column with update status
            format_references(
                config.apply_changes(),
                config.obsidian_path(),
                items,
                keeper_path,
            ),
        ]]
    }

    fn title(&self) -> Option<String> {
        Some(format!("image file hash: {}", &self.hash))
    }

    fn description(&self, items: &[Self::Item]) -> String {
        let total_references: usize = items
            .iter()
            .map(|g| g.info.markdown_file_references.len())
            .sum();

        DescriptionBuilder::new()
            .text(FOUND)
            .number(items.len())
            .text(DUPLICATE)
            .pluralize(Phrase::Image(items.len()))
            .text(REFERENCED_BY)
            .pluralize_with_count(Phrase::File(total_references))
            .build()
    }

    fn level(&self) -> &'static str {
        LEVEL3
    }
}

impl ObsidianRepositoryInfo {
    pub fn write_duplicate_images_report(
        &self,
        config: &ValidatedConfig,
        grouped_images: &GroupedImages,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        writer.writeln(LEVEL2, DUPLICATE_IMAGES_WITH_REFERENCES)?;

        for (hash, groups) in grouped_images.get_duplicate_groups() {
            let report = ReportWriter::new(groups.to_vec()).with_validated_config(config);

            let table = DuplicateImagesTable {
                hash: hash.to_string(),
                groups: groups.to_vec(),
            };

            report.write(&table, writer)?;
            writer.writeln("", "")?; // Add spacing between tables
        }

        Ok(())
    }
}

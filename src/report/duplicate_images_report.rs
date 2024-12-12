use crate::constants::*;
use crate::obsidian_repository::obsidian_repository_types::{GroupedImages, ImageGroup};
use crate::obsidian_repository::ObsidianRepository;
use crate::report;
use crate::report::{ ReportDefinition, ReportWriter};
use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};

pub struct DuplicateImagesTable {
    hash: String,
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
            report::format_duplicates(config, items, keeper_path, false),
            // References column with update status
            format_duplicate_references(
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
        let unique_references: std::collections::HashSet<_> = items
            .iter()
            .flat_map(|g| g.image_references.markdown_file_references.iter())
            .collect();

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
        grouped_images: &GroupedImages,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        writer.writeln(LEVEL2, DUPLICATE_IMAGES_WITH_REFERENCES)?;

        for (hash, groups) in grouped_images.get_duplicate_groups() {
            let report = ReportWriter::new(groups.to_vec()).with_validated_config(config);

            let table = DuplicateImagesTable {
                hash: hash.to_string(),
            };

            report.write(&table, writer)?;
            writer.writeln("", "")?; // Add spacing between tables
        }

        Ok(())
    }
}

// Add this new function specifically for duplicate images report
fn format_duplicate_references(
    apply_changes: bool,
    obsidian_path: &Path,
    groups: &[ImageGroup],
    keeper_path: Option<&PathBuf>,
) -> String {
    // First collect all unique references with their paths
    let mut unique_references: HashMap<String, Vec<&PathBuf>> = HashMap::new();

    for group in groups {
        for ref_path in &group.image_references.markdown_file_references {
            unique_references
                .entry(ref_path.clone())
                .or_default()
                .push(&group.path);
        }
    }

    // Convert to Vec for sorting
    let mut references: Vec<_> = unique_references.into_iter().collect();
    references.sort_by(|(a, _), (b, _)| a.cmp(b));

    // Format each unique reference
    references
        .iter()
        .enumerate()
        .map(|(index, (ref_path, group_paths))| {
            let mut link = format!(
                "{}. {}",
                index + 1,
                report::format_wikilink(Path::new(ref_path), obsidian_path, false)
            );

            // Add status only if any of the referenced images will be updated
            if apply_changes {
                if let Some(keeper) = keeper_path {
                    if group_paths.iter().any(|&path| path != keeper) {
                        link.push_str(UPDATED);
                    }
                }
            } else if keeper_path.is_some() {
                link.push_str(WILL_BE_UPDATED);
            }

            link
        })
        .collect::<Vec<_>>()
        .join("<br>")
}

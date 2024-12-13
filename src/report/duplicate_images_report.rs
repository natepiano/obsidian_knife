use crate::constants::*;
use crate::image_file::{ImageFile, ImageFileState};
use crate::obsidian_repository::ObsidianRepository;
use crate::report;
use crate::report::{ReportDefinition, ReportWriter};
use crate::utils::{ColumnAlignment, OutputFileWriter, VecEnumFilter};
use crate::validated_config::ValidatedConfig;
use itertools::Itertools;
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};

pub struct DuplicateImagesTable {
    hash: String,
}

impl ReportDefinition for DuplicateImagesTable {
    type Item = ImageFile;

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
            format_duplicates(config, items, keeper_path),
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
        let unique_references: std::collections::HashSet<_> =
            items.iter().flat_map(|img| &img.references).collect();

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
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        writer.writeln(LEVEL2, DUPLICATE_IMAGES_WITH_REFERENCES)?;

        // Get duplicate images and group by hash
        let grouped_by_hash: HashMap<String, Vec<ImageFile>> = self
            .image_files
            .filter_by_predicate(|state| matches!(state, ImageFileState::Duplicate { hash: _ }))
            .into_iter()
            .map(|img| {
                let ImageFileState::Duplicate { hash } = &img.image_state else {
                    unreachable!("filter_by_predicate ensures this is a Duplicate state")
                };
                (hash.clone(), img)
            })
            .into_group_map();

        // Write report for each group
        for (hash, images) in grouped_by_hash {
            let report = ReportWriter::new(images.to_vec()).with_validated_config(config);

            let table = DuplicateImagesTable {
                hash: hash.to_string(),
            };

            report.write(&table, writer)?;
            writer.writeln("", "")?; // Add spacing between tables
        }

        Ok(())
    }
}

fn format_duplicates(
    config: &ValidatedConfig,
    images: &[ImageFile],
    keeper_path: Option<&PathBuf>,
) -> String {
    images
        .iter()
        .enumerate()
        .map(|(i, image)| {
            let mut link = format!(
                "{}. {}",
                i + 1,
                report::format_wikilink(&image.path, config.obsidian_path(), true)
            );

            if config.apply_changes() {
                if let Some(keeper) = keeper_path {
                    if &image.path == keeper {
                        link.push_str(" - kept");
                    } else {
                        link.push_str(" - deleted");
                    }
                }
            }
            link
        })
        .collect::<Vec<_>>()
        .join("<br>")
}

fn format_duplicate_references(
    apply_changes: bool,
    obsidian_path: &Path,
    images: &[ImageFile],
    keeper_path: Option<&PathBuf>,
) -> String {
    // Collect and sort unique references with their paths
    let references: Vec<_> = images
        .iter()
        .flat_map(|image| {
            image
                .references
                .iter()
                .map(move |ref_path| (ref_path.to_owned(), &image.path))
        })
        .fold(
            HashMap::<PathBuf, Vec<&PathBuf>>::new(),
            |mut acc, (ref_path, image_path)| {
                acc.entry(ref_path).or_default().push(image_path);
                acc
            },
        )
        .into_iter()
        .sorted_by(|(a, _), (b, _)| a.cmp(b))
        .collect();

    // Format each unique reference
    references
        .iter()
        .enumerate()
        .map(|(index, (ref_path, image_paths))| {
            let mut link = format!(
                "{}. {}",
                index + 1,
                report::format_wikilink(ref_path, obsidian_path, false)
            );

            // Add status only if any of the referenced images will be updated
            if apply_changes {
                if let Some(keeper) = keeper_path {
                    if image_paths.iter().any(|&path| path != keeper) {
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

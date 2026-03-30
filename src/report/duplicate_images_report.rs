use std::collections::HashMap;
use std::error::Error;
use std::path::Path;

use super::orchestration::WikilinkFormat;
use super::report_writer::ReportDefinition;
use super::report_writer::ReportWriter;
use crate::constants::ACTION;
use crate::constants::COLON;
use crate::constants::CONFIG_EXPECT;
use crate::constants::DELETED;
use crate::constants::DUPLICATE;
use crate::constants::DUPLICATE_IMAGES;
use crate::constants::DescriptionBuilder;
use crate::constants::FILE;
use crate::constants::FOUND;
use crate::constants::IMAGE_FILE;
use crate::constants::IMAGE_FILE_HASH;
use crate::constants::LEVEL2;
use crate::constants::LEVEL3;
use crate::constants::LINE;
use crate::constants::NO_CHANGE;
use crate::constants::NOT_REFERENCED;
use crate::constants::POSITION;
use crate::constants::Phrase;
use crate::constants::REFERENCE_CHANGE;
use crate::constants::REFERENCED_BY;
use crate::constants::THUMBNAIL;
use crate::constants::THUMBNAIL_WIDTH;
use crate::constants::TYPE;
use crate::constants::UNKNOWN;
use crate::constants::WILL_DELETE;
use crate::image_file::DeletionStatus;
use crate::image_file::ImageFile;
use crate::image_file::ImageFileState;
use crate::image_file::ImageHash;
use crate::markdown_files::MarkdownFiles;
use crate::obsidian_repository::ObsidianRepository;
use crate::utils;
use crate::utils::ColumnAlignment;
use crate::utils::OutputFileWriter;
use crate::utils::VecEnumFilter;
use crate::validated_config::ValidatedConfig;

pub(super) struct DuplicateImagesTable<'a> {
    hash:           ImageHash,
    markdown_files: &'a MarkdownFiles,
}

impl ReportDefinition for DuplicateImagesTable<'_> {
    type Item = ImageFile;

    fn headers(&self) -> Vec<&str> {
        vec![
            THUMBNAIL,
            IMAGE_FILE,
            TYPE,
            FILE,
            LINE,
            POSITION,
            ACTION,
            REFERENCE_CHANGE,
        ]
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
            ColumnAlignment::Left,
        ]
    }

    #[allow(
        clippy::expect_used,
        reason = "config is structurally guaranteed Some by callers of this report"
    )]
    fn build_rows(
        &self,
        items: &[Self::Item],
        config: Option<&ValidatedConfig>,
    ) -> Vec<Vec<String>> {
        let config = config.expect(CONFIG_EXPECT);
        let keeper = items
            .iter()
            .find(|img| matches!(img.image_state, ImageFileState::DuplicateKeeper { .. }));

        let mut rows = Vec::new();
        for image in items {
            let filename = image.path.file_name().unwrap_or_default().to_string_lossy();
            let thumbnail = format!("![[{filename}\\|{THUMBNAIL_WIDTH}]]");
            let image_link = format!("[[{filename}]]");

            let (image_type, action, base_reference_update) = match &image.image_state {
                ImageFileState::DuplicateKeeper { .. } => {
                    ("keeper", NO_CHANGE.to_string(), NO_CHANGE.to_string())
                },
                ImageFileState::Duplicate { .. } => {
                    let action = if config.apply_changes() {
                        DELETED.to_string()
                    } else {
                        WILL_DELETE.to_string()
                    };

                    let reference_update = keeper.map_or_else(
                        || UNKNOWN.to_string(),
                        |keeper_img| {
                            let keeper_name = keeper_img
                                .path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy();
                            utils::escape_brackets(&format!("![[{keeper_name}]]"))
                        },
                    );

                    ("duplicate", action, reference_update)
                },
                _ => ("unknown", UNKNOWN.to_string(), UNKNOWN.to_string()),
            };

            if image.markdown_file_references.is_empty() {
                rows.push(vec![
                    thumbnail.clone(),
                    image_link.clone(),
                    image_type.to_string(),
                    NOT_REFERENCED.to_string(),
                    String::new(),
                    String::new(),
                    action.clone(),
                    String::new(), // No reference change for unreferenced files
                ]);
            } else {
                for ref_path in &image.markdown_file_references {
                    let file_link = super::orchestration::format_wikilink(
                        Path::new(ref_path),
                        config.obsidian_path(),
                        WikilinkFormat::StemOnly,
                    );

                    // Get line number and position from markdown files
                    let (line_number, position) = self
                        .markdown_files
                        .iter()
                        .find(|f| f.path == Path::new(ref_path))
                        .and_then(|markdown_file| {
                            let filename = image
                                .path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            markdown_file
                                .image_links
                                .iter()
                                .find(|l| l.filename == filename)
                        })
                        .map_or_else(
                            || (String::new(), String::new()),
                            |image_link| {
                                (
                                    image_link.line_number.to_string(),
                                    image_link.position.to_string(),
                                )
                            },
                        );

                    rows.push(vec![
                        thumbnail.clone(),
                        image_link.clone(),
                        image_type.to_string(),
                        file_link,
                        line_number,
                        position,
                        action.clone(),
                        if matches!(image.image_state, ImageFileState::Duplicate { .. }) {
                            base_reference_update.clone()
                        } else {
                            String::new()
                        },
                    ]);
                }
            }
        }
        rows.sort_by(|a, b| a[1].cmp(&b[1]));

        rows
    }

    fn title(&self) -> Option<String> {
        Some(format!("{IMAGE_FILE_HASH}{COLON} {}", &self.hash))
    }

    fn description(&self, items: &[Self::Item]) -> String {
        let unique_references: std::collections::HashSet<_> = items
            .iter()
            .flat_map(|img| &img.markdown_file_references)
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

    fn level(&self) -> &'static str { LEVEL3 }
}

impl ObsidianRepository {
    pub(super) fn write_duplicate_images_report(
        &self,
        config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Only write the header if we find at least one group with deletable duplicates
        let mut header_written = false;

        // Collect both duplicates and keepers by hash
        let mut grouped_by_hash: HashMap<ImageHash, Vec<ImageFile>> = HashMap::new();

        // Add duplicates
        let duplicates = self
            .image_files
            .filter_by_predicate(|state| matches!(state, ImageFileState::Duplicate { .. }));
        for img in duplicates {
            if let ImageFileState::Duplicate { hash } = &img.image_state {
                grouped_by_hash.entry(hash.clone()).or_default().push(img);
            }
        }

        // Add keepers to their respective groups
        let keepers = self
            .image_files
            .filter_by_predicate(|state| matches!(state, ImageFileState::DuplicateKeeper { .. }));
        for img in keepers {
            if let ImageFileState::DuplicateKeeper { hash } = &img.image_state {
                grouped_by_hash.entry(hash.clone()).or_default().push(img);
            }
        }

        // Write report for each group that has deletable duplicates
        for (hash, images) in grouped_by_hash {
            // Check if this group has any deletable duplicates
            if images.iter().any(|img| {
                matches!(img.image_state, ImageFileState::Duplicate { .. })
                    && img.deletion == DeletionStatus::Delete
            }) {
                if !header_written {
                    writer.writeln(LEVEL2, DUPLICATE_IMAGES)?;
                    header_written = true;
                }

                let report = ReportWriter::new(images.clone()).with_validated_config(config);

                let table = DuplicateImagesTable {
                    hash,
                    markdown_files: &self.markdown_files,
                };

                report.write(&table, writer)?;
                writer.writeln("", "")?; // Add spacing between tables
            }
        }

        Ok(())
    }
}

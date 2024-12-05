mod ambiguous_matches_report;
mod back_populate_report;
mod duplicate_images_report;
mod frontmatter_issues_report;
mod incompatible_image_report;
mod invalid_wikilink_report;
mod missing_references_report;
mod unreferenced_images_report;

mod report_writer;

pub use report_writer::*;

use crate::constants::*;
use crate::markdown_file_info::{MarkdownFileInfo, PersistReason};
use crate::obsidian_repository_info::{GroupedImages, ImageGroup, ObsidianRepositoryInfo};
use crate::utils::{escape_brackets, escape_pipe, ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use crate::wikilink::ToWikilink;
use chrono::Utc;
use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};

impl ObsidianRepositoryInfo {
    pub fn write_reports(
        &self,
        validated_config: &ValidatedConfig,
        grouped_images: &GroupedImages,
        markdown_references_to_missing_image_files: &Vec<(PathBuf, String)>,
        files_to_persist: &[&MarkdownFileInfo],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let writer = OutputFileWriter::new(validated_config.output_folder())?;

        self.write_execution_start(validated_config, &writer, files_to_persist)?;
        self.write_frontmatter_issues_report(&writer)?;

        writer.writeln(LEVEL1, IMAGES_SECTION)?;
        // hack just so cargo fmt doesn't expand the report call across multiple lines
        let missing_image_files = markdown_references_to_missing_image_files;
        self.write_missing_references_report(validated_config, missing_image_files, &writer)?;
        self.write_tiff_images_report(validated_config, grouped_images, &writer)?;
        self.write_zero_byte_images_report(validated_config, grouped_images, &writer)?;
        self.write_unreferenced_images_report(validated_config, grouped_images, &writer)?;
        self.write_duplicate_images_report(validated_config, grouped_images, &writer)?;

        // back populate reports
        write_back_populate_report_header(validated_config, &writer)?;
        self.write_invalid_wikilinks_report(&writer)?;
        self.write_ambiguous_matches_report(&writer)?;
        self.write_back_populate_report(&writer)?;

        // audit of persist reasons
        self.write_persist_reasons_table(&writer, files_to_persist)?;

        Ok(())
    }

    pub fn write_persist_reasons_table(
        &self,
        writer: &OutputFileWriter,
        files_to_persist: &[&MarkdownFileInfo],
    ) -> io::Result<()> {
        let mut rows: Vec<Vec<String>> = Vec::new();

        for file in &self.markdown_files.files {
            if !file.persist_reasons.is_empty() {
                let file_name = file
                    .path
                    .file_name()
                    .and_then(|f| f.to_str())
                    .map(|s| s.trim_end_matches(".md"))
                    .unwrap_or_default();

                let wikilink = format!("[[{}]]", file_name);

                // Count instances of BackPopulated and ImageReferencesModified
                let back_populate_count = file.matches.unambiguous.len();

                let image_refs_count = file
                    .persist_reasons
                    .iter()
                    .filter(|&r| matches!(r, PersistReason::ImageReferencesModified))
                    .count();

                // Generate rows for each persist reason
                for reason in &file.persist_reasons {
                    let (before, after, reason_info) = match reason {
                        PersistReason::DateCreatedUpdated { reason } => (
                            file.date_validation_created
                                .frontmatter_date
                                .clone()
                                .unwrap_or_default(),
                            format!(
                                "[[{}]]",
                                file.date_validation_created
                                    .file_system_date
                                    .format("%Y-%m-%d")
                            ),
                            reason.to_string(),
                        ),
                        PersistReason::DateModifiedUpdated { reason } => (
                            file.date_validation_modified
                                .frontmatter_date
                                .clone()
                                .unwrap_or_default(),
                            format!(
                                "[[{}]]",
                                file.date_validation_modified
                                    .file_system_date
                                    .format("%Y-%m-%d")
                            ),
                            reason.to_string(),
                        ),
                        PersistReason::DateCreatedFixApplied => (
                            file.date_created_fix
                                .date_string
                                .clone()
                                .unwrap_or_default(),
                            file.date_created_fix
                                .fix_date
                                .map(|d| format!("[[{}]]", d.format("%Y-%m-%d")))
                                .unwrap_or_default(),
                            String::new(),
                        ),
                        PersistReason::BackPopulated => (
                            String::new(),
                            String::new(),
                            format!("{} instances", back_populate_count),
                        ),
                        PersistReason::ImageReferencesModified => (
                            String::new(),
                            String::new(),
                            format!("{} instances", image_refs_count),
                        ),
                    };

                    rows.push(vec![
                        wikilink.clone(),
                        reason.to_string(),
                        reason_info,
                        before,
                        after,
                    ]);
                }
            }
        }

        if !rows.is_empty() {
            rows.sort_by(|a, b| {
                let file_cmp = a[0].to_lowercase().cmp(&b[0].to_lowercase());
                if file_cmp == std::cmp::Ordering::Equal {
                    a[1].cmp(&b[1])
                } else {
                    file_cmp
                }
            });

            writer.writeln(LEVEL1, "files to be updated")?;
            writer.writeln("", "")?;

            let headers = &["file", "persist reason", "info", "before", "after"];
            let alignments = &[
                ColumnAlignment::Left,
                ColumnAlignment::Left,
                ColumnAlignment::Left,
                ColumnAlignment::Left,
                ColumnAlignment::Left,
            ];

            for (i, chunk) in rows.chunks(500).enumerate() {
                if i > 0 {
                    writer.writeln("", "")?;
                }
                writer.write_markdown_table(headers, chunk, Some(alignments))?;
            }
        }

        Ok(())
    }

    pub fn write_execution_start(
        &self,
        validated_config: &ValidatedConfig,
        writer: &OutputFileWriter,
        files_to_persist: &[&MarkdownFileInfo],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let timestamp = Utc::now().format(FORMAT_TIME_STAMP);
        let properties = format!(
            "{}{}\n{}{}\n",
            YAML_TIMESTAMP,
            timestamp,
            YAML_APPLY_CHANGES,
            validated_config.apply_changes(),
        );

        writer.write_properties(&properties)?;

        if validated_config.apply_changes() {
            writer.writeln("", MODE_APPLY_CHANGES)?;
        } else {
            writer.writeln("", MODE_DRY_RUN)?;
        }

        if let Some(limit) = validated_config.file_process_limit() {
            writer.writeln("", format!("config.file_process_limit: {}", limit).as_str())?;
        }

        if validated_config.file_process_limit().is_some() {
            let total_files = self.markdown_files.get_files_to_persist(None).len();
            let message = format!(
                "{} {} {} {} {}",
                files_to_persist.len(),
                OF,
                total_files,
                pluralize(total_files, PhraseOld::Files),
                THAT_NEED_UPDATES,
            );
            writer.writeln("", message.as_str())?;
        }

        Ok(())
    }
}

fn write_back_populate_report_header(
    validated_config: &ValidatedConfig,
    writer: &OutputFileWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL1, BACK_POPULATE_COUNT_PREFIX)?;

    //output the name of the file filter if necessary
    if let Some(filter) = validated_config.back_populate_file_filter() {
        writer.writeln(
            "",
            &format!(
                "{} {}\n{}\n",
                BACK_POPULATE_FILE_FILTER_PREFIX,
                filter.to_wikilink(),
                BACK_POPULATE_FILE_FILTER_SUFFIX
            ),
        )?;
    }
    Ok(())
}

fn format_wikilink(path: &Path, obsidian_path: &Path, use_full_filename: bool) -> String {
    let relative_path = path.strip_prefix(obsidian_path).unwrap_or(path);
    let display_name = if use_full_filename {
        path.file_name().unwrap_or_default().to_string_lossy()
    } else {
        path.file_stem().unwrap_or_default().to_string_lossy()
    };
    format!("[[{}\\|{}]]", relative_path.display(), display_name)
}

fn format_duplicates(
    config: &ValidatedConfig,
    groups: &[ImageGroup],
    keeper_path: Option<&PathBuf>,
    is_special_group: bool,
) -> String {
    groups
        .iter()
        .enumerate()
        .map(|(i, group)| {
            let mut link = format!(
                "{}. {}",
                i + 1,
                format_wikilink(&group.path, config.obsidian_path(), true)
            );
            if config.apply_changes() {
                if is_special_group {
                    // For special files (zero byte, tiff, unreferenced), always delete
                    link.push_str(" - deleted");
                } else {
                    // For duplicate groups
                    if let Some(keeper) = keeper_path {
                        if &group.path == keeper {
                            link.push_str(" - kept");
                        } else {
                            link.push_str(" - deleted");
                        }
                    }
                }
            }
            link
        })
        .collect::<Vec<_>>()
        .join("<br>")
}

fn format_references(
    apply_changes: bool,
    obsidian_path: &Path,
    groups: &[ImageGroup],
    _keeper_path: Option<&PathBuf>, // Can remove this parameter since it's no longer needed
) -> String {
    let references: Vec<String> = groups
        .iter()
        .flat_map(|group| &group.info.markdown_file_references)
        .map(|ref_path| {
            let mut link = format!(
                "{}",
                format_wikilink(Path::new(ref_path), obsidian_path, false)
            );

            // Simpler status message - these reports only deal with removal
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

// Helper function to highlight all instances of a pattern in text
fn highlight_matches(text: &str, positions: &[usize], match_length: usize) -> String {
    let mut result = String::with_capacity(text.len() * 2);
    let mut last_end = 0;

    // Sort positions to ensure we process them in order
    let mut sorted_positions = positions.to_vec();
    sorted_positions.sort_unstable();

    for &start in sorted_positions.iter() {
        let end = start + match_length;

        // Validate UTF-8 boundaries
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            eprintln!(
                "Invalid UTF-8 boundary detected at position {} or {}",
                start, end
            );
            return text.to_string();
        }

        // Add text before the match
        result.push_str(&text[last_end..start]);

        // Add the highlighted match
        result.push_str("<span style=\"color: red;\">");
        result.push_str(&text[start..end]);
        result.push_str("</span>");

        last_end = end;
    }

    // Add any remaining text after the last match
    result.push_str(&text[last_end..]);
    result
}

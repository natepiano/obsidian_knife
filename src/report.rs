mod ambiguous_matches_report;
mod back_populate_report;
mod duplicate_images_report;
mod frontmatter_issues_report;
mod incompatible_image_report;
mod invalid_wikilink_report;
mod missing_references_report;
mod persist_reasons_report;
mod unreferenced_images_report;

mod report_writer;

pub use report_writer::*;

use crate::constants::*;
use crate::image_file::ImageFileState;
use crate::markdown_file::ImageLinkState;
use crate::utils::{OutputFileWriter, VecEnumFilter};
use crate::validated_config::ValidatedConfig;
use crate::wikilink::{InvalidWikilinkReason, ToWikilink};
use crate::ObsidianRepository;
use chrono::{Local, Utc};
use std::error::Error;
use std::path::Path;

impl ObsidianRepository {
    pub fn write_reports(
        &self,
        validated_config: &ValidatedConfig,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let writer = OutputFileWriter::new(validated_config.output_folder())?;

        self.write_execution_start(validated_config, &writer)?; // done
        self.write_frontmatter_issues_report(&writer)?; // done

        self.write_image_reports(validated_config, &writer)?;
        self.write_back_populate_reports(validated_config, &writer)?;

        // this report is slightly duplicative - i.e., outputting when
        // back populate or image references are updated - which already have reports
        // but it does uniquely represent any date changes so that's helpful
        // and mostly it acts as an audit
        self.write_persist_reasons_report(validated_config, &writer)?; // done

        Ok(())
    }

    pub fn write_back_populate_reports(
        &self,
        validated_config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Check for unambiguous matches in files_to_persist
        let has_back_populate_entries = self
            .markdown_files
            .files_to_persist()
            .iter()
            .any(|file| file.has_unambiguous_matches());

        // Check for ambiguous matches globally
        let has_ambiguous_matches = self
            .markdown_files
            .iter()
            .any(|file| file.has_ambiguous_matches());

        // Check for invalid wikilinks globally
        let has_invalid_wikilinks = self.markdown_files.iter().any(|file| {
            file.wikilinks.invalid.iter().any(|wikilink| {
                !matches!(
                    wikilink.reason,
                    InvalidWikilinkReason::EmailAddress
                        | InvalidWikilinkReason::Tag
                        | InvalidWikilinkReason::RawHttpLink
                )
            })
        });

        // Write the report header if any type of entry exists
        if has_back_populate_entries || has_invalid_wikilinks || has_ambiguous_matches {
            write_back_populate_report_header(validated_config, writer)?;

            if has_invalid_wikilinks {
                self.write_invalid_wikilinks_report(writer)?;
            }

            if has_ambiguous_matches {
                self.write_ambiguous_matches_report(writer)?;
            }

            if has_back_populate_entries {
                self.write_back_populate_report(writer)?;
            }
        }

        Ok(())
    }

    fn write_image_reports(
        &self,
        validated_config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // only output image reports if we have image files to report on or markdown files with
        // missing image references
        let has_report_entries = self.image_files.files.iter().any(|image| {
            matches!(
                image.image_state,
                ImageFileState::Unreferenced
                    | ImageFileState::Duplicate { .. }
                    | ImageFileState::Incompatible { .. }
            )
        }) || self.markdown_files.files_to_persist().iter().any(|file| {
            !file
                .image_links
                .filter_by_variant(ImageLinkState::Missing)
                .is_empty()
        });

        if has_report_entries {
            writer.writeln(LEVEL1, IMAGES)?;

            self.write_missing_references_report(validated_config, writer)?;
            self.write_incompatible_image_report(validated_config, writer)?;
            self.write_unreferenced_images_report(validated_config, writer)?;
            self.write_duplicate_images_report(validated_config, writer)?;
        }

        Ok(())
    }

    pub fn write_execution_start(
        &self,
        validated_config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let timestamp_utc = Utc::now().format(FORMAT_TIME_STAMP);
        let timestamp_local = Local::now().format(FORMAT_TIME_STAMP);

        let limit_string = validated_config
            .file_limit()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "None".to_string());

        let apply_changes = validated_config.apply_changes();

        let properties = DescriptionBuilder::new()
            .no_space(YAML_TIMESTAMP_UTC)
            .text_with_newline(&timestamp_utc.to_string())
            .no_space(YAML_TIMESTAMP_LOCAL)
            .text_with_newline(&timestamp_local.to_string())
            .no_space(YAML_APPLY_CHANGES)
            .text_with_newline(&apply_changes.to_string())
            .no_space(YAML_FILE_LIMIT)
            .text_with_newline(&limit_string)
            .build();

        writer.write_properties(&properties)?;

        let total_files_to_persist = self.markdown_files.total_files_to_persist();
        let files_to_persist = self.markdown_files.files_to_persist().len();

        let message = match validated_config.file_limit() {
            Some(_) => DescriptionBuilder::new()
                .number(files_to_persist)
                .text(OF)
                .pluralize_with_count(Phrase::File(total_files_to_persist))
                .text(IN_CHANGESET)
                .build(),
            None => DescriptionBuilder::new()
                .pluralize_with_count(Phrase::File(total_files_to_persist))
                .text(IN_CHANGESET)
                .build(),
        };

        writer.writeln("", message.as_str())?;

        if validated_config.apply_changes() {
            writer.writeln("", MODE_APPLY_CHANGES)?;
        } else {
            writer.writeln("", MODE_APPLY_CHANGES_OFF)?;
        }

        Ok(())
    }
}

fn write_back_populate_report_header(
    validated_config: &ValidatedConfig,
    writer: &OutputFileWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL1, BACK_POPULATE)?;

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

use std::error::Error;
use std::path::Path;

use chrono::Local;
use chrono::Utc;

use crate::constants::BACK_POPULATE;
use crate::constants::BACK_POPULATE_FILE_FILTER_PREFIX;
use crate::constants::BACK_POPULATE_FILE_FILTER_SUFFIX;
use crate::constants::FORMAT_TIME_STAMP;
use crate::constants::IMAGES;
use crate::constants::IN_CHANGESET;
use crate::constants::LEVEL1;
use crate::constants::MODE_APPLY_CHANGES;
use crate::constants::MODE_APPLY_CHANGES_OFF;
use crate::constants::OF;
use crate::constants::YAML_APPLY_CHANGES;
use crate::constants::YAML_FILE_LIMIT;
use crate::constants::YAML_TIMESTAMP_LOCAL;
use crate::constants::YAML_TIMESTAMP_UTC;
use crate::description_builder::DescriptionBuilder;
use crate::description_builder::Phrase;
use crate::image_file::ImageFileState;
use crate::markdown_file::ImageLinkState;
use crate::markdown_file::MarkdownFile;
use crate::markdown_file::PersistReason;
use crate::obsidian_repository::ObsidianRepository;
use crate::utils::OutputFileWriter;
use crate::utils::VecEnumFilter;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::InvalidWikilinkReason;
use crate::wikilink::ToWikilink;

impl ObsidianRepository {
    pub fn write_reports(
        &self,
        validated_config: &ValidatedConfig,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_reports_impl(validated_config)
    }

    fn write_reports_impl(
        &self,
        validated_config: &ValidatedConfig,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let writer = OutputFileWriter::new(validated_config.output_folder())?;

        self.write_execution_start(validated_config, &writer)?;
        self.write_frontmatter_issues_report(&writer)?;

        self.write_image_reports(validated_config, &writer)?;
        self.write_ambiguous_matches_reports(&writer)?;
        self.write_back_populate_reports(validated_config, &writer)?;

        // This report is slightly duplicative because image reference updates and back-populate
        // updates already have dedicated reports. It still captures date changes clearly, so it
        // remains useful as an audit trail.
        self.write_persist_reasons_report(validated_config, &writer)?;

        Ok(())
    }

    fn write_ambiguous_matches_reports(
        &self,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let has_ambiguous_matches = self
            .markdown_files
            .iter()
            .any(MarkdownFile::has_ambiguous_matches);

        if has_ambiguous_matches {
            self.write_ambiguous_matches_report(writer)?;
        }

        Ok(())
    }

    fn write_back_populate_reports(
        &self,
        validated_config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let has_back_populate_entries = self
            .markdown_files
            .files_to_persist()
            .iter()
            .any(MarkdownFile::has_unambiguous_matches);

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

        let has_frontmatter_created = self.markdown_files.iter().any(|file| {
            file.persist_reasons
                .iter()
                .any(|r| matches!(r, PersistReason::FrontmatterCreated))
        });

        if has_back_populate_entries || has_invalid_wikilinks || has_frontmatter_created {
            write_back_populate_report_header(validated_config, writer)?;

            if has_frontmatter_created {
                self.write_add_frontmatter_report(writer)?;
            }

            if has_invalid_wikilinks {
                self.write_invalid_wikilinks_report(writer)?;
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
        let has_report_entries = self.image_files.files.iter().any(|image| {
            matches!(
                image.state,
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

    fn write_execution_start(
        &self,
        validated_config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let timestamp_utc = Utc::now().format(FORMAT_TIME_STAMP);
        let timestamp_local = Local::now().format(FORMAT_TIME_STAMP);

        let limit_string = validated_config
            .file_limit()
            .map_or_else(|| "None".to_string(), |value| value.to_string());

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

    if let Some(filter) = validated_config.back_populate_file_filter() {
        writer.writeln(
            "",
            &format!(
                "{BACK_POPULATE_FILE_FILTER_PREFIX} {}\n{BACK_POPULATE_FILE_FILTER_SUFFIX}\n",
                filter.to_wikilink(),
            ),
        )?;
    }

    Ok(())
}

pub(super) fn format_wikilink(path: &Path, obsidian_path: &Path) -> String {
    let relative_path = path.strip_prefix(obsidian_path).unwrap_or(path);
    let display_name = path.file_stem().unwrap_or_default().to_string_lossy();

    format!("[[{}\\|{display_name}]]", relative_path.display())
}

pub(super) fn highlight_matches(text: &str, positions: &[usize], match_length: usize) -> String {
    let mut result = String::with_capacity(text.len() * 2);
    let mut last_end = 0;

    let mut sorted_positions = positions.to_vec();
    sorted_positions.sort_unstable();

    for &start in &sorted_positions {
        let end = start + match_length;

        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            eprintln!("Invalid UTF-8 boundary detected at position {start} or {end}");
            return text.to_string();
        }

        result.push_str(&text[last_end..start]);
        result.push_str("<span style=\"color: red;\">");
        result.push_str(&text[start..end]);
        result.push_str("</span>");
        last_end = end;
    }

    result.push_str(&text[last_end..]);
    result
}

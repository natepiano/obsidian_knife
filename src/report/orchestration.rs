use std::error::Error;

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
use crate::constants::YAML_FALSE;
use crate::constants::YAML_FILE_LIMIT;
use crate::constants::YAML_NONE;
use crate::constants::YAML_TIMESTAMP_LOCAL;
use crate::constants::YAML_TIMESTAMP_UTC;
use crate::constants::YAML_TRUE;
use crate::description_builder::DescriptionBuilder;
use crate::image_file::ImageFileState;
use crate::markdown_file::ImageLinkState;
use crate::markdown_file::MarkdownFile;
use crate::markdown_file::PersistReason;
use crate::obsidian_repository::ObsidianRepository;
use crate::output_file_writer::OutputFileWriter;
use crate::phrase::Phrase;
use crate::support::VecEnumFilter;
use crate::validated_config::ChangeMode;
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
        let output_file_writer = OutputFileWriter::new(validated_config.output_folder())?;

        self.write_execution_start(validated_config, &output_file_writer)?;
        self.write_frontmatter_issues_report(&output_file_writer)?;

        self.write_image_reports(validated_config, &output_file_writer)?;
        self.write_ambiguous_matches_reports(&output_file_writer)?;
        self.write_unresolved_links_report(&output_file_writer)?;
        self.write_back_populate_reports(validated_config, &output_file_writer)?;

        // This report is slightly duplicative because image reference updates and back-populate
        // updates already have dedicated reports. It still captures date changes clearly, so it
        // remains useful as an audit trail.
        self.write_persist_reasons_report(validated_config, &output_file_writer)?;

        Ok(())
    }

    fn write_ambiguous_matches_reports(
        &self,
        output_file_writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let has_ambiguous_matches = self
            .markdown_files
            .iter()
            .any(MarkdownFile::has_ambiguous_matches);

        if has_ambiguous_matches {
            self.write_ambiguous_matches_report(output_file_writer)?;
        }

        Ok(())
    }

    fn write_back_populate_reports(
        &self,
        validated_config: &ValidatedConfig,
        output_file_writer: &OutputFileWriter,
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

        let has_canonical_links = self
            .markdown_files
            .files_to_persist()
            .iter()
            .any(MarkdownFile::has_canonical_link_matches);

        let has_phantom_links = self
            .markdown_files
            .files_to_persist()
            .iter()
            .any(MarkdownFile::has_phantom_link_matches);

        if has_back_populate_entries
            || has_invalid_wikilinks
            || has_frontmatter_created
            || has_canonical_links
            || has_phantom_links
        {
            write_back_populate_report_header(validated_config, output_file_writer)?;

            if has_frontmatter_created {
                self.write_add_frontmatter_report(output_file_writer)?;
            }

            if has_invalid_wikilinks {
                self.write_invalid_wikilinks_report(output_file_writer)?;
            }

            if has_canonical_links {
                self.write_canonical_links_report(output_file_writer)?;
            }

            if has_phantom_links {
                self.write_phantom_links_report(output_file_writer)?;
            }

            if has_back_populate_entries {
                self.write_back_populate_report(output_file_writer)?;
            }
        }

        Ok(())
    }

    fn write_image_reports(
        &self,
        validated_config: &ValidatedConfig,
        output_file_writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let has_report_entries = self.image_files.images.iter().any(|image| {
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
            output_file_writer.writeln(LEVEL1, IMAGES)?;

            self.write_missing_references_report(validated_config, output_file_writer)?;
            self.write_incompatible_image_report(validated_config, output_file_writer)?;
            self.write_unreferenced_images_report(validated_config, output_file_writer)?;
            self.write_duplicate_images_report(validated_config, output_file_writer)?;
        }

        Ok(())
    }

    fn write_execution_start(
        &self,
        validated_config: &ValidatedConfig,
        output_file_writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let timestamp_utc = Utc::now().format(FORMAT_TIME_STAMP);
        let timestamp_local = Local::now().format(FORMAT_TIME_STAMP);

        let limit_string = validated_config
            .file_limit()
            .map_or_else(|| String::from(YAML_NONE), |value| value.to_string());

        let change_mode = validated_config.change_mode();
        let apply_changes = match change_mode {
            ChangeMode::Apply => YAML_TRUE,
            ChangeMode::DryRun => YAML_FALSE,
        };

        let properties = DescriptionBuilder::new()
            .no_space(YAML_TIMESTAMP_UTC)
            .text_with_newline(&timestamp_utc.to_string())
            .no_space(YAML_TIMESTAMP_LOCAL)
            .text_with_newline(&timestamp_local.to_string())
            .no_space(YAML_APPLY_CHANGES)
            .text_with_newline(apply_changes)
            .no_space(YAML_FILE_LIMIT)
            .text_with_newline(&limit_string)
            .build();

        output_file_writer.write_properties(&properties)?;

        let total_files_to_persist = self.markdown_files.total_files_to_persist();
        let files_to_persist = self.markdown_files.files_to_persist().len();

        let message = validated_config.file_limit().map_or_else(
            || {
                DescriptionBuilder::new()
                    .pluralize_with_count(Phrase::File(total_files_to_persist))
                    .text(IN_CHANGESET)
                    .build()
            },
            |_| {
                DescriptionBuilder::new()
                    .number(files_to_persist)
                    .text(OF)
                    .pluralize_with_count(Phrase::File(total_files_to_persist))
                    .text(IN_CHANGESET)
                    .build()
            },
        );

        output_file_writer.writeln("", message.as_str())?;

        match change_mode {
            ChangeMode::Apply => output_file_writer.writeln("", MODE_APPLY_CHANGES)?,
            ChangeMode::DryRun => output_file_writer.writeln("", MODE_APPLY_CHANGES_OFF)?,
        }

        Ok(())
    }
}

fn write_back_populate_report_header(
    validated_config: &ValidatedConfig,
    output_file_writer: &OutputFileWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    output_file_writer.writeln(LEVEL1, BACK_POPULATE)?;

    if let Some(filter) = validated_config.back_populate_file_filter() {
        output_file_writer.writeln(
            "",
            &format!(
                "{BACK_POPULATE_FILE_FILTER_PREFIX} {}\n{BACK_POPULATE_FILE_FILTER_SUFFIX}\n",
                filter.to_wikilink(),
            ),
        )?;
    }

    Ok(())
}

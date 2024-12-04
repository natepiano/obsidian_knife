mod invalid_wikilink_report;
mod table_system;

pub use table_system::*;

use crate::constants::*;
use crate::markdown_file_info::{BackPopulateMatch, MarkdownFileInfo};
use crate::obsidian_repository_info::obsidian_repository_info_types::{
    GroupedImages, ImageGroup, ImageGroupType, ImageReferences,
};
use crate::obsidian_repository_info::{write_back_populate_table, ObsidianRepositoryInfo};
use crate::utils::{escape_brackets, escape_pipe, ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use crate::wikilink;
use crate::wikilink::ToWikilink;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::error::Error;
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
        self.write_execution_start(&validated_config, &writer, files_to_persist)?;

        self.report_frontmatter_issues(&writer)?;

        self.write_image_analysis(
            &validated_config,
            &writer,
            &grouped_images,
            &markdown_references_to_missing_image_files,
            files_to_persist,
        )?;

        self.write_back_populate_tables(&validated_config, &writer, files_to_persist)?;

        self.markdown_files
            .write_persist_reasons_table(&writer, files_to_persist)?;

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

        if let Some(_) = validated_config.file_process_limit() {
            let total_files = self.markdown_files.get_files_to_persist(None).len();
            let message = format!(
                "{} {} {} {} {}",
                files_to_persist.len(),
                OF,
                total_files,
                pluralize(total_files, Phrase::Files),
                THAT_NEED_UPDATES,
            );
            writer.writeln("", message.as_str())?;
        }

        Ok(())
    }

    pub fn report_frontmatter_issues(
        &self,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let files_with_errors: Vec<_> = self
            .markdown_files
            .files
            .iter()
            .filter_map(|info| info.frontmatter_error.as_ref().map(|err| (&info.path, err)))
            .collect();

        if files_with_errors.is_empty() {
            return Ok(());
        }

        writer.writeln(LEVEL1, FRONTMATTER_ISSUES)?;

        writer.writeln(
            "",
            &format!(
                "found {} files with frontmatter parsing errors",
                files_with_errors.len()
            ),
        )?;

        for (path, err) in files_with_errors {
            writer.writeln(
                LEVEL3,
                &format!("in file {}", wikilink::format_path_as_wikilink(path)),
            )?;
            writer.writeln("", &format!("{}", err))?;
            writer.writeln("", "")?;
        }

        Ok(())
    }

    fn write_image_analysis(
        &self,
        config: &ValidatedConfig,
        writer: &OutputFileWriter,
        grouped_images: &GroupedImages,
        markdown_references_to_missing_image_files: &[(PathBuf, String)],
        files_to_persist: &[&MarkdownFileInfo],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        writer.writeln(LEVEL1, SECTION_IMAGE_CLEANUP)?;

        let empty_vec = Vec::new();

        let tiff_images = grouped_images
            .get(&ImageGroupType::TiffImage)
            .unwrap_or(&empty_vec);
        let zero_byte_images = grouped_images
            .get(&ImageGroupType::ZeroByteImage)
            .unwrap_or(&empty_vec);
        let unreferenced_images = grouped_images
            .get(&ImageGroupType::UnreferencedImage)
            .unwrap_or(&empty_vec);

        let duplicate_groups = grouped_images.get_duplicate_groups();

        if tiff_images.is_empty()
            && zero_byte_images.is_empty()
            && unreferenced_images.is_empty()
            && duplicate_groups.is_empty()
            && markdown_references_to_missing_image_files.is_empty()
        {
            return Ok(());
        }

        write_image_tables(
            config,
            writer,
            markdown_references_to_missing_image_files,
            tiff_images,
            zero_byte_images,
            unreferenced_images,
            &duplicate_groups,
        )?;

        Ok(())
    }

    pub fn write_back_populate_tables(
        &self,
        config: &ValidatedConfig,
        writer: &OutputFileWriter,
        files_to_persist: &[&MarkdownFileInfo],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        writer.writeln(LEVEL1, BACK_POPULATE_COUNT_PREFIX)?;

        if let Some(filter) = config.back_populate_file_filter() {
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

        // only writes if there are any
        self.write_invalid_wikilinks_table(&writer)?;

        // only writes if there are any
        self.write_ambiguous_matches_table(writer)?;

        let unambiguous_matches = self.markdown_files.unambiguous_matches();

        if !unambiguous_matches.is_empty() {
            write_back_populate_table(
                writer,
                &unambiguous_matches,
                true,
                self.wikilinks_sorted.len(),
            )?;
        }

        Ok(())
    }

    pub fn write_ambiguous_matches_table(
        &self,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Skip if no files have ambiguous matches
        let has_ambiguous = self
            .markdown_files
            .iter()
            .any(|file| !file.matches.ambiguous.is_empty());

        if !has_ambiguous {
            return Ok(());
        }

        writer.writeln(LEVEL2, MATCHES_AMBIGUOUS)?;

        // Create a map to group ambiguous matches by their display text (case-insensitive)
        let mut matches_by_text: HashMap<String, (HashSet<String>, Vec<BackPopulateMatch>)> =
            HashMap::new();

        // First pass: collect all matches and their targets
        for markdown_file in self.markdown_files.iter() {
            for match_info in &markdown_file.matches.ambiguous {
                let key = match_info.found_text.to_lowercase();
                let entry = matches_by_text
                    .entry(key)
                    .or_insert((HashSet::new(), Vec::new()));
                entry.1.push(match_info.clone());
            }
        }

        // Second pass: collect targets for each found text
        for wikilink in &self.wikilinks_sorted {
            if let Some(entry) = matches_by_text.get_mut(&wikilink.display_text.to_lowercase()) {
                entry.0.insert(wikilink.target.clone());
            }
        }

        // Convert to sorted vec for consistent output
        let mut sorted_matches: Vec<_> = matches_by_text.into_iter().collect();
        sorted_matches.sort_by(|(a, _), (b, _)| a.cmp(b));

        // Write out each group of matches
        for (display_text, (targets, matches)) in sorted_matches {
            writer.writeln(
                LEVEL3,
                &format!("\"{}\" matches {} targets:", display_text, targets.len(),),
            )?;

            // Write out all possible targets
            let mut sorted_targets: Vec<_> = targets.into_iter().collect();
            sorted_targets.sort();
            for target in sorted_targets {
                writer.writeln(
                    "",
                    &format!("- \\[\\[{}|{}]]", target.to_wikilink(), display_text),
                )?;
            }

            // Reuse existing table writing code for the matches
            write_back_populate_table(writer, &matches, false, 0)?;
        }

        Ok(())
    }
}

fn write_image_tables(
    config: &ValidatedConfig,
    writer: &OutputFileWriter,
    markdown_references_to_missing_image_files: &[(PathBuf, String)],
    tiff_images: &[ImageGroup],
    zero_byte_images: &[ImageGroup],
    unreferenced_images: &[ImageGroup],
    duplicate_groups: &[(&String, &Vec<ImageGroup>)],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    write_missing_references_table(config, markdown_references_to_missing_image_files, writer)?;

    if !tiff_images.is_empty() {
        write_special_image_group_table(
            config,
            writer,
            TIFF_IMAGES,
            tiff_images,
            Phrase::TiffImages,
        )?;
    }

    if !zero_byte_images.is_empty() {
        write_special_image_group_table(
            config,
            writer,
            ZERO_BYTE_IMAGES,
            zero_byte_images,
            Phrase::ZeroByteImages,
        )?;
    }

    if !unreferenced_images.is_empty() {
        write_special_image_group_table(
            config,
            writer,
            UNREFERENCED_IMAGES,
            unreferenced_images,
            Phrase::UnreferencedImages,
        )?;
    }

    for (hash, group) in duplicate_groups {
        write_duplicate_group_table(config, writer, hash, group)?;
    }

    Ok(())
}

fn write_missing_references_table(
    config: &ValidatedConfig,
    markdown_references_to_missing_image_files: &[(PathBuf, String)],
    writer: &OutputFileWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if markdown_references_to_missing_image_files.is_empty() {
        return Ok(());
    }

    writer.writeln(LEVEL2, MISSING_IMAGE_REFERENCES)?;
    writer.writeln_pluralized(
        markdown_references_to_missing_image_files.len(),
        Phrase::MissingImageReferences,
    )?;

    let headers = &["markdown file", "missing image reference", "action"];

    // Group missing references by markdown file
    let mut grouped_references: HashMap<&PathBuf, Vec<ImageGroup>> = HashMap::new();
    for (markdown_path, extracted_filename) in markdown_references_to_missing_image_files {
        grouped_references
            .entry(markdown_path)
            .or_default()
            .push(ImageGroup {
                path: PathBuf::from(extracted_filename),
                info: ImageReferences {
                    hash: String::new(),
                    markdown_file_references: vec![markdown_path.to_string_lossy().to_string()],
                },
            });
    }

    let rows: Vec<Vec<String>> = grouped_references
        .iter()
        .map(|(markdown_path, image_groups)| {
            let markdown_link = format_wikilink(markdown_path, config.obsidian_path(), false);
            let image_links = image_groups
                .iter()
                .map(|group| {
                    escape_pipe(&escape_brackets(&group.path.to_string_lossy().to_string()))
                })
                .collect::<Vec<_>>()
                .join(", ");
            let actions = format_references(config, image_groups, None);
            vec![markdown_link, image_links, actions]
        })
        .collect();

    writer.write_markdown_table(
        headers,
        &rows,
        Some(&[
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]),
    )?;

    Ok(())
}

fn write_duplicate_group_table(
    config: &ValidatedConfig,
    writer: &OutputFileWriter,
    group_hash: &str,
    groups: &[ImageGroup],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL2, "duplicate images with references")?;
    writer.writeln(LEVEL3, &format!("image file hash: {}", group_hash))?;
    writer.writeln_pluralized(groups.len(), Phrase::DuplicateImages)?;
    let total_references: usize = groups
        .iter()
        .map(|g| g.info.markdown_file_references.len())
        .sum();
    let references_string = pluralize(total_references, Phrase::Files);
    writer.writeln(
        "",
        &format!("referenced by {} {}\n", total_references, references_string),
    )?;

    write_group_table(config, writer, groups, true, false)?;
    Ok(())
}

fn write_special_image_group_table(
    config: &ValidatedConfig,
    writer: &OutputFileWriter,
    group_type: &str,
    groups: &[ImageGroup],
    phrase: Phrase,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL2, group_type)?;

    let description = format!("{} {}", groups.len(), pluralize(groups.len(), phrase));
    writer.writeln("", &format!("{}\n", description))?;

    write_group_table(config, writer, groups, false, true)?;
    Ok(())
}

fn write_group_table(
    config: &ValidatedConfig,
    writer: &OutputFileWriter,
    groups: &[ImageGroup],
    is_ref_group: bool,
    is_special_group: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let headers = &["Sample", "Duplicates", "Referenced By"];

    // For special groups like unreferenced images, first group by hash
    let mut hash_groups: HashMap<String, Vec<ImageGroup>> = HashMap::new();
    if is_special_group {
        for group in groups {
            hash_groups
                .entry(group.info.hash.clone())
                .or_default()
                .push(group.clone());
        }
    } else {
        // For regular groups (duplicates), keep as single group
        hash_groups.insert("single".to_string(), groups.to_vec());
    }

    // Create rows for each hash group
    let mut rows = Vec::new();
    for group_vec in hash_groups.values() {
        let keeper_path = if is_ref_group {
            Some(&group_vec[0].path)
        } else {
            None
        };

        let sample = format!(
            "![[{}\\|400]]",
            group_vec[0].path.file_name().unwrap().to_string_lossy()
        );

        let duplicates = format_duplicates(config, group_vec, keeper_path, is_special_group);
        let references = format_references(config, group_vec, keeper_path);

        rows.push(vec![sample, duplicates, references]);
    }

    writer.write_markdown_table(
        headers,
        &rows,
        Some(&[
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]),
    )?;

    writer.writeln("", "")?; // Add an empty line between tables
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
    config: &ValidatedConfig,
    groups: &[ImageGroup],
    keeper_path: Option<&PathBuf>,
) -> String {
    // First, collect all references into a Vec
    let all_references: Vec<(usize, String, &PathBuf)> = groups
        .iter()
        .flat_map(|group| {
            group
                .info
                .markdown_file_references
                .iter()
                .enumerate()
                .map(|(index, ref_path)| (index, ref_path.clone(), &group.path))
                .collect::<Vec<_>>()
        })
        .collect();

    // Then process them
    let processed_refs: Vec<String> = all_references
        .into_iter()
        .map(|(index, ref_path, group_path)| {
            let mut link = format!(
                "{}. {}",
                index + 1,
                format_wikilink(Path::new(&ref_path), config.obsidian_path(), false)
            );
            if config.apply_changes() {
                if let Some(keeper) = keeper_path {
                    if group_path != keeper {
                        link.push_str(" - updated");
                    }
                } else {
                    link.push_str(" - reference removed");
                }
            } else {
                if keeper_path.is_some() {
                    link.push_str(" - would be updated");
                } else {
                    link.push_str(" - reference would be removed");
                }
            }
            link
        })
        .collect();

    processed_refs.join("<br>")
}

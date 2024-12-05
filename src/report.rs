mod duplicate_images_report;
mod frontmatter_issues_report;
mod incompatible_image_report;
mod invalid_wikilink_report;
mod missing_references_report;
mod unreferenced_images_report;

mod report_writer;

pub use report_writer::*;

use crate::constants::*;
use crate::markdown_file_info::{BackPopulateMatch, MarkdownFileInfo, PersistReason};
use crate::obsidian_repository_info::{GroupedImages, ImageGroup, ObsidianRepositoryInfo};
use crate::utils::{escape_brackets, escape_pipe, ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use crate::wikilink::ToWikilink;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
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

        self.write_back_populate_tables(validated_config, &writer, files_to_persist)?;
        self.write_persist_reasons_table(&writer, files_to_persist)?;

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

        self.write_invalid_wikilinks_report(writer)?;

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
                format_wikilink(Path::new(&ref_path), obsidian_path, false)
            );
            if apply_changes {
                if let Some(keeper) = keeper_path {
                    if group_path != keeper {
                        link.push_str(" - updated");
                    }
                } else {
                    link.push_str(" - reference removed");
                }
            } else {
                if keeper_path.is_some() {
                    link.push_str(" - will be updated");
                } else {
                    link.push_str(" - reference will be removed");
                }
            }
            link
        })
        .collect();

    processed_refs.join("<br>")
}

pub fn write_back_populate_table(
    writer: &OutputFileWriter,
    matches: &[BackPopulateMatch],
    is_unambiguous_match: bool,
    wikilinks_count: usize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // description only for unambiguous matches
    if is_unambiguous_match {
        writer.writeln(LEVEL2, MATCHES_UNAMBIGUOUS)?;
        writer.writeln(
            "",
            &format!(
                "{} {} {}",
                BACK_POPULATE_COUNT_PREFIX, wikilinks_count, BACK_POPULATE_COUNT_SUFFIX
            ),
        )?;

        // Count unique files across all matches
        let unique_files: HashSet<String> =
            matches.iter().map(|m| m.relative_path.clone()).collect();

        let header_message = DescriptionBuilder::new()
            .number(matches.len())
            .pluralize(Phrase::Match(matches.len()))
            .text(IN)
            .pluralize_with_count(Phrase::File(unique_files.len()))
            .text(WILL_BE_BACK_POPULATED)
            .build();

        writer.writeln("", header_message.as_str())?;
    }

    // Step 1: Group matches by found_text (case-insensitive) using a HashMap
    let mut matches_by_text: HashMap<String, Vec<&BackPopulateMatch>> = HashMap::new();
    for m in matches {
        let key = m.found_text.to_lowercase();
        matches_by_text.entry(key).or_default().push(m);
    }

    // Step 2: Get display text for each group (use first occurrence's case)
    let mut display_text_map: HashMap<String, String> = HashMap::new();
    for m in matches {
        let key = m.found_text.to_lowercase();
        display_text_map
            .entry(key)
            .or_insert_with(|| m.found_text.clone());
    }

    // Headers for the tables
    let headers: Vec<&str> = if is_unambiguous_match {
        vec![
            "file name",
            "line",
            COL_TEXT,
            COL_OCCURRENCES,
            COL_WILL_REPLACE_WITH,
            COL_SOURCE_TEXT,
        ]
    } else {
        vec!["file name", "line", COL_TEXT, COL_OCCURRENCES]
    };

    // Step 3: Collect and sort the keys
    let mut sorted_found_texts: Vec<String> = matches_by_text.keys().cloned().collect();
    sorted_found_texts.sort();

    // Step 4: Iterate over the sorted keys
    for found_text_key in sorted_found_texts {
        let text_matches = &matches_by_text[&found_text_key];
        let display_text = &display_text_map[&found_text_key];
        let total_occurrences = text_matches.len();
        let file_paths: HashSet<String> = text_matches
            .iter()
            .map(|m| m.relative_path.clone())
            .collect();

        let level_string = if is_unambiguous_match { LEVEL3 } else { LEVEL4 };

        writer.writeln(
            level_string,
            &format!(
                "found: \"{}\" ({})",
                display_text,
                pluralize_occurrence_in_files(total_occurrences, file_paths.len())
            ),
        )?;

        // Sort matches by file path and line number
        let mut sorted_matches = text_matches.to_vec();
        sorted_matches.sort_by(|a, b| {
            let file_a = Path::new(&a.relative_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let file_b = Path::new(&b.relative_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            // First compare by file name (case-insensitive)
            let file_cmp = file_a.to_lowercase().cmp(&file_b.to_lowercase());
            if file_cmp != std::cmp::Ordering::Equal {
                return file_cmp;
            }

            // Then by line number within the same file
            a.line_number.cmp(&b.line_number)
        });

        // Consolidate matches
        let consolidated = consolidate_matches(&sorted_matches);

        // Prepare rows
        let mut table_rows = Vec::new();

        for m in consolidated {
            let file_path = Path::new(&m.file_path);
            let file_stem = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

            // Create a row for each line, maintaining the consolidation of occurrences
            for line_info in m.line_info {
                let highlighted_line = highlight_matches(
                    &line_info.line_text,
                    &line_info.positions,
                    display_text.len(),
                );

                let mut row = vec![
                    file_stem.to_wikilink(),
                    line_info.line_number.to_string(),
                    escape_pipe(&highlighted_line),
                    line_info.positions.len().to_string(),
                ];

                // Only add replacement columns for unambiguous matches
                if is_unambiguous_match {
                    let replacement = if m.in_markdown_table {
                        m.replacement.clone()
                    } else {
                        escape_pipe(&m.replacement)
                    };
                    row.push(replacement.clone());
                    row.push(escape_brackets(&replacement));
                }

                table_rows.push(row);
            }
        }

        // Write the table with appropriate column alignments
        let alignments = if is_unambiguous_match {
            vec![
                ColumnAlignment::Left,
                ColumnAlignment::Right,
                ColumnAlignment::Left,
                ColumnAlignment::Center,
                ColumnAlignment::Left,
                ColumnAlignment::Left,
            ]
        } else {
            vec![
                ColumnAlignment::Left,
                ColumnAlignment::Right,
                ColumnAlignment::Left,
                ColumnAlignment::Center,
            ]
        };

        writer.write_markdown_table(&headers, &table_rows, Some(&alignments))?;
        writer.writeln("", "\n---")?;
    }

    Ok(())
}

fn pluralize_occurrence_in_files(occurrences: usize, file_count: usize) -> String {
    // We want "time" for 1, "times" for other numbers
    let occurrence_word = pluralize(occurrences, PhraseOld::Times);

    // Format as "time(s) in file(s)"
    format!(
        "{} {} in {} {}",
        occurrences,
        occurrence_word,
        file_count,
        pluralize(file_count, PhraseOld::Files)
    )
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

#[derive(Debug, Clone)]
struct ConsolidatedMatch {
    file_path: String,
    line_info: Vec<LineInfo>, // Sorted vector of line information
    replacement: String,
    in_markdown_table: bool,
}

#[derive(Debug, Clone)]
struct LineInfo {
    line_number: usize,
    line_text: String,
    positions: Vec<usize>, // Multiple positions for same line
}

fn consolidate_matches(matches: &[&BackPopulateMatch]) -> Vec<ConsolidatedMatch> {
    // First, group by file path and line number
    let mut line_map: HashMap<(String, usize), LineInfo> = HashMap::new();
    let mut file_info: HashMap<String, (String, bool)> = HashMap::new(); // Tracks replacement and table status per file

    // Group matches by file and line
    for match_info in matches {
        let key = (match_info.relative_path.clone(), match_info.line_number);

        // Update or create line info
        let line_info = line_map.entry(key).or_insert(LineInfo {
            line_number: match_info.line_number + match_info.frontmatter_line_count,
            line_text: match_info.line_text.clone(),
            positions: Vec::new(),
        });
        line_info.positions.push(match_info.position);

        // Track file-level information
        file_info.insert(
            match_info.relative_path.clone(),
            (match_info.replacement.clone(), match_info.in_markdown_table),
        );
    }

    // Convert to consolidated matches, sorting lines within each file
    let mut result = Vec::new();
    for (file_path, (replacement, in_markdown_table)) in file_info {
        let mut file_lines: Vec<LineInfo> = line_map
            .iter()
            .filter(|((path, _), _)| path == &file_path)
            .map(|((_, _), line_info)| line_info.clone())
            .collect();

        // Sort lines by line number
        file_lines.sort_by_key(|line| line.line_number);

        result.push(ConsolidatedMatch {
            file_path,
            line_info: file_lines,
            replacement,
            in_markdown_table,
        });
    }

    // Sort consolidated matches by file path
    result.sort_by(|a, b| {
        let file_a = Path::new(&a.file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let file_b = Path::new(&b.file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        file_a.cmp(file_b)
    });

    result
}

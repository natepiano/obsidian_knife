#[cfg(test)]
mod ambiguous_matches_tests;
#[cfg(test)]
mod file_process_limit_tests;
#[cfg(test)]
mod image_tests;
#[cfg(test)]
mod persist_file_tests;
#[cfg(test)]
mod update_modified_tests;

pub mod obsidian_repository_info_types;

use crate::obsidian_repository_info::obsidian_repository_info_types::{
    GroupedImages, ImageGroup, ImageGroupType, ImageOperation, ImageOperations, ImageReferences,
    MarkdownOperation,
};
use crate::{
    constants::*,
    markdown_file_info::BackPopulateMatch,
    markdown_files::MarkdownFiles,
    utils::{escape_brackets, escape_pipe, ColumnAlignment, ThreadSafeWriter},
    validated_config::ValidatedConfig,
    wikilink::{InvalidWikilinkReason, ToWikilink, Wikilink},
    Timer,
};
use aho_corasick::AhoCorasick;
use itertools::Itertools;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct ObsidianRepositoryInfo {
    pub markdown_files: MarkdownFiles,
    pub image_path_to_references_map: HashMap<PathBuf, ImageReferences>,
    pub other_files: Vec<PathBuf>,
    pub wikilinks_ac: Option<AhoCorasick>,
    pub wikilinks_sorted: Vec<Wikilink>,
}

impl ObsidianRepositoryInfo {
    pub fn identify_ambiguous_matches(&mut self) {
        // Create target and display_text maps as before...
        let mut target_map: HashMap<String, String> = HashMap::new();
        for wikilink in &self.wikilinks_sorted {
            let lower_target = wikilink.target.to_lowercase();
            if !target_map.contains_key(&lower_target)
                || wikilink.target.to_lowercase() == wikilink.target
            {
                target_map.insert(lower_target.clone(), wikilink.target.clone());
            }
        }

        let mut display_text_map: HashMap<String, HashSet<String>> = HashMap::new();
        for wikilink in &self.wikilinks_sorted {
            let lower_display_text = wikilink.display_text.to_lowercase();
            let lower_target = wikilink.target.to_lowercase();
            if let Some(canonical_target) = target_map.get(&lower_target) {
                display_text_map
                    .entry(lower_display_text.clone())
                    .or_default()
                    .insert(canonical_target.clone());
            }
        }

        // Process each file's matches
        for markdown_file in &mut self.markdown_files.iter_mut() {
            // Create a map to group matches by their lowercased found_text within this file
            let mut matches_by_text: HashMap<String, Vec<BackPopulateMatch>> = HashMap::new();

            // Drain matches from the file into our temporary map
            let file_matches = std::mem::take(&mut markdown_file.matches.unambiguous);
            for match_info in file_matches {
                let lower_found_text = match_info.found_text.to_lowercase();
                matches_by_text
                    .entry(lower_found_text)
                    .or_default()
                    .push(match_info);
            }

            // Process each group of matches
            for (found_text_lower, text_matches) in matches_by_text {
                if let Some(targets) = display_text_map.get(&found_text_lower) {
                    if targets.len() > 1 {
                        // This is an ambiguous match
                        // Add to the file's ambiguous collection
                        markdown_file.matches.ambiguous.extend(text_matches.clone());
                    } else {
                        // Unambiguous matches go back into the markdown_file
                        markdown_file.matches.unambiguous.extend(text_matches);
                    }
                } else {
                    // Handle unclassified matches
                    println!(
                        "[WARNING] Found unclassified matches for '{}' in file '{}'",
                        found_text_lower,
                        markdown_file.path.display()
                    );
                    markdown_file.matches.unambiguous.extend(text_matches);
                }
            }
        }
    }

    pub fn find_all_back_populate_matches(&mut self, config: &ValidatedConfig) {
        let _timer = Timer::new("find_all_back_populate_matches");

        let ac = self
            .wikilinks_ac
            .as_ref()
            .expect("Wikilinks AC pattern should be initialized");

        // turn them into references
        let sorted_wikilinks: Vec<&Wikilink> = self.wikilinks_sorted.iter().collect();

        self.markdown_files
            .process_files(config, sorted_wikilinks, ac);
    }

    pub fn apply_back_populate_changes(&mut self) {
        // Only process files that have matches
        for markdown_file in self.markdown_files.iter_mut() {
            if markdown_file.matches.unambiguous.is_empty() {
                continue;
            }

            // Sort matches by line number and position (reverse position for same line)
            let mut sorted_matches = markdown_file.matches.unambiguous.clone();
            sorted_matches.sort_by_key(|m| (m.line_number, std::cmp::Reverse(m.position)));

            let mut updated_content = String::new();
            let mut current_line_num = 1;

            // Process line by line
            for (line_idx, line) in markdown_file.content.lines().enumerate() {
                if current_line_num != line_idx + 1 {
                    updated_content.push_str(line);
                    updated_content.push('\n');
                    continue;
                }

                // Collect matches for the current line
                let line_matches: Vec<&BackPopulateMatch> = sorted_matches
                    .iter()
                    .filter(|m| m.line_number == current_line_num)
                    .collect();

                // Apply matches in reverse order if there are any
                let mut updated_line = line.to_string();
                if !line_matches.is_empty() {
                    updated_line =
                        apply_line_replacements(line, &line_matches, &markdown_file.path);
                }

                updated_content.push_str(&updated_line);
                updated_content.push('\n');
                current_line_num += 1;
            }

            // Final validation check
            if updated_content.contains("[[[")
                || updated_content.contains("]]]")
                || updated_content.matches("[[").count() != updated_content.matches("]]").count()
            {
                eprintln!(
                    "Unintended pattern detected in file '{}'.\nContent has mismatched or unexpected nesting.\nFull content:\n{}",
                    markdown_file.path.display(),
                    updated_content.escape_debug()
                );
                panic!(
                    "Unintended nesting or malformed brackets detected in file '{}'. Please check the content above for any hidden or misplaced patterns.",
                    markdown_file.path.display(),
                );
            }

            // Update the content and mark file as modified
            markdown_file.content = updated_content.trim_end().to_string();
            markdown_file.mark_as_back_populated();
        }
    }

    pub fn persist(
        &mut self,
        config: &ValidatedConfig,
        image_operations: ImageOperations,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.markdown_files
            .persist_all(config.file_process_limit(), image_operations)
    }

    pub fn write_back_populate_tables(
        &self,
        config: &ValidatedConfig,
        writer: &ThreadSafeWriter,
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

    pub fn write_invalid_wikilinks_table(
        &self,
        writer: &ThreadSafeWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Collect all invalid wikilinks from all files
        let invalid_wikilinks = self
            .markdown_files
            .iter()
            .flat_map(|markdown_file_info| {
                markdown_file_info
                    .invalid_wikilinks
                    .iter()
                    .filter(|wikilink| {
                        !matches!(
                            wikilink.reason,
                            InvalidWikilinkReason::EmailAddress
                                | InvalidWikilinkReason::Tag
                                | InvalidWikilinkReason::RawHttpLink
                        )
                    })
                    .map(move |wikilink| (&markdown_file_info.path, wikilink))
            })
            .collect::<Vec<_>>()
            .into_iter()
            .sorted_by(|a, b| {
                let file_a = a.0.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let file_b = b.0.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                file_a
                    .cmp(file_b)
                    .then(a.1.line_number.cmp(&b.1.line_number))
            })
            .collect::<Vec<_>>();

        if invalid_wikilinks.is_empty() {
            return Ok(());
        }

        writer.writeln(LEVEL2, "invalid wikilinks")?;

        // Write header describing the count
        writer.writeln(
            "",
            &format!(
                "found {} invalid wikilinks in {} files\n",
                invalid_wikilinks.len(),
                invalid_wikilinks
                    .iter()
                    .map(|(p, _)| p)
                    .collect::<HashSet<_>>()
                    .len()
            ),
        )?;

        // Prepare headers and alignments for the table
        let headers = vec![
            "file name",
            "line",
            "line text",
            "invalid reason",
            "source text",
        ];

        let alignments = vec![
            ColumnAlignment::Left,
            ColumnAlignment::Right,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ];

        // Prepare rows
        let rows: Vec<Vec<String>> = invalid_wikilinks
            .iter()
            .map(|(file_path, invalid_wikilink)| {
                vec![
                    file_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_wikilink(),
                    invalid_wikilink.line_number.to_string(),
                    escape_pipe(&invalid_wikilink.line),
                    invalid_wikilink.reason.to_string(),
                    escape_brackets(&invalid_wikilink.content),
                ]
            })
            .collect();

        // Write the table
        writer.write_markdown_table(&headers, &rows, Some(&alignments))?;
        writer.writeln("", "\n---\n")?;

        Ok(())
    }

    pub fn write_ambiguous_matches_table(
        &self,
        writer: &ThreadSafeWriter,
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

    pub fn analyze_repository(
        &mut self,
        validated_config: &ValidatedConfig,
    ) -> Result<
        (GroupedImages, Vec<(PathBuf, String)>, ImageOperations),
        Box<dyn Error + Send + Sync>,
    > {
        self.find_all_back_populate_matches(&validated_config);
        self.identify_ambiguous_matches();
        self.apply_back_populate_changes();

        let (grouped_images, missing_references, image_operations) =
            self.analyze_images(&validated_config)?;

        self.process_image_reference_updates(&image_operations);
        Ok((grouped_images, missing_references, image_operations))
    }

    pub(crate) fn analyze_images(
        &self,
        config: &ValidatedConfig,
    ) -> Result<
        (GroupedImages, Vec<(PathBuf, String)>, ImageOperations),
        Box<dyn Error + Send + Sync>,
    > {
        // Get basic analysis
        let grouped_images = group_images(&self.image_path_to_references_map);

        let missing_references = self.generate_missing_references()?;

        // Get files being persisted in this run
        let files_to_persist = self
            .markdown_files
            .get_files_to_persist(config.file_process_limit());
        let files_to_persist: HashSet<_> = files_to_persist.iter().map(|f| &f.path).collect();

        let mut operations = ImageOperations::default();

        // 0. Handle missing references first
        for (markdown_path, missing_image) in &missing_references {
            if files_to_persist.contains(markdown_path) {
                operations
                    .markdown_ops
                    .push(MarkdownOperation::RemoveReference {
                        markdown_path: markdown_path.clone(),
                        image_path: PathBuf::from(missing_image),
                    });
            }
        }

        // 1. Handle unreferenced images - always safe to delete
        if let Some(unreferenced) = grouped_images.get(&ImageGroupType::UnreferencedImage) {
            for group in unreferenced {
                operations
                    .image_ops
                    .push(ImageOperation::Delete(group.path.clone()));
            }
        }

        // 2. Handle zero byte images
        if let Some(zero_byte) = grouped_images.get(&ImageGroupType::ZeroByteImage) {
            process_special_image_group(zero_byte, &files_to_persist, &mut operations);
        }

        // 3. Handle TIFF images - same logic as zero byte
        if let Some(tiff_images) = grouped_images.get(&ImageGroupType::TiffImage) {
            process_special_image_group(tiff_images, &files_to_persist, &mut operations);
        }

        // 4. Handle duplicate groups
        for (_, duplicate_group) in grouped_images.get_duplicate_groups() {
            if let Some(keeper) = duplicate_group.first() {
                for duplicate in duplicate_group.iter().skip(1) {
                    let can_delete = duplicate
                        .info
                        .markdown_file_references
                        .iter()
                        .all(|path| files_to_persist.contains(&PathBuf::from(path)));
                    if can_delete {
                        operations
                            .image_ops
                            .push(ImageOperation::Delete(duplicate.path.clone()));

                        // Add operations to update references to point to keeper
                        for ref_path in &duplicate.info.markdown_file_references {
                            operations
                                .markdown_ops
                                .push(MarkdownOperation::UpdateReference {
                                    markdown_path: PathBuf::from(ref_path),
                                    old_image_path: duplicate.path.clone(),
                                    new_image_path: keeper.path.clone(),
                                });
                        }
                    }
                }
            }
        }

        Ok((grouped_images, missing_references, operations))
    }

    pub(crate) fn write_image_analysis(
        &self,
        config: &ValidatedConfig,
        writer: &ThreadSafeWriter,
        grouped_images: &GroupedImages,
        missing_references: &[(PathBuf, String)],
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
            && missing_references.is_empty()
        {
            return Ok(());
        }

        write_image_tables(
            config,
            writer,
            missing_references,
            tiff_images,
            zero_byte_images,
            unreferenced_images,
            &duplicate_groups,
        )?;

        Ok(())
    }

    fn generate_missing_references(
        &self,
    ) -> Result<Vec<(PathBuf, String)>, Box<dyn Error + Send + Sync>> {
        let mut missing_references = Vec::new();
        let image_filenames: HashSet<String> = self
            .image_path_to_references_map
            .keys()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_lowercase())
            .collect();

        for markdown_file_info in self.markdown_files.iter() {
            for image_link in &markdown_file_info.image_links {
                if !image_exists_in_set(&image_link.filename, &image_filenames) {
                    missing_references
                        .push((markdown_file_info.path.clone(), image_link.filename.clone()));
                }
            }
        }

        Ok(missing_references)
    }

    pub fn process_image_reference_updates(&mut self, operations: &ImageOperations) {
        for op in &operations.markdown_ops {
            match op {
                MarkdownOperation::RemoveReference {
                    markdown_path,
                    image_path,
                } => {
                    // todo - eventually we need to store these changes directly on the MarkdownFileInfo - probably
                    //        with an updated version of BackPopulateMatch that becomes generic as a Replacement or something like that
                    if let Some(markdown_file) = self.markdown_files.get_mut(markdown_path) {
                        let regex = create_file_specific_image_regex(
                            image_path.file_name().unwrap().to_str().unwrap(),
                        );
                        markdown_file.content =
                            process_content(&markdown_file.content, &regex, None);
                        markdown_file.mark_image_reference_as_updated();
                    }
                }
                MarkdownOperation::UpdateReference {
                    markdown_path,
                    old_image_path,
                    new_image_path,
                } => {
                    if let Some(markdown_file) = self.markdown_files.get_mut(markdown_path) {
                        let regex = create_file_specific_image_regex(
                            old_image_path.file_name().unwrap().to_str().unwrap(),
                        );
                        markdown_file.content =
                            process_content(&markdown_file.content, &regex, Some(new_image_path));
                        markdown_file.mark_image_reference_as_updated();
                    }
                }
            }
        }
    }
}

pub fn execute_image_deletions(
    operations: &ImageOperations,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // First execute image deletions
    for op in &operations.image_ops {
        match op {
            ImageOperation::Delete(path) => {
                if let Err(e) = fs::remove_file(path) {
                    eprintln!("Error deleting file {:?}: {}", path, e);
                    return Err(e.into());
                }
            }
        }
    }

    Ok(())
}

fn apply_line_replacements(
    line: &str,
    line_matches: &[&BackPopulateMatch],
    file_path: &PathBuf,
) -> String {
    let mut updated_line = line.to_string();

    // Sort matches in descending order by `position`
    let mut sorted_matches = line_matches.to_vec();
    sorted_matches.sort_by_key(|m| std::cmp::Reverse(m.position));

    // Apply replacements in sorted (reverse) order
    for match_info in sorted_matches {
        let start = match_info.position;
        let end = start + match_info.found_text.len();

        // Check for UTF-8 boundary issues
        if !updated_line.is_char_boundary(start) || !updated_line.is_char_boundary(end) {
            eprintln!(
                "Error: Invalid UTF-8 boundary in file '{:?}', line {}.\n\
                Match position: {} to {}.\nLine content:\n{}\nFound text: '{}'\n",
                file_path, match_info.line_number, start, end, updated_line, match_info.found_text
            );
            panic!("Invalid UTF-8 boundary detected. Check positions and text encoding.");
        }

        // Perform the replacement
        updated_line.replace_range(start..end, &match_info.replacement);

        // Validation check after each replacement
        if updated_line.contains("[[[") || updated_line.contains("]]]") {
            eprintln!(
                "\nWarning: Potential nested pattern detected after replacement in file '{:?}', line {}.\n\
                Current line:\n{}\n",
                file_path, match_info.line_number, updated_line
            );
        }
    }

    updated_line
}

fn process_special_image_group(
    group_images: &[ImageGroup],
    files_to_persist: &HashSet<&PathBuf>,
    operations: &mut ImageOperations,
) {
    for group in group_images {
        if group.info.markdown_file_references.is_empty() {
            operations
                .image_ops
                .push(ImageOperation::Delete(group.path.clone()));
        } else {
            let can_delete = group
                .info
                .markdown_file_references
                .iter()
                .all(|path| files_to_persist.contains(&PathBuf::from(path)));
            if can_delete {
                operations
                    .image_ops
                    .push(ImageOperation::Delete(group.path.clone()));
                // Add operations to remove references
                for ref_path in &group.info.markdown_file_references {
                    operations
                        .markdown_ops
                        .push(MarkdownOperation::RemoveReference {
                            markdown_path: PathBuf::from(ref_path),
                            image_path: group.path.clone(),
                        });
                }
            }
        }
    }
}

pub fn write_back_populate_table(
    writer: &ThreadSafeWriter,
    matches: &[BackPopulateMatch],
    is_unambiguous_match: bool,
    wikilinks_count: usize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if is_unambiguous_match {
        writer.writeln(LEVEL2, MATCHES_UNAMBIGUOUS)?;
        writer.writeln(
            "",
            &format!(
                "{} {} {}",
                BACK_POPULATE_COUNT_PREFIX, wikilinks_count, BACK_POPULATE_COUNT_SUFFIX
            ),
        )?;
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

    if is_unambiguous_match {
        // Count unique files across all matches
        let unique_files: HashSet<String> =
            matches.iter().map(|m| m.relative_path.clone()).collect();
        writer.writeln(
            "",
            &format!(
                "{} {}",
                format_back_populate_header(matches.len(), unique_files.len()),
                BACK_POPULATE_TABLE_HEADER_SUFFIX,
            ),
        )?;
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

fn format_back_populate_header(match_count: usize, file_count: usize) -> String {
    format!(
        "{} {} {} {} {}",
        match_count,
        pluralize(match_count, Phrase::Matches),
        BACK_POPULATE_TABLE_HEADER_MIDDLE,
        file_count,
        pluralize(file_count, Phrase::Files)
    )
}

fn pluralize_occurrence_in_files(occurrences: usize, file_count: usize) -> String {
    // We want "time" for 1, "times" for other numbers
    let occurrence_word = pluralize(occurrences, Phrase::Times);

    // Format as "time(s) in file(s)"
    format!(
        "{} {} in {} {}",
        occurrences,
        occurrence_word,
        file_count,
        pluralize(file_count, Phrase::Files)
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

fn group_images(image_map: &HashMap<PathBuf, ImageReferences>) -> GroupedImages {
    let mut groups = GroupedImages::new();

    for (path_buf, image_references) in image_map {
        let group_type = determine_group_type(path_buf, image_references);
        groups.add_or_update(
            group_type,
            ImageGroup {
                path: path_buf.clone(),
                info: image_references.clone(),
            },
        );
    }

    // Sort groups by path
    for group in groups.groups.values_mut() {
        group.sort_by(|a, b| a.path.cmp(&b.path));
    }

    groups
}

fn write_image_tables(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
    missing_references: &[(PathBuf, String)],
    tiff_images: &[ImageGroup],
    zero_byte_images: &[ImageGroup],
    unreferenced_images: &[ImageGroup],
    duplicate_groups: &[(&String, &Vec<ImageGroup>)],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    write_missing_references_table(config, missing_references, writer)?;

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

fn determine_group_type(path: &Path, info: &ImageReferences) -> ImageGroupType {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .map_or(false, |ext| ext.eq_ignore_ascii_case(TIFF_EXTENSION))
    {
        ImageGroupType::TiffImage
    } else if fs::metadata(path).map(|m| m.len() == 0).unwrap_or(false) {
        ImageGroupType::ZeroByteImage
    } else if info.markdown_file_references.is_empty() {
        ImageGroupType::UnreferencedImage
    } else {
        ImageGroupType::DuplicateGroup(info.hash.clone())
    }
}

fn write_missing_references_table(
    config: &ValidatedConfig,
    missing_references: &[(PathBuf, String)],
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if missing_references.is_empty() {
        return Ok(());
    }

    writer.writeln(LEVEL2, MISSING_IMAGE_REFERENCES)?;
    writer.writeln_pluralized(missing_references.len(), Phrase::MissingImageReferences)?;

    let headers = &["markdown file", "missing image reference", "action"];

    // Group missing references by markdown file
    let mut grouped_references: HashMap<&PathBuf, Vec<ImageGroup>> = HashMap::new();
    for (markdown_path, extracted_filename) in missing_references {
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
    writer: &ThreadSafeWriter,
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

fn image_exists_in_set(image_filename: &str, image_filenames: &HashSet<String>) -> bool {
    image_filenames.contains(&image_filename.to_lowercase())
}

fn write_special_image_group_table(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
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
    writer: &ThreadSafeWriter,
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

fn format_wikilink(path: &Path, obsidian_path: &Path, use_full_filename: bool) -> String {
    let relative_path = path.strip_prefix(obsidian_path).unwrap_or(path);
    let display_name = if use_full_filename {
        path.file_name().unwrap_or_default().to_string_lossy()
    } else {
        path.file_stem().unwrap_or_default().to_string_lossy()
    };
    format!("[[{}\\|{}]]", relative_path.display(), display_name)
}

fn create_file_specific_image_regex(filename: &str) -> Regex {
    Regex::new(&format!(
        r"(!?\[.*?\]\([^)]*{}(?:\|[^)]*)?\)|!\[\[[^]\n]*{}(?:\|[^\]]*?)?\]\])",
        regex::escape(filename),
        regex::escape(filename),
    ))
    .unwrap()
}

fn process_content(content: &str, regex: &Regex, new_path: Option<&Path>) -> String {
    let mut in_frontmatter = false;
    content
        .lines()
        .map(|line| process_line(line, regex, new_path, &mut in_frontmatter))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn process_line(
    line: &str,
    regex: &Regex,
    new_path: Option<&Path>,
    in_frontmatter: &mut bool,
) -> String {
    if line == "---" {
        *in_frontmatter = !*in_frontmatter;
        return line.to_string();
    }
    if *in_frontmatter {
        return line.to_string();
    }

    match new_path {
        Some(new_path) => replace_image_reference(line, regex, new_path),
        None => remove_image_reference(line, regex),
    }
}

fn replace_image_reference(line: &str, regex: &Regex, new_path: &Path) -> String {
    regex
        .replace_all(line, |caps: &regex::Captures| {
            let matched = caps.get(0).unwrap().as_str();
            let relative_path = extract_relative_path(matched);
            let new_name = new_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let new_relative = format!("{}/{}", relative_path, new_name);

            if matched.starts_with(OPENING_IMAGE_WIKILINK_BRACKET) {
                format!("![[{}]]", new_relative)
            } else {
                let alt_text = extract_alt_text(matched);
                format!("![{}]({})", alt_text, new_relative)
            }
        })
        .into_owned()
}

fn remove_image_reference(line: &str, regex: &Regex) -> String {
    let processed = regex.replace_all(line, "");
    let cleaned = processed.trim();

    if should_remove_line(cleaned) {
        String::new()
    } else if regex.find(line).is_none() {
        processed.into_owned()
    } else {
        normalize_spaces(processed.trim())
    }
}

// for deletion, we need the path to the file
fn extract_relative_path(matched: &str) -> String {
    if !matched.contains(FORWARD_SLASH) {
        return DEFAULT_MEDIA_PATH.to_string();
    }

    let old_name = matched.split(FORWARD_SLASH).last().unwrap_or("");
    if let Some(path_start) = matched.find(old_name) {
        let prefix = &matched[..path_start];
        prefix
            .rfind(|c| c == OPENING_PAREN || c == OPENING_BRACKET)
            .map(|pos| &prefix[pos + 1..])
            .map(|p| p.trim_end_matches(FORWARD_SLASH))
            .filter(|p| !p.is_empty())
            .unwrap_or("conf/media")
            .to_string()
    } else {
        DEFAULT_MEDIA_PATH.to_string()
    }
}

fn extract_alt_text(matched: &str) -> &str {
    if matched.starts_with(OPENING_IMAGE_LINK_BRACKET) {
        matched
            .find(CLOSING_BRACKET)
            .map(|alt_end| &matched[2..alt_end])
            .unwrap_or(IMAGE_ALT_TEXT_DEFAULT)
    } else {
        IMAGE_ALT_TEXT_DEFAULT
    }
}

fn should_remove_line(line: &str) -> bool {
    line.is_empty() || line == ":" || line.ends_with(":") || line.ends_with(": ")
}

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

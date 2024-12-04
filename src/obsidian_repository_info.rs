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
    constants::*, markdown_file_info::BackPopulateMatch, markdown_files::MarkdownFiles,
    validated_config::ValidatedConfig, wikilink::Wikilink, Timer,
};
use aho_corasick::AhoCorasick;
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

        let (grouped_images, markdown_references_to_missing_image_files, image_operations) =
            self.analyze_images(&validated_config)?;

        self.process_image_reference_updates(&image_operations);
        Ok((
            grouped_images,
            markdown_references_to_missing_image_files,
            image_operations,
        ))
    }

    fn analyze_images(
        &self,
        config: &ValidatedConfig,
    ) -> Result<
        (GroupedImages, Vec<(PathBuf, String)>, ImageOperations),
        Box<dyn Error + Send + Sync>,
    > {
        // Get basic analysis
        let grouped_images = group_images(&self.image_path_to_references_map);

        let markdown_references_to_missing_image_files =
            self.get_markdown_references_to_missing_image_files()?;

        // Get files being persisted in this run
        let files_to_persist = self
            .markdown_files
            .get_files_to_persist(config.file_process_limit());
        let files_to_persist: HashSet<_> = files_to_persist.iter().map(|f| &f.path).collect();

        let mut operations = ImageOperations::default();

        // 0. Handle missing references first
        for (markdown_path, missing_image) in &markdown_references_to_missing_image_files {
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

        Ok((
            grouped_images,
            markdown_references_to_missing_image_files,
            operations,
        ))
    }

    fn get_markdown_references_to_missing_image_files(
        &self,
    ) -> Result<Vec<(PathBuf, String)>, Box<dyn Error + Send + Sync>> {
        let mut markdown_files_referencing_missing_image_files = Vec::new();
        let image_filenames: HashSet<String> = self
            .image_path_to_references_map
            .keys()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_lowercase())
            .collect();

        for markdown_file_info in self.markdown_files.iter() {
            for image_link in &markdown_file_info.image_links {
                if !image_exists_in_set(&image_link.filename, &image_filenames) {
                    markdown_files_referencing_missing_image_files
                        .push((markdown_file_info.path.clone(), image_link.filename.clone()));
                }
            }
        }

        Ok(markdown_files_referencing_missing_image_files)
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

fn image_exists_in_set(image_filename: &str, image_filenames: &HashSet<String>) -> bool {
    image_filenames.contains(&image_filename.to_lowercase())
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

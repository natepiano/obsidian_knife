#[cfg(test)]
mod ambiguous_matches_tests;
#[cfg(test)]
mod file_process_limit_tests;
#[cfg(test)]
mod image_tests;
#[cfg(test)]
mod obsidian_repository_tests;
#[cfg(test)]
mod persist_file_tests;
#[cfg(test)]
mod scan_tests;
#[cfg(test)]
mod update_modified_tests;

pub mod obsidian_repository_types;

pub use obsidian_repository_types::GroupedImages;
pub use obsidian_repository_types::ImageGroup;

use crate::image_file::{ImageFile, ImageFileState};
use crate::image_files::ImageFiles;
use crate::markdown_file::{
    ImageLink, ImageLinkState, MarkdownFile, MatchType, ReplaceableContent,
};
use crate::obsidian_repository::obsidian_repository_types::{
    ImageGroupType, ImageOperation, ImageOperations, ImageReferences, MarkdownOperation,
};
use crate::utils::collect_repository_files;
use crate::{
    constants::*, markdown_file::BackPopulateMatch, markdown_files::MarkdownFiles,
    validated_config::ValidatedConfig, wikilink::Wikilink, Timer,
};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct ObsidianRepository {
    pub markdown_files: MarkdownFiles,
    pub markdown_files_to_persist: MarkdownFiles,
    pub image_files: ImageFiles,
    pub image_path_to_references_map: HashMap<PathBuf, ImageReferences>,
    #[allow(dead_code)]
    pub other_files: Vec<PathBuf>,
    pub wikilinks_ac: Option<AhoCorasick>,
    pub wikilinks_sorted: Vec<Wikilink>,
}

impl ObsidianRepository {
    pub fn new(config: &ValidatedConfig) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let _timer = Timer::new("obsidian_repository_new");
        let ignore_folders = config.ignore_folders().unwrap_or(&[]);

        let repository_files = collect_repository_files(config, ignore_folders)?;

        // Process markdown files
        let markdown_files = pre_scan_markdown_files(
            &repository_files.markdown_files,
            config.operational_timezone(),
        )?;

        // Process wikilinks
        let all_wikilinks: HashSet<Wikilink> = markdown_files
            .iter()
            .flat_map(|file_info| file_info.wikilinks.valid.clone())
            .collect();

        let (sorted, ac) = sort_and_build_wikilinks_ac(all_wikilinks);

        // Initialize instance with defaults
        let mut repository = Self {
            markdown_files,
            image_files: ImageFiles::new(),
            markdown_files_to_persist: MarkdownFiles::default(),
            image_path_to_references_map: HashMap::new(),
            other_files: repository_files.other_files,
            wikilinks_ac: Some(ac),
            wikilinks_sorted: sorted,
        };

        // Get image map using existing functionality
        repository.image_path_to_references_map = repository
            .markdown_files
            .get_image_info_map(config, &repository_files.image_files)?;

        // Build the new ImageFiles struct from the map data
        // this is new but things may change as we go continue with the refactoring to use image_files
        repository.image_files =
            build_image_files_from_map(&repository.image_path_to_references_map)?;

        // Validate and partition image references into found and missing
        // first get the distinct, lowercase list for comparison
        let image_filenames: HashSet<String> = repository_files
            .image_files
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_lowercase())
            .collect();

        // find the ones where the markdown file has them listed in its image_links
        // these are "found", otherwise they are "missing"
        // missing will get handled by apply_replaceable_matches where it deletes missing references
        // along with updating back populate matches
        for markdown_file in &mut repository.markdown_files {
            let (found, missing): (Vec<ImageLink>, Vec<ImageLink>) = markdown_file
                .image_links
                .found
                .drain(..)
                .partition(|link| image_filenames.contains(&link.filename.to_lowercase()));

            let missing = missing
                .into_iter()
                .map(|mut link| {
                    link.state = ImageLinkState::Missing;
                    link
                })
                .collect();

            markdown_file.image_links.found = found;
            markdown_file.image_links.missing = missing;
        }

        Ok(repository)
    }
}

fn build_image_files_from_map(
    image_map: &HashMap<PathBuf, ImageReferences>,
) -> Result<ImageFiles, Box<dyn Error + Send + Sync>> {
    let mut image_files = ImageFiles::new();

    for (path, image_refs) in image_map {
        let file_info = ImageFile::new(path.clone(), image_refs.hash.clone(), image_refs);

        image_files.push(file_info);
    }

    Ok(image_files)
}

fn sort_and_build_wikilinks_ac(all_wikilinks: HashSet<Wikilink>) -> (Vec<Wikilink>, AhoCorasick) {
    let mut wikilinks: Vec<_> = all_wikilinks.into_iter().collect();
    // uses
    wikilinks.sort_unstable();

    let mut patterns = Vec::with_capacity(wikilinks.len());
    patterns.extend(wikilinks.iter().map(|w| w.display_text.as_str()));

    let ac = AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .match_kind(MatchKind::LeftmostLongest)
        .build(&patterns)
        .expect("Failed to build Aho-Corasick automaton for wikilinks");

    (wikilinks, ac)
}

fn pre_scan_markdown_files(
    markdown_paths: &[PathBuf],
    timezone: &str,
) -> Result<MarkdownFiles, Box<dyn Error + Send + Sync>> {
    // Use Arc<Mutex<...>> for safe shared collection
    let markdown_files = Arc::new(Mutex::new(MarkdownFiles::new()));

    markdown_paths.par_iter().try_for_each(|file_path| {
        match MarkdownFile::new(file_path.clone(), timezone) {
            Ok(file_info) => {
                markdown_files.lock().unwrap().push(file_info);
                Ok(())
            }
            Err(e) => {
                eprintln!("Error processing file {:?}: {}", file_path, e);
                Err(e)
            }
        }
    })?;

    // Extract data from Arc<Mutex<...>>
    let markdown_files = Arc::try_unwrap(markdown_files)
        .unwrap()
        .into_inner()
        .unwrap();

    Ok(markdown_files)
}

impl ObsidianRepository {
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
        for markdown_file in &mut self.markdown_files {
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
            .process_files_for_back_populate_matches(config, sorted_wikilinks, ac);
    }

    pub fn apply_replaceable_matches(&mut self) {
        // Only process files that have matches or missing image references
        for markdown_file in &mut self.markdown_files {
            if markdown_file.matches.unambiguous.is_empty()
                && markdown_file.image_links.missing.is_empty()
            {
                continue;
            }

            let sorted_replaceable_matches = Self::collect_replaceable_matches(markdown_file);

            if sorted_replaceable_matches.is_empty() {
                continue;
            }

            let mut updated_content = String::new();
            let mut content_line_number = 1;
            let mut has_back_populate_changes = false;
            let mut has_image_reference_changes = false;

            // Process line by line
            for (zero_based_idx, line) in markdown_file.content.lines().enumerate() {
                let current_content_line = zero_based_idx + 1;
                let absolute_line_number =
                    current_content_line + markdown_file.frontmatter_line_count;

                if content_line_number != current_content_line {
                    updated_content.push_str(line);
                    updated_content.push('\n');
                    continue;
                }

                // Collect matches for the current line
                let line_matches: Vec<&Box<dyn ReplaceableContent>> = sorted_replaceable_matches
                    .iter()
                    .filter(|m| m.line_number() == absolute_line_number)
                    .collect();

                // Apply matches if there are any
                let mut updated_line = line.to_string();
                if !line_matches.is_empty() {
                    updated_line =
                        apply_line_replacements(line, &line_matches, &markdown_file.path);
                    // Track which types of changes occurred
                    for m in &line_matches {
                        match m.as_ref().match_type() {
                            MatchType::BackPopulate => has_back_populate_changes = true,
                            MatchType::ImageReference => has_image_reference_changes = true,
                        }
                    }
                }

                updated_content.push_str(&updated_line);
                updated_content.push('\n');
                content_line_number += 1;
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

            if has_back_populate_changes {
                markdown_file.mark_as_back_populated();
            }
            if has_image_reference_changes {
                markdown_file.mark_image_reference_as_updated();
            }
        }
    }

    fn collect_replaceable_matches(
        markdown_file: &MarkdownFile,
    ) -> Vec<Box<dyn ReplaceableContent>> {
        let mut matches = Vec::new();

        // Add BackPopulateMatches
        matches.extend(
            markdown_file
                .matches
                .unambiguous
                .iter()
                .cloned()
                .map(|m| Box::new(m) as Box<dyn ReplaceableContent>),
        );

        // Add ImageLinks.missing
        matches.extend(
            markdown_file
                .image_links
                .missing
                .iter()
                .cloned()
                .map(|m| Box::new(m) as Box<dyn ReplaceableContent>),
        );

        // Add ImageLinks.missing
        matches.extend(
            markdown_file
                .image_links
                .found
                .iter()
                .filter(|image_link| {
                    matches!(image_link.state, ImageLinkState::Incompatible { .. })
                })
                .cloned()
                .map(|m| Box::new(m) as Box<dyn ReplaceableContent>),
        );

        // Sort by line number and reverse position
        matches.sort_by_key(|m| (m.line_number(), std::cmp::Reverse(m.position())));

        matches
    }

    pub fn persist(
        &mut self,
        image_operations: ImageOperations,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.markdown_files_to_persist.persist_all(image_operations)
    }

    pub fn analyze_repository(
        &mut self,
        validated_config: &ValidatedConfig,
    ) -> Result<(GroupedImages, ImageOperations), Box<dyn Error + Send + Sync>> {
        self.find_all_back_populate_matches(validated_config);
        self.identify_ambiguous_matches();
        self.identify_image_reference_replacements();
        self.apply_replaceable_matches();

        // after checking for all back populate matches and references to nonexistent files
        // and then applying replacement matches,
        // mark either all files - or the file_process_limit count files - as to be persisted
        self.populate_files_to_persist(validated_config.file_process_limit());

        // after populating files to persist, we can use this dataset to determine whether
        // an image can be deleted - if it's referenced in a file that won't be persisted
        // then we won't delete it in this pass
        let (grouped_images, image_operations) = self.analyze_images()?;

        self.process_image_reference_updates(&image_operations);

        Ok((grouped_images, image_operations))
    }

    fn populate_files_to_persist(&mut self, file_limit: Option<usize>) {
        let files_to_persist: Vec<MarkdownFile> = self
            .markdown_files
            .iter()
            .filter(|file_info| {
                file_info
                    .frontmatter
                    .as_ref()
                    .map_or(false, |fm| fm.needs_persist())
            })
            .cloned()
            .collect();

        let total_files = files_to_persist.len();
        let count = file_limit.unwrap_or(total_files);

        self.markdown_files_to_persist = MarkdownFiles {
            files: files_to_persist.into_iter().take(count).collect(),
        };
    }

    fn identify_image_reference_replacements(&mut self) {
        let incompatible = self
            .image_files
            .files_in_state(|state| matches!(state, ImageFileState::Incompatible { .. }));

        for image_file in incompatible.files {
            if let ImageFileState::Incompatible { reason } = &image_file.image_state {
                for markdown_file in &mut self.markdown_files.files {
                    if let Some(image_link) =
                        markdown_file.image_links.found.iter_mut().find(|link| {
                            link.filename == image_file.path.file_name().unwrap().to_str().unwrap()
                        })
                    {
                        image_link.state = ImageLinkState::Incompatible {
                            reason: reason.clone(),
                        };
                    }
                }
            }
        }

        let grouped_images = group_images(&self.image_path_to_references_map);

        // Handle duplicate groups
        for (_, duplicate_group) in grouped_images.get_duplicate_groups() {
            if let Some(keeper) = duplicate_group.first() {
                for duplicate in duplicate_group.iter().skip(1) {
                    // Update ImageLink states in files to be persisted
                    for markdown_file in &mut self.markdown_files.files {
                        if let Some(image_link) =
                            markdown_file.image_links.found.iter_mut().find(|link| {
                                link.filename
                                    == duplicate.path.file_name().unwrap().to_str().unwrap()
                            })
                        {
                            image_link.state = ImageLinkState::Duplicate {
                                keeper_path: keeper.path.clone(),
                            };
                        }
                    }
                }
            }
        }
    }

    fn analyze_images(
        &self,
    ) -> Result<(GroupedImages, ImageOperations), Box<dyn Error + Send + Sync>> {
        // Get basic analysis
        let grouped_images = group_images(&self.image_path_to_references_map);

        let files_to_persist: HashSet<_> = self
            .markdown_files_to_persist
            .iter()
            .map(|f| &f.path)
            .collect();

        let mut operations = ImageOperations::default();

        // uses new ImageFiles / ImageFile approach
        for unreferenced_image_file in self
            .image_files
            .files_in_state(|state| matches!(state, ImageFileState::Unreferenced))
        {
            operations
                .image_ops
                .push(ImageOperation::Delete(unreferenced_image_file.path.clone()))
        }

        // 2. Handle zero byte images
        if let Some(zero_byte) = grouped_images.get(&ImageGroupType::ZeroByteImage) {
            process_zero_byte_and_tiff_images(zero_byte, &files_to_persist, &mut operations);
        }

        // 3. Handle TIFF images - same logic as zero byte
        if let Some(tiff_images) = grouped_images.get(&ImageGroupType::TiffImage) {
            process_zero_byte_and_tiff_images(tiff_images, &files_to_persist, &mut operations);
        }

        // 4. Handle duplicate groups
        for (_, duplicate_group) in grouped_images.get_duplicate_groups() {
            if let Some(keeper) = duplicate_group.first() {
                for duplicate in duplicate_group.iter().skip(1) {
                    let can_delete = duplicate
                        .image_references
                        .markdown_file_references
                        .iter()
                        .all(|path| files_to_persist.contains(&PathBuf::from(path)));
                    if can_delete {
                        operations
                            .image_ops
                            .push(ImageOperation::Delete(duplicate.path.clone()));

                        // Add operations to update references to point to keeper
                        for ref_path in &duplicate.image_references.markdown_file_references {
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

        Ok((grouped_images, operations))
    }

    // todo - eventually we need to store these changes directly on the MarkdownFile - probably
    //        with an updated version of BackPopulateMatch that becomes generic as a Replacement or something like that
    pub fn process_image_reference_updates(&mut self, operations: &ImageOperations) {
        for op in &operations.markdown_ops {
            match op {
                MarkdownOperation::RemoveReference {
                    markdown_path,
                    image_path,
                } => {
                    if let Some(markdown_file) =
                        self.markdown_files_to_persist.get_mut(markdown_path)
                    {
                        let regex = create_file_specific_image_regex(
                            image_path.file_name().unwrap().to_str().unwrap(),
                        );
                        markdown_file.content = process_content_for_image_reference_updates(
                            &markdown_file.content,
                            &regex,
                            None,
                        );
                        markdown_file.mark_image_reference_as_updated();
                    }
                }
                MarkdownOperation::UpdateReference {
                    markdown_path,
                    old_image_path,
                    new_image_path,
                } => {
                    if let Some(markdown_file) =
                        self.markdown_files_to_persist.get_mut(markdown_path)
                    {
                        let regex = create_file_specific_image_regex(
                            old_image_path.file_name().unwrap().to_str().unwrap(),
                        );
                        markdown_file.content = process_content_for_image_reference_updates(
                            &markdown_file.content,
                            &regex,
                            Some(new_image_path),
                        );
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
    line_matches: &[&Box<dyn ReplaceableContent>],
    file_path: &PathBuf,
) -> String {
    let mut updated_line = line.to_string();
    let mut has_image_replacement = false;

    // Sort matches in descending order by `position`
    let mut sorted_matches = line_matches.to_vec();
    sorted_matches.sort_by_key(|m| std::cmp::Reverse(m.position()));

    // Apply replacements in sorted (reverse) order
    for match_info in sorted_matches {
        let start = match_info.position();
        let end = start + match_info.matched_text().len();

        // Check for UTF-8 boundary issues
        if !updated_line.is_char_boundary(start) || !updated_line.is_char_boundary(end) {
            eprintln!(
                "Error: Invalid UTF-8 boundary in file '{:?}', line {}.\n\
                Match position: {} to {}.\nLine content:\n{}\nFound text: '{}'\n",
                file_path,
                match_info.line_number(),
                start,
                end,
                updated_line,
                match_info.matched_text()
            );
            panic!("Invalid UTF-8 boundary detected. Check positions and text encoding.");
        }

        // Track if this is an image replacement
        if match_info.as_ref().match_type() == MatchType::ImageReference {
            has_image_replacement = true;
        }

        // Perform the replacement
        updated_line.replace_range(start..end, &match_info.get_replacement());

        // Validation check after each replacement
        if updated_line.contains("[[[") || updated_line.contains("]]]") {
            eprintln!(
                "\nWarning: Potential nested pattern detected after replacement in file '{:?}', line {}.\n\
                Current line:\n{}\n",
                file_path, match_info.line_number(), updated_line
            );
        }
    }

    // If we had any image replacements, clean up the line
    if has_image_replacement {
        let trimmed = updated_line.trim();
        if trimmed.is_empty() {
            String::new()
        } else {
            normalize_spaces(trimmed)
        }
    } else {
        updated_line
    }
}

fn process_zero_byte_and_tiff_images(
    group_images: &[ImageGroup],
    files_to_persist: &HashSet<&PathBuf>,
    operations: &mut ImageOperations,
) {
    for group in group_images {
        if group.image_references.markdown_file_references.is_empty() {
            operations
                .image_ops
                .push(ImageOperation::Delete(group.path.clone()));
        } else {
            let can_delete = group
                .image_references
                .markdown_file_references
                .iter()
                .all(|path| files_to_persist.contains(&PathBuf::from(path)));
            if can_delete {
                operations
                    .image_ops
                    .push(ImageOperation::Delete(group.path.clone()));
                // Add operations to remove references
                for ref_path in &group.image_references.markdown_file_references {
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
        let group_type = determine_image_group_type(path_buf, image_references);
        groups.add_or_update(
            group_type,
            ImageGroup {
                path: path_buf.clone(),
                image_references: image_references.clone(),
            },
        );
    }

    // Sort groups by path
    for group in groups.groups.values_mut() {
        group.sort_by(|a, b| a.path.cmp(&b.path));
    }

    groups
}

fn determine_image_group_type(path: &Path, info: &ImageReferences) -> ImageGroupType {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .map_or(false, |ext| ext.eq_ignore_ascii_case(EXTENSION_TIFF))
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

fn create_file_specific_image_regex(filename: &str) -> Regex {
    Regex::new(&format!(
        r"(!?\[.*?\]\([^)]*{}(?:\|[^)]*)?\)|!\[\[[^]\n]*{}(?:\|[^\]]*?)?\]\])",
        regex::escape(filename),
        regex::escape(filename),
    ))
    .unwrap()
}

fn process_content_for_image_reference_updates(
    content: &str,
    regex: &Regex,
    new_path: Option<&Path>,
) -> String {
    let mut in_frontmatter = false;
    content
        .lines()
        .map(|line| {
            process_line_for_image_reference_updates(line, regex, new_path, &mut in_frontmatter)
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn process_line_for_image_reference_updates(
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
pub fn extract_relative_path(matched: &str) -> String {
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
    line.is_empty()
}

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

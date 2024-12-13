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

use crate::{
    constants::*,
    image_file::{ImageFile, ImageFileState, ImageFiles},
    markdown_file::BackPopulateMatch,
    markdown_file::{ImageLinkState, MarkdownFile, MatchType, ReplaceableContent},
    markdown_files::MarkdownFiles,
    obsidian_repository::obsidian_repository_types::{
        GroupedImages, ImageGroup, ImageGroupType, ImageOperation, ImageOperations, ImageReferences,
    },
    utils,
    utils::VecEnumFilter,
    validated_config::ValidatedConfig,
    wikilink::Wikilink,
    Timer,
};

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
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
        let _timer = Timer::new("ObsidianRepository::new");
        let ignore_folders = config.ignore_folders().unwrap_or(&[]);

        let repository_files = utils::collect_repository_files(config, ignore_folders)?;

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
            image_files: ImageFiles::default(),
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

        Ok(repository)
    }
}

fn build_image_files_from_map(
    image_map: &HashMap<PathBuf, ImageReferences>,
) -> Result<ImageFiles, Box<dyn Error + Send + Sync>> {
    // Count occurrences of each hash so we know how many duplicates there are
    // we can create ImageFiles with this reference count new can classify it accordingly
    let hash_counts: HashMap<String, usize> =
        image_map
            .values()
            .map(|refs| &refs.hash)
            .fold(HashMap::new(), |mut acc, hash| {
                *acc.entry(hash.clone()).or_insert(0) += 1;
                acc
            });

    image_map
        .iter()
        .map(|(path, image_refs)| {
            let duplicate_reference_count = hash_counts.get(&image_refs.hash).copied().unwrap_or(0);
            let file_info = ImageFile::new(
                path.clone(),
                image_refs.hash.clone(),
                image_refs,
                duplicate_reference_count,
            );
            Ok(file_info)
        })
        .collect()
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
    let markdown_files = Arc::new(Mutex::new(MarkdownFiles::default()));

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
        for markdown_file in &mut self.markdown_files {
            let has_replaceable_image_links = markdown_file.image_links.links.iter().any(|link| {
                matches!(
                    link.state,
                    ImageLinkState::Missing
                        | ImageLinkState::Duplicate { .. }
                        | ImageLinkState::Incompatible { .. }
                )
            });

            if markdown_file.matches.unambiguous.is_empty() && !has_replaceable_image_links {
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

                if !line_matches.is_empty() {
                    let updated_line =
                        apply_line_replacements(line, &line_matches, &markdown_file.path);

                    // Track which types of changes occurred
                    for m in &line_matches {
                        match m.as_ref().match_type() {
                            MatchType::BackPopulate => has_back_populate_changes = true,
                            MatchType::ImageReference => has_image_reference_changes = true,
                        }
                    }

                    if !updated_line.is_empty() {
                        updated_content.push_str(&updated_line);
                        updated_content.push('\n');
                    }
                } else {
                    updated_content.push_str(line);
                    updated_content.push('\n');
                }
                content_line_number += 1;
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

        // Add the image link states that need replacement
        matches.extend(
            markdown_file
                .image_links
                .filter_by_predicate(|state| {
                    matches!(
                        state,
                        ImageLinkState::Incompatible { .. }
                            | ImageLinkState::Duplicate { .. }
                            | ImageLinkState::Missing
                    )
                })
                .iter()
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
    ) -> Result<ImageOperations, Box<dyn Error + Send + Sync>> {
        let _timer = Timer::new("ObsidianRepository::analyze");

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
        let image_operations = self.analyze_images()?;

        Ok(image_operations)
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
        // first handle missing references
        let image_filenames: HashSet<String> = self
            .image_files
            .iter()
            .filter_map(|image_file| image_file.path.file_name())
            .map(|name| name.to_string_lossy().to_lowercase())
            .collect();

        for markdown_file in &mut self.markdown_files {
            for link in markdown_file.image_links.links.iter_mut() {
                if !image_filenames.contains(&link.filename.to_lowercase()) {
                    link.state = ImageLinkState::Missing;
                }
            }
        }

        // next handle incompatible image references
        let incompatible = self.image_files.filter_by_predicate(|image_file_state| {
            matches!(image_file_state, ImageFileState::Incompatible { .. })
        });

        // match tiff/zero_byte image files to image_links that refer to them so we can mark the image_link as incompatible
        // the image_link will then be collected as a ReplaceableContent match which happens in the next step
        for image_file in incompatible.files {
            if let ImageFileState::Incompatible { reason } = &image_file.image_state {
                let image_file_name = image_file.path.file_name().unwrap().to_str().unwrap();
                for markdown_file in &mut self.markdown_files {
                    if let Some(image_link) = markdown_file
                        .image_links
                        .links
                        .iter_mut()
                        .find(|link| link.filename == image_file_name)
                    {
                        image_link.state = ImageLinkState::Incompatible {
                            reason: reason.clone(),
                        };
                    }
                }
            }
        }

        // last handle duplicate groups
        let grouped_images = group_images(&self.image_path_to_references_map);

        // Handle duplicate groups
        for (_, duplicate_group) in grouped_images.get_duplicate_groups() {
            if let Some(keeper) = duplicate_group.first() {
                for duplicate in duplicate_group.iter().skip(1) {
                    let duplicate_file_name = duplicate.path.file_name().unwrap().to_str().unwrap();
                    // Update ImageLink states in files to be persisted
                    for markdown_file in &mut self.markdown_files {
                        if let Some(image_link) = markdown_file
                            .image_links
                            .iter_mut()
                            .find(|link| link.filename == duplicate_file_name)
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

    fn analyze_images(&self) -> Result<ImageOperations, Box<dyn Error + Send + Sync>> {
        // Get basic analysis
        let grouped_images = group_images(&self.image_path_to_references_map);

        let files_to_persist: HashSet<_> = self
            .markdown_files_to_persist
            .iter()
            .map(|f| &f.path)
            .collect();

        let mut operations = ImageOperations::default();

        // 1. uses new ImageFiles / ImageFile approach for unreferenced images
        for unreferenced_image_file in self
            .image_files
            .filter_by_variant(ImageFileState::Unreferenced)
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
            if duplicate_group.first().is_some() {
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
                    }
                }
            }
        }

        Ok(operations)
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

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

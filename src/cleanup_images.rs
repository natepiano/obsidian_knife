#[cfg(test)]
mod cleanup_image_tests;

use crate::config::ValidatedConfig;
use crate::constants::*;
use crate::file_utils::update_file;
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::scan::ImageInfo;
use crate::utils::{ColumnAlignment, ThreadSafeWriter};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

// represent different types of image groups
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ImageGroupType {
    TiffImage,
    ZeroByteImage,
    UnreferencedImage,
    DuplicateGroup(String), // String is the hash value
}

#[derive(Clone)]
struct ImageGroup {
    path: PathBuf,
    info: ImageInfo,
}

// New struct to hold grouped images
#[derive(Default)]
struct GroupedImages {
    groups: HashMap<ImageGroupType, Vec<ImageGroup>>,
}

impl GroupedImages {
    fn new() -> Self {
        Self {
            groups: HashMap::new(),
        }
    }

    fn add_or_update(&mut self, group_type: ImageGroupType, image: ImageGroup) {
        self.groups.entry(group_type).or_default().push(image);
    }

    fn get(&self, group_type: &ImageGroupType) -> Option<&Vec<ImageGroup>> {
        self.groups.get(group_type)
    }

    fn get_duplicate_groups(&self) -> Vec<(&String, &Vec<ImageGroup>)> {
        self.groups
            .iter()
            .filter_map(|(key, group)| match key {
                ImageGroupType::DuplicateGroup(hash) if group.len() > 1 => Some((hash, group)),
                _ => None,
            })
            .collect()
    }
}

pub fn cleanup_images(
    config: &ValidatedConfig,
    obsidian_repository_info: &mut ObsidianRepositoryInfo,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL1, SECTION_IMAGE_CLEANUP)?;

    let mut modified_paths = HashSet::new(); // Add HashSet to track modified files

    let grouped_images = group_images(&obsidian_repository_info.image_map);
    let missing_references = generate_missing_references(obsidian_repository_info)?;

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

    write_tables(
        config,
        writer,
        &missing_references,
        tiff_images,
        zero_byte_images,
        unreferenced_images,
        &duplicate_groups,
        &mut modified_paths, // Pass modified_paths to write_tables
    )?;

    if !modified_paths.is_empty() {
        let paths: Vec<PathBuf> = modified_paths.into_iter().collect();
        obsidian_repository_info.update_modified_dates(&paths);
    }

    Ok(())
}

fn write_tables(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
    missing_references: &[(&PathBuf, String)],
    tiff_images: &[ImageGroup],
    zero_byte_images: &[ImageGroup],
    unreferenced_images: &[ImageGroup],
    duplicate_groups: &[(&String, &Vec<ImageGroup>)],
    modified_paths: &mut HashSet<PathBuf>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    write_missing_references_table(config, missing_references, writer, modified_paths)?;

    if !tiff_images.is_empty() {
        write_special_group_table(
            config,
            writer,
            TIFF_IMAGES,
            tiff_images,
            Phrase::TiffImages,
            modified_paths,
        )?;
    }

    if !zero_byte_images.is_empty() {
        write_special_group_table(
            config,
            writer,
            ZERO_BYTE_IMAGES,
            zero_byte_images,
            Phrase::ZeroByteImages,
            modified_paths,
        )?;
    }

    if !unreferenced_images.is_empty() {
        write_special_group_table(
            config,
            writer,
            UNREFERENCED_IMAGES,
            unreferenced_images,
            Phrase::UnreferencedImages,
            modified_paths,
        )?;
    }

    for (hash, group) in duplicate_groups {
        write_duplicate_group_table(config, writer, hash, group, modified_paths)?;
    }

    Ok(())
}

fn group_images(image_map: &HashMap<PathBuf, ImageInfo>) -> GroupedImages {
    let mut groups = GroupedImages::new();

    for (path, info) in image_map {
        let group_type = determine_group_type(path, info);
        groups.add_or_update(
            group_type,
            ImageGroup {
                path: path.clone(),
                info: info.clone(),
            },
        );
    }

    // Sort groups by path
    for group in groups.groups.values_mut() {
        group.sort_by(|a, b| a.path.cmp(&b.path));
    }

    groups
}

fn determine_group_type(path: &Path, info: &ImageInfo) -> ImageGroupType {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .map_or(false, |ext| ext.eq_ignore_ascii_case(TIFF_EXTENSION))
    {
        ImageGroupType::TiffImage
    } else if fs::metadata(path).map(|m| m.len() == 0).unwrap_or(false) {
        ImageGroupType::ZeroByteImage
    } else if info.references.is_empty() {
        ImageGroupType::UnreferencedImage
    } else {
        ImageGroupType::DuplicateGroup(info.hash.clone())
    }
}

fn generate_missing_references(
    collected_files: &ObsidianRepositoryInfo,
) -> Result<Vec<(&PathBuf, String)>, Box<dyn Error + Send + Sync>> {
    let mut missing_references = Vec::new();
    let image_filenames: HashSet<String> = collected_files
        .image_map
        .keys()
        .filter_map(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_lowercase())
        .collect();

    for file_info in &collected_files.markdown_files {
        for image_link in &file_info.image_links {
            if let Some(extracted_filename) = extract_local_image_filename(image_link) {
                if !image_exists_in_set(&extracted_filename, &image_filenames) {
                    missing_references.push((&file_info.path, extracted_filename));
                }
            }
        }
    }

    Ok(missing_references)
}

fn write_missing_references_table(
    config: &ValidatedConfig,
    missing_references: &[(&PathBuf, String)],
    writer: &ThreadSafeWriter,
    modified_paths: &mut HashSet<PathBuf>, // Add modified_paths parameter
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
                info: ImageInfo {
                    hash: String::new(),
                    references: vec![markdown_path.to_string_lossy().to_string()],
                },
            });
    }

    let rows: Vec<Vec<String>> = grouped_references
        .iter()
        .map(|(markdown_path, image_groups)| {
            let markdown_link = format_wikilink(markdown_path, config.obsidian_path(), false);
            let image_links = image_groups
                .iter()
                .map(|group| group.path.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let actions = format_references(config, image_groups, None, modified_paths);
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

fn extract_local_image_filename(image_link: &str) -> Option<String> {
    // Handle Obsidian-style links (always local)
    if image_link.starts_with(OPENING_IMAGE_WIKILINK_BRACKET)
        && image_link.ends_with(CLOSING_WIKILINK)
    {
        let inner = &image_link[3..image_link.len() - 2];
        let filename = inner.split('|').next().unwrap_or(inner).trim();
        Some(filename.to_lowercase())
    }
    // Handle Markdown-style links (check if local)
    else if image_link.starts_with("![") && image_link.contains("](") && image_link.ends_with(")")
    {
        let start = image_link.find("](").map(|i| i + 2)?;
        let end = image_link.len() - 1;
        let url = &image_link[start..end];

        // Check if the URL is local (doesn't start with http:// or https://)
        if !url.starts_with("https://") && !url.starts_with("https://") {
            url.rsplit(FORWARD_SLASH).next().map(|s| s.to_lowercase())
        } else {
            None
        }
    }
    // If it's not a recognized image link format, return None
    else {
        None
    }
}

fn image_exists_in_set(image_filename: &str, image_filenames: &HashSet<String>) -> bool {
    image_filenames.contains(&image_filename.to_lowercase())
}

fn write_special_group_table(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
    group_type: &str,
    groups: &[ImageGroup],
    phrase: Phrase,
    modified_paths: &mut HashSet<PathBuf>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL2, group_type)?;

    let description = format!("{} {}", groups.len(), pluralize(groups.len(), phrase));
    writer.writeln("", &format!("{}\n", description))?;

    write_group_table(config, writer, groups, false, true, modified_paths)?;
    Ok(())
}

fn write_duplicate_group_table(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
    group_hash: &str,
    groups: &[ImageGroup],
    modified_paths: &mut HashSet<PathBuf>, // Add modified_paths parameter
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL2, "duplicate images with references")?;
    writer.writeln(LEVEL3, &format!("image file hash: {}", group_hash))?;
    writer.writeln_pluralized(groups.len(), Phrase::DuplicateImages)?;
    let total_references: usize = groups.iter().map(|g| g.info.references.len()).sum();
    let references_string = pluralize(total_references, Phrase::Files);
    writer.writeln(
        "",
        &format!("referenced by {} {}\n", total_references, references_string),
    )?;

    write_group_table(config, writer, groups, true, false, modified_paths)?;
    Ok(())
}

fn write_group_table(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
    groups: &[ImageGroup],
    is_ref_group: bool,
    is_special_group: bool,
    modified_paths: &mut HashSet<PathBuf>, // Add modified_paths parameter
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let headers = &["Sample", "Duplicates", "Referenced By"];
    let keeper_path = if is_ref_group {
        Some(&groups[0].path)
    } else {
        None
    };

    let sample = format!(
        "![[{}\\|400]]",
        groups[0].path.file_name().unwrap().to_string_lossy()
    );

    let duplicates = format_duplicates(config, groups, keeper_path, is_special_group);
    let references = format_references(config, groups, keeper_path, modified_paths);

    let rows = vec![vec![sample, duplicates, references]];

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
                    if let Err(e) = handle_file_operation(&group.path, FileOperation::Delete) {
                        eprintln!("Error deleting file {:?}: {}", group.path, e);
                    }
                } else {
                    // For duplicate groups
                    if let Some(keeper) = keeper_path {
                        if &group.path == keeper {
                            link.push_str(" - kept");
                        } else {
                            link.push_str(" - deleted");
                            if let Err(e) =
                                handle_file_operation(&group.path, FileOperation::Delete)
                            {
                                eprintln!("Error deleting file {:?}: {}", group.path, e);
                            }
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
    modified_paths: &mut HashSet<PathBuf>,
) -> String {
    // First, collect all references into a Vec
    let all_references: Vec<(usize, String, &PathBuf)> = groups
        .iter()
        .flat_map(|group| {
            group
                .info
                .references
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
                modified_paths.insert(PathBuf::from(&ref_path));

                if let Some(keeper) = keeper_path {
                    if group_path != keeper {
                        link.push_str(" - updated");
                        if let Err(e) = handle_file_operation(
                            Path::new(&ref_path),
                            FileOperation::UpdateReference(group_path.clone(), keeper.clone()),
                        ) {
                            eprintln!("Error updating reference in {:?}: {}", ref_path, e);
                        }
                    }
                } else {
                    link.push_str(" - reference removed");
                    let remove_path = group_path
                        .file_name()
                        .unwrap_or_default()
                        .to_str()
                        .unwrap_or_default();
                    if let Err(e) = handle_file_operation(
                        Path::new(&ref_path),
                        FileOperation::RemoveReference(PathBuf::from(remove_path)),
                    ) {
                        eprintln!("Error removing reference in {:?}: {}", ref_path, e);
                    }
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

fn format_wikilink(path: &Path, obsidian_path: &Path, use_full_filename: bool) -> String {
    let relative_path = path.strip_prefix(obsidian_path).unwrap_or(path);
    let display_name = if use_full_filename {
        path.file_name().unwrap_or_default().to_string_lossy()
    } else {
        path.file_stem().unwrap_or_default().to_string_lossy()
    };
    format!("[[{}\\|{}]]", relative_path.display(), display_name)
}

#[derive(Debug)]
enum FileOperation {
    Delete,
    RemoveReference(PathBuf),
    UpdateReference(PathBuf, PathBuf), // (old_path, new_path)
}

fn handle_file_operation(
    path: &Path,
    operation: FileOperation,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Check if the path is a wikilink
    if path.to_str().map_or(false, |s| {
        s.contains(OPENING_WIKILINK) && s.contains(CLOSING_WIKILINK)
    }) {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Wikilink paths are not allowed: {:?}", path),
        )));
    }
    match operation {
        FileOperation::Delete => {
            fs::remove_file(path)?;
        }
        FileOperation::RemoveReference(ref old_path) => {
            update_file_content(path, old_path, None)?;
        }
        FileOperation::UpdateReference(ref old_path, ref new_path) => {
            update_file_content(path, old_path, Some(new_path))?;
        }
    }

    Ok(())
}

fn update_file_content(
    file_path: &Path,
    old_path: &Path,
    new_path: Option<&Path>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    update_file(file_path, |content| {
        let old_name = old_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        let regex = create_image_regex(old_name);
        process_content(content, &regex, new_path)
    })
}

fn create_image_regex(filename: &str) -> Regex {
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

use crate::constants::*;
use crate::file_utils::update_file;
use crate::scan::{ImageInfo, ObsidianRepositoryInfo};
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::validated_config::ValidatedConfig;
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
    collected_files: &ObsidianRepositoryInfo,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL1, SECTION_IMAGE_CLEANUP)?;

    let grouped_images = group_images(&collected_files.image_map);
    let missing_references = generate_missing_references(collected_files)?;

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
    )?;

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
) -> Result<(), Box<dyn Error + Send + Sync>> {
    write_missing_references_table(config, missing_references, writer)?;

    if !tiff_images.is_empty() {
        write_special_group_table(config, writer, TIFF_IMAGES, tiff_images, Phrase::TiffImages)?;
    }

    if !zero_byte_images.is_empty() {
        write_special_group_table(
            config,
            writer,
            ZERO_BYTE_IMAGES,
            zero_byte_images,
            Phrase::ZeroByteImages,
        )?;
    }

    if !unreferenced_images.is_empty() {
        write_special_group_table(
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
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL2, group_type)?;

    let description = format!("{} {}", groups.len(), pluralize(groups.len(), phrase));
    writer.writeln("", &format!("{}\n", description))?;

    write_group_table(config, writer, groups, false, true)?;
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
    let total_references: usize = groups.iter().map(|g| g.info.references.len()).sum();
    let references_string = pluralize(total_references, Phrase::Files);
    writer.writeln(
        "",
        &format!("referenced by {} {}\n", total_references, references_string),
    )?;

    write_group_table(config, writer, groups, true, false)?;
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
    let references = format_references(config, groups, keeper_path);

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
) -> String {
    groups
        .iter()
        .flat_map(|group| {
            group
                .info
                .references
                .iter()
                .enumerate()
                .map(move |(index, ref_path)| {
                    let mut link = format!(
                        "{}. {}",
                        index + 1,
                        format_wikilink(Path::new(ref_path), config.obsidian_path(), false)
                    );
                    if config.apply_changes() {
                        if let Some(keeper) = keeper_path {
                            if &group.path != keeper {
                                link.push_str(" - updated");
                                if let Err(e) = handle_file_operation(
                                    Path::new(ref_path),
                                    FileOperation::UpdateReference(
                                        group.path.clone(),
                                        keeper.clone(),
                                    ),
                                ) {
                                    eprintln!("Error updating reference in {:?}: {}", ref_path, e);
                                }
                            }
                        } else {
                            link.push_str(" - reference removed");
                            let remove_path = group
                                .path
                                .file_name()
                                .unwrap_or_default()
                                .to_str()
                                .unwrap_or_default();
                            if let Err(e) = handle_file_operation(
                                Path::new(ref_path),
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn setup_test_file(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_file.md");
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();
        (temp_dir, file_path)
    }

    #[test]
    fn test_remove_reference() {
        let content = "# Test\n![Image](test.jpg)\nSome text\n![[test.jpg]]\nMore text";
        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("test.jpg");

        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();

        let today = Local::now().format("[[%Y-%m-%d]]").to_string();
        let expected_content = format!(
            "---\ndate_modified: \"{}\"\n---\n# Test\nSome text\nMore text",
            today
        );

        assert_eq!(result, expected_content);
    }

    #[test]
    fn test_single_invocation() {
        let content = "# Test\n![Image](test.jpg)\nSome text";
        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("test.jpg");

        // first invocation
        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        // second invocation
        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();

        let today = Local::now().format("[[%Y-%m-%d]]").to_string();
        let expected_content = format!("---\ndate_modified: \"{}\"\n---\n# Test\nSome text", today);

        assert_eq!(result, expected_content);
    }

    #[test]
    fn test_delete() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file_to_delete.jpg");
        File::create(&file_path).unwrap();

        assert!(file_path.exists(), "Test file should exist before deletion");

        handle_file_operation(&file_path, FileOperation::Delete).unwrap();

        assert!(
            !file_path.exists(),
            "Test file should not exist after deletion"
        );
    }

    #[test]
    fn test_handle_file_operation_wikilink_error() {
        let wikilink_path = PathBuf::from("[[Some File]]");

        // Test with Delete operation
        let result = handle_file_operation(&wikilink_path, FileOperation::Delete);
        assert!(
            result.is_err(),
            "Delete operation should fail with wikilink path"
        );

        // Test with RemoveReference operation
        let result = handle_file_operation(
            &wikilink_path,
            FileOperation::RemoveReference(PathBuf::from("old.jpg")),
        );
        assert!(
            result.is_err(),
            "RemoveReference operation should fail with wikilink path"
        );

        // Test with UpdateReference operation
        let result = handle_file_operation(
            &wikilink_path,
            FileOperation::UpdateReference(PathBuf::from("old.jpg"), PathBuf::from("new.jpg")),
        );
        assert!(
            result.is_err(),
            "UpdateReference operation should fail with wikilink path"
        );
    }

    #[test]
    fn test_remove_reference_with_path() {
        let content =
            "# Test\n![[conf/media/test.jpg]]\nSome text\n![Image](conf/media/test.jpg)\nMore text";
        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("conf").join("media").join("test.jpg");

        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        let today = Local::now().format("[[%Y-%m-%d]]").to_string();
        let expected_content = format!(
            "---\ndate_modified: \"{}\"\n---\n# Test\nSome text\nMore text",
            today
        );

        assert_eq!(result, expected_content);
    }

    #[test]
    fn test_update_reference_with_path() {
        let content =
            "# Test\n![[conf/media/old.jpg]]\nSome text\n![Image](conf/media/old.jpg)\nMore text";
        let (temp_dir, file_path) = setup_test_file(content);
        let old_path = temp_dir.path().join("conf").join("media").join("old.jpg");
        let new_path = temp_dir.path().join("conf").join("media").join("new.jpg");

        handle_file_operation(
            &file_path,
            FileOperation::UpdateReference(old_path.clone(), new_path.clone()),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        let today = Local::now().format("[[%Y-%m-%d]]").to_string();
        let expected_content = format!(
            "---\ndate_modified: \"{}\"\n---\n# Test\n![[conf/media/new.jpg]]\nSome text\n![Image](conf/media/new.jpg)\nMore text",
            today
        );

        assert_eq!(result, expected_content);
    }

    #[test]
    fn test_update_reference_path_variants() {
        let content = r#"# Test
Normal link: ![Alt](test.jpg)
Wiki link: ![[test.jpg]]
Path link: ![Alt](path/to/test.jpg)
Path wiki: ![[path/to/test.jpg]]
More text"#;

        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("test.jpg"); // Note: just using test.jpg

        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        let today = Local::now().format("[[%Y-%m-%d]]").to_string();
        let expected_content = format!("---\ndate_modified: \"{}\"\n---\n# Test\nMore text", today);

        assert_eq!(result, expected_content);
    }

    #[test]
    fn test_mixed_reference_styles() {
        let content = r#"# Test
![Simple](test.jpg)
![[test.jpg]]
![Full Path](conf/media/test.jpg)
![[conf/media/test.jpg]]
More text"#;

        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("conf").join("media").join("test.jpg");

        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        let today = Local::now().format("[[%Y-%m-%d]]").to_string();
        let expected_content = format!("---\ndate_modified: \"{}\"\n---\n# Test\nMore text", today);

        assert_eq!(result, expected_content);
    }

    #[test]
    fn test_reference_with_spaces() {
        let content = r#"# Test
![Alt text](my test.jpg)
![[my test.jpg]]
More text"#;

        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("my test.jpg");

        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        let today = Local::now().format("[[%Y-%m-%d]]").to_string();
        let expected_content = format!("---\ndate_modified: \"{}\"\n---\n# Test\nMore text", today);

        assert_eq!(result, expected_content);
    }
    #[test]
    fn test_cleanup_with_labels() {
        let content = r#"# Test
Label 1: ![Alt](test.jpg) text
Label 2: ![[test.jpg]] more text
Just label: ![[test.jpg]]
Mixed: ![Alt](test.jpg) ![[test.jpg]]
More text"#;

        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("test.jpg");

        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        let today = Local::now().format("[[%Y-%m-%d]]").to_string();
        let expected_content = format!(
            "---\ndate_modified: \"{}\"\n---\n# Test\nLabel 1: text\nLabel 2: more text\nMore text",
            today
        );

        assert_eq!(result, expected_content);
    }

    #[test]
    fn test_reference_with_inline_text() {
        let content = r#"# Test
Before ![Alt](test.jpg) after
Text before ![[test.jpg]] and after
More text"#;

        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("test.jpg");

        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        let today = Local::now().format("[[%Y-%m-%d]]").to_string();
        let expected_content = format!(
            "---\ndate_modified: \"{}\"\n---\n# Test\nBefore after\nText before and after\nMore text",
            today
        );

        assert_eq!(result, expected_content);
    }

    #[test]
    fn test_frontmatter_preservation() {
        let content = r#"---
title: Test Document
tags: [test, image]
date: 2024-01-01
---
# Test
![Image](test.jpg)
Some text"#;

        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("test.jpg");

        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        assert!(result.contains("title: Test Document"));
        assert!(result.contains("tags: [test, image]"));
        assert!(result.contains("date: 2024-01-01"));
    }

    #[test]
    fn test_multiple_references_same_image() {
        let content = r#"# Test
First reference: ![Alt](test.jpg)
Second reference: ![[test.jpg]]
Third reference in path: ![Alt](conf/media/test.jpg)
Fourth reference: ![[conf/media/test.jpg]]
Some content here."#;

        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("test.jpg");

        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        let today = Local::now().format("[[%Y-%m-%d]]").to_string();
        let expected_content = format!(
            "---\ndate_modified: \"{}\"\n---\n# Test\nSome content here.",
            today
        );

        assert_eq!(result, expected_content);
        assert!(!result.contains("test.jpg"));
        assert!(!result.contains("reference:")); // Verify labels are removed
    }

    #[test]
    fn test_update_reference_with_special_characters() {
        let content = r#"# Test
![Alt](test-with-dashes.jpg)
![[test with spaces.jpg]]
![Alt](test_with_underscores.jpg)
![[test.with.dots.jpg]]"#;

        let (temp_dir, file_path) = setup_test_file(content);
        let old_files = vec![
            "test-with-dashes.jpg",
            "test with spaces.jpg",
            "test_with_underscores.jpg",
            "test.with.dots.jpg",
        ];

        for old_file in old_files {
            let old_path = temp_dir.path().join(old_file);
            handle_file_operation(&file_path, FileOperation::RemoveReference(old_path)).unwrap();
        }

        let result = fs::read_to_string(&file_path).unwrap();
        assert!(!result.contains("test-with-dashes.jpg"));
        assert!(!result.contains("test with spaces.jpg"));
        assert!(!result.contains("test_with_underscores.jpg"));
        assert!(!result.contains("test.with.dots.jpg"));
    }

    #[test]
    fn test_nested_directories() {
        let content = r#"# Test
![Alt](deeply/nested/path/test.jpg)
![[another/path/test.jpg]]
![Alt](../relative/path/test.jpg)
![[./current/path/test.jpg]]"#;

        let (temp_dir, file_path) = setup_test_file(content);

        // Create nested directory structure
        let paths = ["deeply/nested/path", "another/path", "current/path"];

        for path in paths.iter() {
            fs::create_dir_all(temp_dir.path().join(path)).unwrap();
        }

        let test_path = temp_dir.path().join("deeply/nested/path/test.jpg");

        handle_file_operation(&file_path, FileOperation::RemoveReference(test_path)).unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        assert!(!result.contains("deeply/nested/path/test.jpg"));
    }

    #[test]
    fn test_image_reference_with_metadata() {
        let content = r#"# Test
Standard link: ![Alt|size=200](test.jpg)
Wiki with size: ![[test.jpg|200]]
Wiki with caption: ![[test.jpg|This is a caption]]
Multiple params: ![[test.jpg|200|caption text]]
Some text"#;

        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("test.jpg");

        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        let today = Local::now().format("[[%Y-%m-%d]]").to_string();
        // Our cleanup function is designed to remove empty lines and simplify to just the text
        let expected_content = format!("---\ndate_modified: \"{}\"\n---\n# Test\nSome text", today);
        assert_eq!(result, expected_content);
        assert!(!result.contains("test.jpg"));
    }

    #[test]
    fn test_group_images() {
        let temp_dir = TempDir::new().unwrap();
        let mut image_map = HashMap::new();

        // Create test files
        let tiff_path = temp_dir.path().join("test.tiff");
        let zero_byte_path = temp_dir.path().join("empty.jpg");
        let unreferenced_path = temp_dir.path().join("unreferenced.jpg");
        let duplicate_path1 = temp_dir.path().join("duplicate1.jpg");
        let duplicate_path2 = temp_dir.path().join("duplicate2.jpg");

        // Create empty file
        File::create(&zero_byte_path).unwrap();

        // Add test entries to image_map
        image_map.insert(
            tiff_path.clone(),
            ImageInfo {
                hash: "hash1".to_string(),
                references: vec!["ref1".to_string()],
            },
        );

        image_map.insert(
            zero_byte_path.clone(),
            ImageInfo {
                hash: "hash2".to_string(),
                references: vec!["ref2".to_string()],
            },
        );

        image_map.insert(
            unreferenced_path.clone(),
            ImageInfo {
                hash: "hash3".to_string(),
                references: vec![],
            },
        );

        let duplicate_hash = "hash4".to_string();
        image_map.insert(
            duplicate_path1.clone(),
            ImageInfo {
                hash: duplicate_hash.clone(),
                references: vec!["ref3".to_string()],
            },
        );
        image_map.insert(
            duplicate_path2.clone(),
            ImageInfo {
                hash: duplicate_hash.clone(),
                references: vec!["ref4".to_string()],
            },
        );

        // Group the images
        let grouped = group_images(&image_map);

        // Verify TIFF images
        assert!(grouped.get(&ImageGroupType::TiffImage).is_some());

        // Verify zero-byte images
        let zero_byte_group = grouped.get(&ImageGroupType::ZeroByteImage).unwrap();
        assert_eq!(zero_byte_group.len(), 1);
        assert_eq!(zero_byte_group[0].path, zero_byte_path);

        // Verify unreferenced images
        let unreferenced_group = grouped.get(&ImageGroupType::UnreferencedImage).unwrap();
        assert_eq!(unreferenced_group.len(), 1);
        assert_eq!(unreferenced_group[0].path, unreferenced_path);

        // Verify duplicate groups
        let duplicate_groups = grouped.get_duplicate_groups();
        assert_eq!(duplicate_groups.len(), 1);
        let (hash, group) = duplicate_groups[0];
        assert_eq!(hash, &duplicate_hash);
        assert_eq!(group.len(), 2);
        assert!(group.iter().any(|g| g.path == duplicate_path1));
        assert!(group.iter().any(|g| g.path == duplicate_path2));
    }

    #[test]
    fn test_determine_group_type_case_insensitive() {
        let temp_dir = TempDir::new().unwrap();

        // Test different case variations of TIFF extension
        let extensions = ["tiff", "TIFF", "Tiff", "TiFf"];

        for ext in extensions {
            let path = temp_dir.path().join(format!("test.{}", ext));
            File::create(&path).unwrap();

            let info = ImageInfo {
                hash: "hash1".to_string(),
                references: vec!["ref1".to_string()],
            };

            let group_type = determine_group_type(&path, &info);
            assert!(
                matches!(group_type, ImageGroupType::TiffImage),
                "Failed to match TIFF extension: {}",
                ext
            );
        }
    }
}

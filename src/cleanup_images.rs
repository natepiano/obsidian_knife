use crate::file_utils::update_file;
use crate::scan::{CollectedFiles, ImageInfo};
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::validated_config::ValidatedConfig;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
struct ImageGroup {
    path: PathBuf,
    info: ImageInfo,
}

pub fn cleanup_images(
    config: &ValidatedConfig,
    collected_files: &CollectedFiles,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("#", "image cleanup")?;

    let image_groups = group_images(&collected_files.image_map);
    let missing_references = generate_missing_references(&collected_files)?;

    let empty_vec = Vec::new();
    let tiff_images = image_groups.get("TIFF Images").unwrap_or(&empty_vec);
    let zero_byte_images = image_groups.get("Zero-Byte Images").unwrap_or(&empty_vec);
    let unreferenced_images = image_groups
        .get("Unreferenced Images")
        .unwrap_or(&empty_vec);
    let duplicate_groups: Vec<_> = image_groups
        .iter()
        .filter(|(key, group)| {
            *key != "TIFF Images"
                && *key != "zero-byte images"
                && *key != "unreferenced images"
                && group.len() > 1
        })
        .collect();

    if tiff_images.is_empty()
        && zero_byte_images.is_empty()
        && unreferenced_images.is_empty()
        && duplicate_groups.is_empty()
        && missing_references.is_empty()
    {
        writer.writeln("", "no issues found during image analysis.")?;
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
        let description = format!(
            "The following {} {} may not render correctly in Obsidian:",
            tiff_images.len(),
            if tiff_images.len() == 1 {
                "TIFF image"
            } else {
                "TIFF images"
            }
        );
        write_special_group_table(config, writer, "TIFF Images", tiff_images, &description)?;
    }

    if !zero_byte_images.is_empty() {
        let description = format!(
            "The following {} {} zero bytes and may be corrupted:",
            zero_byte_images.len(),
            if zero_byte_images.len() == 1 {
                "image has"
            } else {
                "images have"
            }
        );
        write_special_group_table(
            config,
            writer,
            "Zero-Byte Images",
            zero_byte_images,
            &description,
        )?;
    }

    if !unreferenced_images.is_empty() {
        let description = format!(
            "The following {} {} not referenced by any files:",
            unreferenced_images.len(),
            if unreferenced_images.len() == 1 {
                "image is"
            } else {
                "images are"
            }
        );
        write_special_group_table(
            config,
            writer,
            "Unreferenced Images",
            unreferenced_images,
            &description,
        )?;
    }

    for (hash, group) in duplicate_groups {
        write_duplicate_group_table(config, writer, hash, group)?;
    }

    Ok(())
}

fn group_images(image_map: &HashMap<PathBuf, ImageInfo>) -> HashMap<String, Vec<ImageGroup>> {
    let mut groups: HashMap<String, Vec<ImageGroup>> = HashMap::new();

    for (path, info) in image_map {
        let group_type = determine_group_type(path, info);
        groups.entry(group_type).or_default().push(ImageGroup {
            path: path.clone(),
            info: info.clone(),
        });
    }

    // Sort groups by path
    for group in groups.values_mut() {
        group.sort_by(|a, b| a.path.cmp(&b.path));
    }

    groups
}

fn determine_group_type(path: &Path, info: &ImageInfo) -> String {
    if path.extension().and_then(|ext| ext.to_str()) == Some("tiff") {
        "TIFF Images".to_string()
    } else if fs::metadata(path).map(|m| m.len() == 0).unwrap_or(false) {
        "Zero-Byte Images".to_string()
    } else if info.references.is_empty() {
        "Unreferenced Images".to_string()
    } else {
        info.hash.clone()
    }
}

fn generate_missing_references(
    collected_files: &CollectedFiles,
) -> Result<Vec<(&PathBuf, String)>, Box<dyn Error + Send + Sync>> {
    let mut missing_references = Vec::new();
    let image_filenames: HashSet<String> = collected_files
        .image_map
        .keys()
        .filter_map(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_lowercase())
        .collect();

    for (markdown_path, file_info) in &collected_files.markdown_files {
        for image_link in &file_info.image_links {
            if let Some(extracted_filename) = extract_local_image_filename(image_link) {
                if !image_exists_in_set(&extracted_filename, &image_filenames) {
                    missing_references.push((markdown_path, extracted_filename));
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

    writer.writeln("## Missing Image References", "")?;
    writer.writeln(
        "",
        "The following markdown files refer to missing local image files:\n",
    )?;

    let headers = &["Markdown File", "Missing Image Reference", "Action"];

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
    if image_link.starts_with("![[") && image_link.ends_with("]]") {
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
        if !url.starts_with("http://") && !url.starts_with("https://") {
            url.rsplit('/').next().map(|s| s.to_lowercase())
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
    description: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("##", group_type)?;
    writer.writeln("", &format!("{}\n", description))?;
    write_group_table(config, writer, groups, false, true)?; // Note the added `true` parameter
    Ok(())
}

fn write_duplicate_group_table(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
    group_hash: &str,
    groups: &[ImageGroup],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("## Duplicate Images with References", "")?;
    writer.writeln("###", &format!("image file hash: {}", group_hash))?;
    writer.writeln("", &format!("{} duplicates", groups.len() - 1))?;
    let total_references: usize = groups.iter().map(|g| g.info.references.len()).sum();
    writer.writeln("", &format!("referenced by {} files\n", total_references))?;
    write_group_table(config, writer, groups, true, false)?; // Note the added `false` parameter
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
    if path
        .to_str()
        .map_or(false, |s| s.contains("[[") && s.contains("]]"))
    {
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
        let old_name = old_path.file_name().unwrap().to_str().unwrap();
        let regex = Regex::new(&format!(
            r"(!?\[.*?\]\({}(?:\|.*?)?\))|(!\[\[{}(?:\|.*?)?\]\])",
            regex::escape(old_name),
            regex::escape(old_name)
        ))
        .unwrap();

        let new_content = regex.replace_all(&content, |caps: &regex::Captures| {
            let matched = caps.get(0).unwrap().as_str();
            if let Some(new_path) = new_path {
                let new_name = new_path.file_name().unwrap().to_str().unwrap();
                matched.replace(old_name, new_name)
            } else {
                String::new()
            }
        });

        // Clean up empty lines when content was removed
        if new_path.is_none() {
            new_content
                .lines()
                .filter(|&line| !line.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            new_content.into_owned()
        }
    })
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
}

use crate::scan::{CollectedFiles, ImageInfo};
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::validated_config::ValidatedConfig;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

pub fn dedupe(
    config: &ValidatedConfig,
    collected_files: CollectedFiles,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if !config.cleanup_image_files() {
        writer.writeln("#", "Image deduplication is off")?;
        return Ok(());
    }

    writer.writeln("#", "Image Deduplication")?;
    if config.destructive() {
        writer.writeln("", "Changes will be applied")?;
    }

    let image_map = &collected_files.image_map;

    // Group images by hash
    let mut hash_groups: HashMap<String, Vec<(&PathBuf, &ImageInfo)>> = HashMap::new();
    for (path, info) in image_map {
        hash_groups
            .entry(info.hash.clone())
            .or_default()
            .push((path, info));
    }

    // Identify different image groups
    let mut tiff_images = Vec::new();
    let mut zero_byte_images = Vec::new();
    let mut unreferenced_images = Vec::new();
    let mut duplicate_groups = Vec::new();

    for (hash, group) in hash_groups {
        let is_tiff = group
            .iter()
            .any(|(path, _)| path.extension().and_then(|ext| ext.to_str()) == Some("tiff"));
        let is_zero_byte = group
            .iter()
            .any(|(path, _)| fs::metadata(path).map(|m| m.len() == 0).unwrap_or(false));
        let all_unreferenced = group.iter().all(|(_, info)| info.references.is_empty());

        if is_tiff {
            tiff_images.extend(group);
        } else if is_zero_byte {
            zero_byte_images.extend(group);
        } else if all_unreferenced {
            unreferenced_images.extend(group);
        } else if group.len() > 1 {
            duplicate_groups.push((hash, group));
        }
    }

    let missing_references = generate_missing_references(&collected_files)?;

    // Return early if all vectors are empty
    if tiff_images.is_empty()
        && zero_byte_images.is_empty()
        && unreferenced_images.is_empty()
        && duplicate_groups.is_empty()
        && missing_references.is_empty()
    {
        writer.writeln("", "No issues found during image analysis.")?;
        return Ok(());
    }

    // Write missing image references table if there are any missing references
    if !missing_references.is_empty() {
        write_missing_image_references_table(config, &missing_references, writer)?;
    }

    // Sort and write tables for each group
    tiff_images.sort_by(|a, b| a.0.cmp(b.0));
    zero_byte_images.sort_by(|a, b| a.0.cmp(b.0));
    unreferenced_images.sort_by(|a, b| a.0.cmp(b.0));
    duplicate_groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    if !tiff_images.is_empty() {
        writer.writeln("##", "TIFF Images")?;
        writer.writeln(
            "",
            format!(
                "The following {} TIFF images may not render correctly in Obsidian:",
                tiff_images.len()
            )
            .as_str(),
        )?;
        write_group_tables(config, writer, &[("TIFF Images", tiff_images)], false)?;
    }

    if !zero_byte_images.is_empty() {
        writer.writeln("##", "Zero-Byte Images")?;
        writer.writeln(
            "",
            format!(
                "The following {} images have zero bytes and may be corrupted:",
                zero_byte_images.len()
            )
            .as_str(),
        )?;
        write_group_tables(
            config,
            writer,
            &[("Zero-Byte Images", zero_byte_images)],
            false,
        )?;
    }

    if !unreferenced_images.is_empty() {
        writer.writeln("##", "Unreferenced Images")?;
        writer.writeln(
            "",
            format!(
                "The following {} images are not referenced by any files:",
                unreferenced_images.len()
            )
            .as_str(),
        )?;
        // Group unreferenced images by their hash
        let mut unreferenced_groups: HashMap<String, Vec<(&PathBuf, &ImageInfo)>> = HashMap::new();
        for (path, info) in unreferenced_images {
            unreferenced_groups
                .entry(info.hash.clone())
                .or_default()
                .push((path, info));
        }
        let unreferenced_groups: Vec<_> = unreferenced_groups.into_iter().collect();
        write_group_tables(config, writer, &unreferenced_groups, false)?;
    }

    if !duplicate_groups.is_empty() {
        writer.writeln("##", "Duplicate Images with References")?;
        write_group_tables(config, writer, &duplicate_groups, true)?;
    }

    Ok(())
}

fn generate_missing_references<'a>(
    collected_files: &CollectedFiles,
) -> Result<Vec<(&PathBuf, &String, String)>, Box<dyn Error + Send + Sync>> {
    let mut missing_references = Vec::new();

    let image_map = &collected_files.image_map;

    // Create a HashSet of normalized image filenames for faster lookup
    let image_filenames: HashSet<String> = image_map
        .keys()
        .filter_map(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_lowercase())
        .collect();

    for (markdown_path, file_info) in &collected_files.markdown_files {
        for image_link in &file_info.image_links {
            if let Some(extracted_filename) = extract_local_image_filename(image_link) {
                if !image_exists_in_set(&extracted_filename, &image_filenames) {
                    missing_references.push((markdown_path, image_link, extracted_filename));
                }
            }
        }
    }

    Ok(missing_references)
}

fn write_missing_image_references_table(
    config: &ValidatedConfig,
    missing_references: &[(&PathBuf, &String, String)],
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("## Missing Image References", "")?;
    writer.writeln(
        "",
        "The following markdown files refer to missing local image files:\n",
    )?;

    let headers = &[
        "Markdown File",
        "Missing Image Reference",
        "Extracted Filename",
    ];
    let rows: Vec<Vec<String>> = missing_references
        .iter()
        .map(|(markdown_path, image_link, extracted_filename)| {
            vec![
                format_wikilink(markdown_path, config.obsidian_path(), false),
                (*image_link).to_string(),
                extracted_filename.to_string(),
            ]
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

fn write_group_tables(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
    groups: &[(impl AsRef<str>, Vec<(&PathBuf, &ImageInfo)>)],
    is_ref_group: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let headers = &["Sample", "Duplicates", "Referenced By"];

    for (group_name, group) in groups.iter() {
        write_table_header(writer, group_name, group)?;

        let keeper_path = if is_ref_group {
            Some(group[0].0.clone())
        } else {
            None
        };

        let sample = format!(
            "![[{}\\|400]]",
            group[0].0.file_name().unwrap().to_string_lossy()
        );

        let duplicates: Vec<String> = group
            .iter()
            .enumerate()
            .map(|(i, (path, _))| {
                let mut link = format!(
                    "{}. {}",
                    i + 1,
                    format_wikilink(path, config.obsidian_path(), true)
                );
                if config.destructive() {
                    if is_ref_group && i == 0 {
                        link.push_str(" - kept");
                    } else {
                        link.push_str(" - deleted");
                        if let Err(e) = handle_file_operation(path, FileOperation::Delete) {
                            eprintln!("Error deleting file {:?}: {}", path, e);
                        }
                    }
                }
                link
            })
            .collect();

        let duplicates = duplicates.join("<br>");

        let references: Vec<String> = group
            .iter()
            .flat_map(|(path, info)| {
                let path = (*path).to_path_buf();
                let keeper_path = keeper_path.clone();
                info.references
                    .iter()
                    .map(move |ref_path| (path.clone(), keeper_path.clone(), ref_path.to_string()))
            })
            .enumerate()
            .map(|(index, (path, keeper_path, ref_path))| {
                let mut link = format!(
                    "{}. {}",
                    index + 1,
                    format_wikilink(Path::new(&ref_path), config.obsidian_path(), false)
                );
                if config.destructive() {
                    if is_ref_group {
                        link.push_str(" - updated");
                        if let Some(keeper_path) = &keeper_path {
                            if &path != keeper_path {
                                if let Err(e) = handle_file_operation(
                                    Path::new(&ref_path),
                                    FileOperation::UpdateReference(
                                        path.clone(),
                                        keeper_path.clone(),
                                    ),
                                ) {
                                    eprintln!("Error updating reference in {:?}: {}", ref_path, e);
                                }
                            }
                        }
                    } else {
                        link.push_str(" - reference removed");
                        if let Err(e) = handle_file_operation(
                            Path::new(&ref_path),
                            FileOperation::RemoveReference(path),
                        ) {
                            eprintln!("Error removing reference in {:?}: {}", ref_path, e);
                        }
                    }
                }
                link
            })
            .collect();

        let references = references.join("<br>");

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
    }
    Ok(())
}

fn write_table_header(
    writer: &ThreadSafeWriter,
    group_name: impl AsRef<str>,
    group: &[(&PathBuf, &ImageInfo)],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("###", &format!("image file hash: {}", group_name.as_ref()))?;

    let duplicates_message = if group.len() == 1 {
        "no duplicates"
    } else if group.len() == 2 {
        "1 duplicate"
    } else {
        &format!("{} duplicates", group.len() - 1)
    };

    writer.writeln("", duplicates_message)?;

    // Calculate total number of references
    let total_references: usize = group.iter().map(|(_, info)| info.references.len()).sum();

    // Output reference count if there are any references
    if total_references > 0 {
        writer.writeln("", &format!("referenced by {} files", total_references))?;
    } else {
        writer.writeln("", "not referenced by any file in obsidian folder")?;
    }
    writer.writeln("", "")?; // Add an empty line for better readability
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
    let content = fs::read_to_string(file_path)?;
    let old_name = old_path.file_name().unwrap().to_str().unwrap();

    let regex = Regex::new(&format!(
        r"(!?\[.*?\]\({}(?:\|.*?)?\))|(!\[\[{}(?:\|.*?)?\]\])",
        regex::escape(old_name),
        regex::escape(old_name)
    ))?;

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
    let new_content = if new_path.is_none() {
        new_content
            .lines()
            .filter(|&line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        new_content.into_owned()
    };

    fs::write(file_path, new_content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn setup_test_file(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_file.md");
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();
        println!("Test file created at: {:?}", file_path);
        (temp_dir, file_path)
    }

    #[test]
    fn test_remove_reference() {
        println!("\n--- Starting test_remove_reference ---");
        let content = "# Test\n![Image](test.jpg)\nSome text\n![[test.jpg]]\nMore text";
        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("test.jpg");

        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        println!("File content after operation: {:?}", result);
        assert_eq!(result, "# Test\nSome text\nMore text");

        println!("--- Ending test_remove_reference ---\n");
    }

    #[test]
    fn test_update_reference() {
        println!("\n--- Starting test_update_reference ---");
        let content = "# Test\n![Image](old.jpg)\nSome text\n![[old.jpg]]\nMore text";
        let (temp_dir, file_path) = setup_test_file(content);
        let old_image_path = temp_dir.path().join("old.jpg");
        let new_image_path = temp_dir.path().join("new.jpg");

        handle_file_operation(
            &file_path,
            FileOperation::UpdateReference(old_image_path, new_image_path),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        println!("File content after operation: {:?}", result);
        assert_eq!(
            result,
            "# Test\n![Image](new.jpg)\nSome text\n![[new.jpg]]\nMore text"
        );

        println!("--- Ending test_update_reference ---\n");
    }

    #[test]
    fn test_single_invocation() {
        println!("\n--- Starting test_single_invocation ---");
        let content = "# Test\n![Image](test.jpg)\nSome text";
        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("test.jpg");

        println!("Performing first invocation");
        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        println!("Performing second invocation");
        handle_file_operation(
            &file_path,
            FileOperation::RemoveReference(image_path.clone()),
        )
        .unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        println!("File content after operations: {:?}", result);
        assert_eq!(result, "# Test\nSome text");

        println!("--- Ending test_single_invocation ---\n");
    }

    #[test]
    fn test_delete() {
        println!("\n--- Starting test_delete ---");
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file_to_delete.jpg");
        File::create(&file_path).unwrap();
        println!("Test file created at: {:?}", file_path);

        assert!(file_path.exists(), "Test file should exist before deletion");

        handle_file_operation(&file_path, FileOperation::Delete).unwrap();

        assert!(
            !file_path.exists(),
            "Test file should not exist after deletion"
        );

        println!("--- Ending test_delete ---\n");
    }
}

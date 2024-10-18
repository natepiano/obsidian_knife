use crate::scan::ImageInfo;
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::validated_config::ValidatedConfig;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use regex::Regex;
use tempfile::TempDir;

pub fn dedupe(
    config: &ValidatedConfig,
    image_map: &HashMap<PathBuf, ImageInfo>,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if !config.dedupe_images() {
        writer.writeln("#", "Image deduplication is off")?;
        return Ok(());
    }

    writer.writeln("#", "Image Analysis")?;
    if config.destructive() {
        writer.writeln("", "Changes will be applied")?;
    }

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
    let mut duplicate_groups = Vec::new();

    for (hash, group) in hash_groups {
        let is_tiff = group.iter().any(|(path, _)|
            path.extension().and_then(|ext| ext.to_str()) == Some("tiff"));
        let is_zero_byte = group.iter().any(|(path, _)|
            fs::metadata(path).map(|m| m.len() == 0).unwrap_or(false));

        if is_tiff {
            tiff_images.extend(group);
        } else if is_zero_byte {
            zero_byte_images.extend(group);
        } else if group.len() > 1 {
            duplicate_groups.push((hash, group));
        }
    }

    // Sort and write tables for each group
    tiff_images.sort_by(|a, b| a.0.cmp(b.0));
    zero_byte_images.sort_by(|a, b| a.0.cmp(b.0));
    duplicate_groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    if !tiff_images.is_empty() {
        writer.writeln("##", "TIFF Images")?;
        writer.writeln("", "The following TIFF images may not render correctly in Obsidian:")?;
        write_group_tables(config, writer, &[("TIFF Images", tiff_images)], false)?;
    }

    if !zero_byte_images.is_empty() {
        writer.writeln("##", "Zero-Byte Images")?;
        writer.writeln("", "The following images have zero bytes and may be corrupted:")?;
        write_group_tables(config, writer, &[("Zero-Byte Images", zero_byte_images)], false)?;
    }

    if !duplicate_groups.is_empty() {
        // Separate groups with no references and those with references
        let (no_ref_groups, ref_groups): (Vec<_>, Vec<_>) = duplicate_groups
            .into_iter()
            .partition(|(_, group)| group.iter().all(|(_, info)| info.references.is_empty()));

        if !no_ref_groups.is_empty() {
            writer.writeln("##", "Duplicate Images with No References")?;
            write_group_tables(config, writer, &no_ref_groups, false)?;
        }

        if !ref_groups.is_empty() {
            writer.writeln("##", "Duplicate Images with References")?;
            write_group_tables(config, writer, &ref_groups, true)?;
        }
    }

    Ok(())
}

fn write_group_tables(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
    groups: &[(impl AsRef<str>, Vec<(&PathBuf, &ImageInfo)>)],
    is_ref_group: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let headers = &["Sample", "Duplicates", "Referenced By"];

    for (group_name, group) in groups {
        writer.writeln("###", &format!("{} - {}", group.len(), group_name.as_ref()))?;

        let sample = format!(
            "![[{}\\|400]]",
            group[0].0.file_name().unwrap().to_string_lossy()
        );
        let duplicates = group
            .iter()
            .enumerate()
            .map(|(i, (path, _))| {
                let mut link = format_wikilink(path, config.obsidian_path(), true);
                if config.destructive() {
                    if is_ref_group && i == 0 {
                        link.push_str(" - kept");
                    } else {
                        link.push_str(" - deleted");
                        // handle_file_operation(path, FileOperation::Delete);
                    }
                }
                link
            })
            .collect::<Vec<_>>()
            .join("<br>");
        let references = group
            .iter()
            .flat_map(|(_, info)| &info.references)
            .map(|path| {
                let mut link = format_wikilink(Path::new(path), config.obsidian_path(), false);
                if config.destructive() {
                    if is_ref_group {
                        link.push_str(" - updated");
                        // handle_file_operation(Path::new(path), FileOperation::UpdateReference(group[0].0.clone()));
                    } else {
                        link.push_str(" - reference removed");
                        // handle_file_operation(Path::new(path), FileOperation::RemoveReference(group[0].0.clone()));
                    }
                }
                link
            })
            .collect::<Vec<_>>()
            .join("<br>");

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

fn handle_file_operation(path: &Path, operation: FileOperation) -> Result<(), Box<dyn Error + Send + Sync>> {
    match operation {
        FileOperation::Delete => {
            fs::remove_file(path)?;
            Ok(())
        }
        FileOperation::RemoveReference(ref old_path) => {
            // Treat removal as a replacement with empty string
            update_file_content(path, old_path, None)
        }
        FileOperation::UpdateReference(ref old_path, ref new_path) => {
            update_file_content(path, old_path, Some(new_path))
        }
    }
}

fn update_file_content(file_path: &Path, old_path: &Path, new_path: Option<&Path>) -> Result<(), Box<dyn Error + Send + Sync>> {
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
        (temp_dir, file_path)
    }

    #[test]
    fn test_remove_reference() {
        let content = "# Test\n![Image](test.jpg)\nSome text\n![[test.jpg]]\nMore text";
        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("test.jpg");

        handle_file_operation(&file_path, FileOperation::RemoveReference(image_path)).unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        assert_eq!(result, "# Test\nSome text\nMore text");
    }

    #[test]
    fn test_update_reference() {
        let content = "# Test\n![Image](old.jpg)\nSome text\n![[old.jpg]]\nMore text";
        let (temp_dir, file_path) = setup_test_file(content);
        let old_image_path = temp_dir.path().join("old.jpg");
        let new_image_path = temp_dir.path().join("new.jpg");

        handle_file_operation(&file_path, FileOperation::UpdateReference(old_image_path, new_image_path)).unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        assert_eq!(result, "# Test\n![Image](new.jpg)\nSome text\n![[new.jpg]]\nMore text");
    }

    #[test]
    fn test_update_reference_with_alt_text() {
        let content = "# Test\n![Old Alt Text](old.jpg)\nSome text\n![[old.jpg|Old Alt Text]]\nMore text";
        let (temp_dir, file_path) = setup_test_file(content);
        let old_image_path = temp_dir.path().join("old.jpg");
        let new_image_path = temp_dir.path().join("new.jpg");

        handle_file_operation(&file_path, FileOperation::UpdateReference(old_image_path, new_image_path)).unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        assert_eq!(result, "# Test\n![Old Alt Text](new.jpg)\nSome text\n![[new.jpg|Old Alt Text]]\nMore text");
    }

    #[test]
    fn test_remove_reference_no_match() {
        let content = "# Test\nNo images here\nJust text";
        let (temp_dir, file_path) = setup_test_file(content);
        let image_path = temp_dir.path().join("nonexistent.jpg");

        handle_file_operation(&file_path, FileOperation::RemoveReference(image_path)).unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        assert_eq!(result, content);
    }

    #[test]
    fn test_update_reference_no_match() {
        let content = "# Test\nNo images here\nJust text";
        let (temp_dir, file_path) = setup_test_file(content);
        let old_image_path = temp_dir.path().join("old.jpg");
        let new_image_path = temp_dir.path().join("new.jpg");

        handle_file_operation(&file_path, FileOperation::UpdateReference(old_image_path, new_image_path)).unwrap();

        let result = fs::read_to_string(&file_path).unwrap();
        assert_eq!(result, content);
    }

    #[test]
    fn test_delete() {
        // Create a temporary file
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file_to_delete.jpg");
        File::create(&file_path).unwrap();

        // Verify file exists
        assert!(file_path.exists(), "Test file should exist before deletion");

        // Delete the file
        handle_file_operation(&file_path, FileOperation::Delete).unwrap();

        // Verify file no longer exists
        assert!(!file_path.exists(), "Test file should not exist after deletion");
    }
}

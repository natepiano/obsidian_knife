use crate::scan::ImageInfo;
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use crate::validated_config::ValidatedConfig;

pub fn find_and_output_duplicate_images(
    config: &ValidatedConfig,
    image_map: &HashMap<PathBuf, ImageInfo>,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if !config.dedupe_images() {
        writer.writeln("##", "Image deduplicate is off")?;
        return Ok(());
    }

    writer.writeln("##", "Duplicate Images")?;

    // Group images by hash
    let mut hash_groups: HashMap<String, Vec<(&PathBuf, &ImageInfo)>> = HashMap::new();
    for (path, info) in image_map {
        hash_groups
            .entry(info.hash.clone())
            .or_default()
            .push((path, info));
    }

    // Filter out unique images and sort groups by size
    let mut duplicate_groups: Vec<(String, Vec<(&PathBuf, &ImageInfo)>)> = hash_groups
        .into_iter()
        .filter(|(_, group)| group.len() > 1)
        .collect();
    duplicate_groups.sort_by_key(|(_hash, group)| std::cmp::Reverse(group.len()));

    if duplicate_groups.is_empty() {
        writer.writeln("", "No duplicate images found.")?;
        return Ok(());
    }

    let headers = &["Sample", "Duplicates"];

    for (hash, group) in duplicate_groups {
        writer.writeln("###", &format!("{} - {}", group.len(), hash))?;

        let sample = format!("![[{}\\|400]]", group[0].0.file_name().unwrap().to_string_lossy());
        let duplicates = group
            .iter()
            .map(|(path, _)| format_image_link(path, config.obsidian_path()))
            .collect::<Vec<_>>()
            .join("<br>");

        let rows = vec![vec![sample, duplicates]];

        writer.write_markdown_table(
            headers,
            &rows,
            Some(&[ColumnAlignment::Left, ColumnAlignment::Left]),
        )?;

        writer.writeln("", "")?; // Add an empty line between tables
    }

    Ok(())
}

fn format_image_link(path: &Path, obsidian_path: &Path) -> String {
    let relative_path = path.strip_prefix(obsidian_path).unwrap_or(path);
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    format!("[[{}\\|{}]]", relative_path.display(), file_name)
}

use crate::scan::ImageInfo;
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::validated_config::ValidatedConfig;
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};

pub fn dedupe(
    config: &ValidatedConfig,
    image_map: &HashMap<PathBuf, ImageInfo>,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if !config.dedupe_images() {
        writer.writeln("#", "Image deduplication is off")?;
        return Ok(());
    }

    writer.writeln("#", "Image Deduplication")?;
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

    write_duplicates_tables(config, writer, &mut duplicate_groups)?;

    Ok(())
}

fn write_duplicates_tables(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
    duplicate_groups: &mut Vec<(String, Vec<(&PathBuf, &ImageInfo)>)>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let headers = &["Sample", "Duplicates", "Referenced By"];

    // Separate groups with no references and those with references
    let (no_ref_groups, ref_groups): (Vec<_>, Vec<_>) = duplicate_groups
        .iter()
        .partition(|(_, group)| group.iter().all(|(_, info)| info.references.is_empty()));

    // Write tables for groups with no references
    if !no_ref_groups.is_empty() {
        writer.writeln("##", "Duplicate Images with No References")?;
        write_group_tables(config, writer, headers, &no_ref_groups)?;
    }

    // Write tables for groups with references
    if !ref_groups.is_empty() {
        writer.writeln("##", "Duplicate Images with References")?;
        write_group_tables(config, writer, headers, &ref_groups)?;
    }

    Ok(())
}

fn write_group_tables(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
    headers: &[&str],
    groups: &[&(String, Vec<(&PathBuf, &ImageInfo)>)],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for (hash, group) in groups {
        writer.writeln("###", &format!("{} - {}", group.len(), hash))?;

        let sample = format!(
            "![[{}\\|400]]",
            group[0].0.file_name().unwrap().to_string_lossy()
        );
        let duplicates = group
            .iter()
            .map(|(path, _)| format_wikilink(path, config.obsidian_path(), true))
            .collect::<Vec<_>>()
            .join("<br>");
        let references = group
            .iter()
            .flat_map(|(_, info)| &info.references)
            .map(|path| format_wikilink(Path::new(path), config.obsidian_path(), false))
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

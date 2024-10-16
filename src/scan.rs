use std::cmp::Reverse;
use std::collections::HashMap;
use crate::{constants::IMAGE_EXTENSIONS,
            validated_config::ValidatedConfig};
use std::path::PathBuf;
use walkdir::{DirEntry, WalkDir};

pub fn scan_obsidian_folder(config: ValidatedConfig) {
    println!("apply_changes: {}", config.destructive());
    println!("dedupe_images:{}\n", config.dedupe_images());
    println!("scanning: {:?}", config.obsidian_path());

    let (markdown_files, image_files, other_files) = collect_files(&config);

    println!("\nMarkdown files: {}", markdown_files.len());

    println!("Image files:");
    let image_counts = count_image_types(&image_files);
    for (ext, count) in image_counts.iter() {
        println!("  .{}: {}", ext, count);
    }
    println!("Total image files: {}", image_files.len());

    println!("Other files: {}", other_files.len());

    if !other_files.is_empty() {
        println!("\nOther files found:");
        for file in other_files {
            println!("  {}", file.display());
        }
    }
}

fn collect_files(config: &ValidatedConfig) -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
    let ignore_folders = config.ignore_folders().unwrap_or(&[]);

    let (markdown_files, image_files, other_files): (Vec<_>, Vec<_>, Vec<_>) =
        WalkDir::new(config.obsidian_path())
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| !is_ignored_folder(e, ignore_folders))
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .map(|e| e.into_path())
            .fold(
                (Vec::new(), Vec::new(), Vec::new()),
                |(mut md, mut img, mut other), path| {
                    if path.file_name().and_then(|s| s.to_str()) == Some(".DS_Store") {
                        return (md, img, other);
                    }
                    match path.extension().and_then(|s| s.to_str()).map(|s| s.to_lowercase()) {
                        Some(ext) if ext == "md" => md.push(path),
                        Some(ext) if IMAGE_EXTENSIONS.contains(&ext.as_str()) => img.push(path),
                        _ => other.push(path),
                    }
                    (md, img, other)
                },
            );

    (markdown_files, image_files, other_files)
}

fn is_ignored_folder(entry: &DirEntry, ignore_folders: &[PathBuf]) -> bool {
    if entry.file_type().is_dir() {
        for ignored_path in ignore_folders {
            if entry.path().starts_with(ignored_path) {
                println!("ignoring: {:?}", entry.path());
                return true;
            }
        }
    }
    false
}

fn count_image_types(image_files: &[PathBuf]) -> Vec<(String, usize)> {
    let counts: HashMap<String, usize> = image_files
        .iter()
        .filter_map(|path| path.extension())
        .filter_map(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .filter(|ext| IMAGE_EXTENSIONS.contains(&ext.as_str()))
        .fold(HashMap::new(), |mut acc, ext| {
            *acc.entry(ext).or_insert(0) += 1;
            acc
        });

    let mut count_vec: Vec<(String, usize)> = counts.into_iter().collect();
    count_vec.sort_by_key(|&(_, count)| Reverse(count));
    count_vec
}

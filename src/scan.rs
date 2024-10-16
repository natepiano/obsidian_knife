use crate::validated_config::ValidatedConfig;
use std::path::PathBuf;
use walkdir::{DirEntry, WalkDir};

pub fn scan_obsidian_folder(config: ValidatedConfig) {
    println!("apply_changes: {}", config.destructive());
    println!("dedupe_images:{}\n", config.dedupe_images());
    println!("scanning: {:?}", config.obsidian_path());

    let (markdown_files, image_files, other_files) = collect_files(&config);

    println!("Markdown files: {}", markdown_files.len());
    println!("Image files: {}", image_files.len());
    println!("Other files: {}", other_files.len());

    if !other_files.is_empty() {
        println!("\nOther files found:");
        for file in other_files {
            println!("  {}", file.display());
        }
    } else {
        println!("\nNo other files found.");
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
                    match path.extension().and_then(|s| s.to_str()) {
                        Some("md") => md.push(path),
                        Some("jpg") | Some("png") | Some("jpeg") | Some("tiff") | Some("pdf") | Some("gif") => {
                            img.push(path)
                        }
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

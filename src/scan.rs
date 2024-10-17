use crate::sha256_cache::{CacheFileStatus, Sha256Cache};
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::{constants::IMAGE_EXTENSIONS, validated_config::ValidatedConfig};

use rayon::prelude::*;

use regex::Regex;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::Arc;
use walkdir::{DirEntry, WalkDir};

#[derive(Debug)]
pub struct ImageInfo {
    path: PathBuf,
    hash: String,
    references: Vec<String>,
}

#[derive(Debug)]
struct ImageReferenceCount {
    markdown_files: usize,
    image_count: usize,
    file_names: Vec<String>,
}

#[derive(Default)]
pub struct CollectedFiles {
    pub markdown_files: HashMap<PathBuf, Vec<String>>,
   // pub image_map: HashMap<PathBuf, ImageInfo>,
    pub image_files: Vec<PathBuf>,
    pub other_files: Vec<PathBuf>,
}

pub fn scan_obsidian_folder(
    config: ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<CollectedFiles, Box<dyn Error + Send + Sync>> {
    write_scan_start(&config, writer)?;

    let collected_files = collect_files(&config, writer)?;

    write_file_info(
        writer,
        &collected_files,
    )?;

    let image_info_map = get_image_info_map(&config, &collected_files, writer)?;

    Ok(collected_files)
}

fn get_image_info_map(
    config: &ValidatedConfig,
    collected_files: &CollectedFiles,
    writer: &ThreadSafeWriter,
) -> Result<HashMap<PathBuf, ImageInfo>, Box<dyn Error + Send + Sync>> {
    let cache_file_path = config
        .obsidian_path()
        .join(crate::constants::CACHE_FOLDER)
        .join("image_cache.json");
    let (mut cache, cache_file_status) = Sha256Cache::new(cache_file_path.clone())?;

    write_cache_file_info(writer, &cache_file_path, cache_file_status)?;

    let mut image_info_map = HashMap::new();
    let image_references_in_markdown_files = &collected_files.markdown_files;

    for image_path in collected_files.image_files.clone() {
        let (hash, _) = cache.get_or_update(&image_path)?;

        let image_file_name = image_path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default();
        let references: Vec<String> = image_references_in_markdown_files
            .iter()
            .filter(|(markdown_path, _)| {
                markdown_path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .unwrap_or_default()
                    .contains(image_file_name)
            })
            .map(|(path, _)| path.to_string_lossy().to_string())
            .collect();

        let image_info = ImageInfo {
            path: image_path.clone(),
            hash,
            references,
        };

        image_info_map.insert(image_path, image_info);
    }

    cache.remove_non_existent_entries();
    cache.save()?;

    write_cache_contents_info(writer, &mut cache, &mut image_info_map)?;

    let histogram = generate_markdown_image_reference_histogram(&image_references_in_markdown_files);
    write_image_reference_histogram(writer, &histogram)?;

    Ok(image_info_map)
}

fn write_image_reference_histogram(writer: &ThreadSafeWriter, histogram: &[ImageReferenceCount]) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln_markdown("###", "markdown files with images")?;

    let take_count = 5;

    let headers = &["Number of Images", "Number of Markdown Files", "Files"];
    let rows: Vec<Vec<String>> = histogram
        .iter()
        .filter(|entry| entry.image_count > 0)
        .map(|entry| {
            let wikilinks: Vec<String> = entry.file_names.iter()
                .take(take_count)  // Limit to first 5 files
                .map(|name| format!("[[{}]]", name))
                .collect();
            let file_list = if entry.file_names.len() > take_count {
                format!("{}, ...", wikilinks.join(", "))
            } else {
                wikilinks.join(", ")
            };
            vec![
                entry.image_count.to_string(),
                entry.markdown_files.to_string(),
                file_list,
            ]
        })
        .collect();

    if rows.is_empty() {
        writer.writeln_markdown("", "No Markdown files contain image references.")?;
    } else {
        writer.write_markdown_table(headers, &rows, Some(&[ColumnAlignment::Right, ColumnAlignment::Right, ColumnAlignment::Left]))?;
    }

    println!();
    Ok(())
}

fn write_cache_file_info(
    writer: &ThreadSafeWriter,
    cache_file_path: &PathBuf,
    cache_file_status: CacheFileStatus,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln_markdown("##", "collecting image file info")?;

    match cache_file_status {
        CacheFileStatus::ReadFromCache => {
            writer.writeln_markdown("", &format!("reading from cache: {:?}", cache_file_path))?
        }
        CacheFileStatus::CreatedNewCache => writer.writeln_markdown(
            "",
            &format!(
                "cache file missing - creating new cache: {:?}",
                cache_file_path
            ),
        )?,
        CacheFileStatus::CacheCorrupted => writer.writeln_markdown(
            "",
            &format!("cache corrupted, creating new cache: {:?}", cache_file_path),
        )?,
    }
    println!();
    Ok(())
}

fn write_cache_contents_info(
    writer: &ThreadSafeWriter,
    cache: &mut Sha256Cache,
    image_info_map: &mut HashMap<PathBuf, ImageInfo>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let stats = cache.get_stats();

    let headers = &["Metric", "Count"];
    let rows = vec![
        // initial was captured before deletions so we can see the results appropriately
        vec![
            "Total entries in cache (initial)".to_string(),
            stats.initial_count.to_string(),
        ],
        vec![
            "Matching files read from cache".to_string(),
            stats.files_read.to_string(),
        ],
        vec![
            "Files added to cache".to_string(),
            stats.files_added.to_string(),
        ],
        vec![
            "Matching files updated in cache".to_string(),
            stats.files_modified.to_string(),
        ],
        vec![
            "Files deleted from cache".to_string(),
            stats.files_deleted.to_string(),
        ],
        vec![
            "Total files in cache (final)".to_string(),
            stats.total_files.to_string(),
        ],
    ];

    let alignments = [ColumnAlignment::Left, ColumnAlignment::Right];
    writer.writeln_markdown("###", "Cache Statistics")?;
    writer.write_markdown_table(headers, &rows, Some(&alignments))?;
    println!();

    assert_eq!(
        image_info_map.len(),
        stats.total_files,
        "The number of entries in image_info_map does not match the total files in cache"
    );
    Ok(())
}

fn is_not_ignored(
    entry: &DirEntry,
    ignore_folders: &[PathBuf],
    writer: &ThreadSafeWriter,
) -> bool {
    let path = entry.path();
    let is_ignored = ignore_folders.iter().any(|ignored| path.starts_with(ignored));
    if is_ignored {
        let _ = writer.writeln_markdown("", &format!("ignoring: {:?}", path));
    }
    !is_ignored
}

fn collect_files(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<CollectedFiles, Box<dyn Error + Send + Sync>> {
    let ignore_folders = config.ignore_folders().unwrap_or(&[]);
    let mut collected_files = CollectedFiles::default();
    let mut markdown_files = Vec::new();

    // create the list of files to operate on
    for entry in WalkDir::new(config.obsidian_path())
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| is_not_ignored(e, ignore_folders, &writer))
    {
        let entry = entry?;
        let path = entry.path();

        if entry.file_type().is_file() {
            if path.file_name().and_then(|s| s.to_str()) == Some(".DS_Store") {
                continue;
            }

            match path.extension().and_then(|s| s.to_str()).map(|s| s.to_lowercase()) {
                Some(ext) if ext == "md" => markdown_files.push(path.to_path_buf()),
                Some(ext) if IMAGE_EXTENSIONS.contains(&ext.as_str()) => collected_files.image_files.push(path.to_path_buf()),
                _ => collected_files.other_files.push(path.to_path_buf()),
            }
        }
    }

    collected_files.markdown_files = scan_markdown_files(&markdown_files)?;
    Ok(collected_files)
}

fn scan_markdown_files(
    markdown_files: &[PathBuf],
) -> Result<HashMap<PathBuf, Vec<String>>, Box<dyn Error + Send + Sync>> {
    let extensions_pattern = IMAGE_EXTENSIONS.join("|");
    let image_regex = Arc::new(Regex::new(&format!(
        r"(!\[(?:[^\]]*)\]\([^)]+\)|!\[\[([^\]]+\.(?:{}))\]\])",
        extensions_pattern
    ))?);

    let image_references: HashMap<PathBuf, Vec<String>> = markdown_files
        .par_iter()
        .filter_map(|file_path| {
            scan_markdown_file(file_path, &image_regex).map(|references| (file_path.clone(), references)).ok()
        })
        .collect();

    Ok(image_references)
}

fn scan_markdown_file(file_path: &PathBuf, image_regex: &Arc<Regex>) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut file_references = Vec::new();

    for line in reader.lines() {
        let line = line?;
        for capture in image_regex.captures_iter(&line) {
            if let Some(reference) = capture.get(1) {
                file_references.push(reference.as_str().to_string());
            }
        }
    }

    Ok(file_references)
}

fn generate_markdown_image_reference_histogram(image_references: &HashMap<PathBuf, Vec<String>>) -> Vec<ImageReferenceCount> {
    let mut histogram = HashMap::new();

    for (path, references) in image_references {
        let count = references.len();
        histogram.entry(count)
            .or_insert_with(Vec::new)
            .push(path.clone());
    }

    let mut histogram_vec: Vec<ImageReferenceCount> = histogram
        .into_iter()
        .map(|(image_count, paths)| ImageReferenceCount {
            markdown_files: paths.len(),
            image_count,
            file_names: paths.into_iter()
                .filter_map(|path| {
                    path.file_stem()
                        .and_then(|stem| stem.to_str())
                        .map(|s| s.to_string())
                })
                .map(|stem| stem.to_string())
                .collect(),
        })
        .collect();

    histogram_vec.sort_by(|a, b| {
        a.image_count.cmp(&b.image_count)
            .then_with(|| b.markdown_files.cmp(&a.markdown_files))
    });

    histogram_vec
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

fn write_file_info(
    writer: &ThreadSafeWriter,
    collected_files: &CollectedFiles,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    println!();
    writer.writeln_markdown("##", "file counts")?;
    writer.writeln_markdown("###", &format!("markdown files: {}", collected_files.markdown_files.len()))?;
    writer.writeln_markdown("###", &format!("image files: {}", collected_files.image_files.len()))?;

    let image_counts = count_image_types(&collected_files.image_files);

    // Create headers and rows for the image counts table
    let headers = &["Extension", "Count"];
    let rows: Vec<Vec<String>> = image_counts
        .iter()
        .map(|(ext, count)| vec![format!(".{}", ext), count.to_string()])
        .collect();

    // Write the image counts as a markdown table
    writer.write_markdown_table(
        headers,
        &rows,
        Some(&[ColumnAlignment::Left, ColumnAlignment::Right]),
    )?;

    writer.writeln_markdown("###", &format!("other files: {}", collected_files.other_files.len()))?;

    if !collected_files.other_files.is_empty() {
        writer.writeln_markdown("####", "other files found:")?;
        for file in &collected_files.other_files {
            writer.writeln_markdown("- ", &format!("{}", file.display()))?;
        }
    }
    println!();
    Ok(())
}

fn write_scan_start(
    config: &ValidatedConfig,
    output: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    output.writeln_markdown("#", "scanning")?;
    output.writeln_markdown("## scan details", "")?;
    output.writeln_markdown("", &format!("scanning: {:?}", config.obsidian_path()))?;
    Ok(())
}

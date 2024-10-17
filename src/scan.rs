use sha2::{Digest, Sha256};

use crate::sha256_cache::{CacheFileStatus, Sha256Cache};
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::{constants::IMAGE_EXTENSIONS, validated_config::ValidatedConfig};
use regex::Regex;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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


pub fn scan_obsidian_folder(
    config: ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<HashMap<PathBuf, ImageInfo>, Box<dyn Error + Send + Sync>> {
    write_scan_start(&config, writer)?;

    let (markdown_files, image_files, other_files) = collect_files(&config, writer)?;
    let image_counts = count_image_types(&image_files);

    write_file_info(
        writer,
        &markdown_files,
        &image_files,
        &other_files,
        &image_counts,
    )?;

    let image_info_map = get_image_info_map(&config, &markdown_files, image_files, writer)?;

    Ok(image_info_map)
}

fn get_image_info_map(
    config: &ValidatedConfig,
    markdown_files: &Vec<PathBuf>,
    image_files: Vec<PathBuf>,
    writer: &ThreadSafeWriter,
) -> Result<HashMap<PathBuf, ImageInfo>, Box<dyn Error + Send + Sync>> {
    let cache_file_path = config
        .obsidian_path()
        .join(crate::constants::CACHE_FOLDER)
        .join("image_cache.json");
    let (mut cache, cache_file_status) = Sha256Cache::new(cache_file_path.clone())?;

    write_cache_file_info(writer, &cache_file_path, cache_file_status)?;

    let mut image_info_map = HashMap::new();
    let image_references_in_markdown_files = collect_image_references(&markdown_files)?;

    for image_path in image_files {
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

    let histogram = generate_image_reference_histogram(&image_references_in_markdown_files);
    write_image_reference_histogram(writer, &histogram)?;

    Ok(image_info_map)
}

fn write_image_reference_histogram(writer: &ThreadSafeWriter, histogram: &[ImageReferenceCount]) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln_markdown("###", "image reference histogram")?;

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

fn hash_file(path: &Path) -> Result<String, Box<dyn Error + Send + Sync>> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 1024];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>), Box<dyn Error + Send + Sync>> {
    let ignore_folders = config.ignore_folders().unwrap_or(&[]);

    let mut markdown_files = Vec::new();
    let mut image_files = Vec::new();
    let mut other_files = Vec::new();

    let walker = WalkDir::new(config.obsidian_path()).follow_links(true);

    for entry in walker.into_iter().filter_entry(|e| {
        let is_ignored = ignore_folders
            .iter()
            .any(|ignored| e.path().starts_with(ignored));
        if is_ignored && e.file_type().is_dir() {
            let _ = writer.writeln_markdown("", &format!("ignoring: {:?}", e.path()));
        }
        !is_ignored
    }) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let path = entry.into_path();
            if path.file_name().and_then(|s| s.to_str()) == Some(".DS_Store") {
                continue;
            }

            match path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase())
            {
                Some(ext) if ext == "md" => markdown_files.push(path),
                Some(ext) if IMAGE_EXTENSIONS.contains(&ext.as_str()) => image_files.push(path),
                _ => other_files.push(path),
            }
        }
    }

    Ok((markdown_files, image_files, other_files))
}

fn collect_image_references(
    markdown_files: &[PathBuf],
) -> Result<HashMap<PathBuf, Vec<String>>, Box<dyn Error + Send + Sync>> {
    let extensions_pattern = IMAGE_EXTENSIONS.join("|");
    let image_regex = Regex::new(&format!(
        r"!\[(?:[^\]]*)\]\(([^)]+)\)|!\[\[([^\]]+\.(?:{}))\]\]",
        extensions_pattern
    ))?;
    let mut image_references = HashMap::new();

    for file_path in markdown_files {
        let file = File::open(file_path)?;
        let reader = BufReader::new(file);
        let mut file_references = Vec::new();

        for line in reader.lines() {
            let line = line?;
            for capture in image_regex.captures_iter(&line) {
                if let Some(reference) = capture.get(1).or_else(|| capture.get(2)) {
                    file_references.push(reference.as_str().to_string());
                }
            }
        }

        image_references.insert(file_path.clone(), file_references);
    }

    Ok(image_references)
}

fn generate_image_reference_histogram(image_references: &HashMap<PathBuf, Vec<String>>) -> Vec<ImageReferenceCount> {
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
    markdown_files: &[PathBuf],
    image_files: &[PathBuf],
    other_files: &[PathBuf],
    image_counts: &[(String, usize)],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    println!();
    writer.writeln_markdown("##", "file counts")?;
    writer.writeln_markdown("###", &format!("markdown files: {}", markdown_files.len()))?;
    writer.writeln_markdown("###", &format!("image files: {}", image_files.len()))?;

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

    writer.writeln_markdown("###", &format!("other files: {}", other_files.len()))?;

    if !other_files.is_empty() {
        writer.writeln_markdown("####", "other files found:")?;
        for file in other_files {
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

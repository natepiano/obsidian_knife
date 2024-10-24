use crate::sha256_cache::{CacheFileStatus, Sha256Cache};
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::{constants::IMAGE_EXTENSIONS, frontmatter, validated_config::ValidatedConfig};

use rayon::prelude::*;

use crate::frontmatter::FrontMatter;
use regex::Regex;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::Arc;
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub hash: String,
    pub(crate) references: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WikilinkInfo {
    pub line: usize,
    pub line_text: String,
    pub search_text: String,
    pub replace_text: String,
}

#[derive(Default, Debug)]
pub struct MarkdownFileInfo {
    pub image_links: Vec<String>,
    pub frontmatter: Option<FrontMatter>,
    pub property_error: Option<String>,
    pub wikilinks: Vec<WikilinkInfo>,
}

#[derive(Default)]
pub struct CollectedFiles {
    pub markdown_files: HashMap<PathBuf, MarkdownFileInfo>,
    pub image_map: HashMap<PathBuf, ImageInfo>,
    pub other_files: Vec<PathBuf>,
}

pub fn scan_obsidian_folder(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<CollectedFiles, Box<dyn Error + Send + Sync>> {
    write_scan_start(&config, writer)?;

    let collected_files = collect_files(&config, writer)?;

    write_file_info(writer, &collected_files)?;

    Ok(collected_files)
}

fn get_image_info_map(
    config: &ValidatedConfig,
    collected_files: &CollectedFiles,
    image_files: &[PathBuf],
    writer: &ThreadSafeWriter,
) -> Result<HashMap<PathBuf, ImageInfo>, Box<dyn Error + Send + Sync>> {
    let cache_file_path = config
        .obsidian_path()
        .join(crate::constants::CACHE_FOLDER)
        .join("image_cache.json");
    let (mut cache, cache_file_status) = Sha256Cache::new(cache_file_path.clone())?;

    write_cache_file_info(writer, &cache_file_path, cache_file_status)?;

    let mut image_info_map = HashMap::new();

    for image_path in image_files {
        let (hash, _) = cache.get_or_update(&image_path)?;

        let image_file_name = image_path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default();

        let references: Vec<String> = collected_files
            .markdown_files
            .iter()
            .filter_map(|(markdown_path, file_info)| {
                if file_info
                    .image_links
                    .iter()
                    .any(|link| link.contains(image_file_name))
                {
                    Some(markdown_path.to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .collect();

        let image_info = ImageInfo { hash, references };

        image_info_map.insert(image_path.clone(), image_info);
    }

    cache.remove_non_existent_entries();
    cache.save()?;

    write_cache_contents_info(writer, &mut cache, &mut image_info_map)?;

    Ok(image_info_map)
}

fn write_cache_file_info(
    writer: &ThreadSafeWriter,
    cache_file_path: &PathBuf,
    cache_file_status: CacheFileStatus,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("##", "collecting image file info")?;

    match cache_file_status {
        CacheFileStatus::ReadFromCache => {
            writer.writeln("", &format!("reading from cache: {:?}", cache_file_path))?
        }
        CacheFileStatus::CreatedNewCache => writer.writeln(
            "",
            &format!(
                "cache file missing - creating new cache: {:?}",
                cache_file_path
            ),
        )?,
        CacheFileStatus::CacheCorrupted => writer.writeln(
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
    writer.writeln("###", "Cache Statistics")?;
    writer.write_markdown_table(headers, &rows, Some(&alignments))?;
    println!();

    assert_eq!(
        image_info_map.len(),
        stats.total_files,
        "The number of entries in image_info_map does not match the total files in cache"
    );
    Ok(())
}

fn is_not_ignored(entry: &DirEntry, ignore_folders: &[PathBuf], writer: &ThreadSafeWriter) -> bool {
    let path = entry.path();
    let is_ignored = ignore_folders
        .iter()
        .any(|ignored| path.starts_with(ignored));
    if is_ignored {
        let _ = writer.writeln("", &format!("ignoring: {:?}", path));
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
    let mut image_files = Vec::new();

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

            match path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase())
            {
                Some(ext) if ext == "md" => markdown_files.push(path.to_path_buf()),
                Some(ext) if IMAGE_EXTENSIONS.contains(&ext.as_str()) => {
                    image_files.push(path.to_path_buf())
                }
                _ => collected_files.other_files.push(path.to_path_buf()),
            }
        }
    }

    collected_files.markdown_files = scan_markdown_files(&markdown_files, config)?;

    collected_files.image_map =
        get_image_info_map(&config, &collected_files, &image_files, &writer)?;

    Ok(collected_files)
}

fn scan_markdown_files(
    markdown_files: &[PathBuf],
    config: &ValidatedConfig,
) -> Result<HashMap<PathBuf, MarkdownFileInfo>, Box<dyn Error + Send + Sync>> {
    let extensions_pattern = IMAGE_EXTENSIONS.join("|");
    let image_regex = Arc::new(Regex::new(&format!(
        r"(!\[(?:[^\]]*)\]\([^)]+\)|!\[\[([^\]]+\.(?:{}))(?:\|[^\]]+)?\]\])",
        extensions_pattern
    ))?);
    let wikilink_regex = Arc::new(Regex::new(r"\[\[([^]]+)]]")?);

    let simplify_patterns = config.simplify_wikilinks().unwrap_or_default();
    let ignore_patterns = config.ignore_text().unwrap_or_default();

    let markdown_info: HashMap<PathBuf, MarkdownFileInfo> = markdown_files
        .par_iter()
        .filter_map(|file_path| {
            scan_markdown_file(
                file_path,
                &image_regex,
                &wikilink_regex,
                &simplify_patterns,
                &ignore_patterns,
            )
            .map(|info| (file_path.clone(), info))
            .ok()
        })
        .collect();

    Ok(markdown_info)
}

fn scan_markdown_file(
    file_path: &PathBuf,
    image_regex: &Arc<Regex>,
    wikilink_regex: &Arc<Regex>,
    simplify_patterns: &[String],
    ignore_patterns: &[String],
) -> Result<MarkdownFileInfo, Box<dyn Error + Send + Sync>> {
    let content = fs::read_to_string(file_path)?;

    let (frontmatter, property_error) = match frontmatter::deserialize_frontmatter(&content) {
        Ok(fm) => (Some(fm), None),
        Err(e) => (None, Some(e.to_string())),
    };

    let mut file_info = MarkdownFileInfo::default();
    file_info.frontmatter = frontmatter;
    file_info.property_error = property_error;

    let reader = BufReader::new(content.as_bytes());

    for (line_number, line) in reader.lines().enumerate() {
        let line = line?;

        collect_image_reference(image_regex, &mut file_info, &line);

        collect_wikilink_info(
            wikilink_regex,
            simplify_patterns,
            ignore_patterns,
            &mut file_info,
            line_number,
            &line,
        );
    }

    Ok(file_info)
}

fn collect_wikilink_info(
    wikilink_regex: &Arc<Regex>,
    simplify_patterns: &[String],
    ignore_patterns: &[String],
    file_info: &mut MarkdownFileInfo,
    line_number: usize,
    line: &str,
) {
    let mut rendered_line = String::new();
    let mut wikilink_positions = Vec::new();
    let mut last_end = 0;

    // Step 1: Render all wikilinks and save their positions
    for capture in wikilink_regex.captures_iter(line) {
        if let (Some(whole_match), Some(inner_content)) = (capture.get(0), capture.get(1)) {
            rendered_line.push_str(&line[last_end..whole_match.start()]);
            let start = rendered_line.len();
            let rendered = render_wikilink(inner_content.as_str());
            rendered_line.push_str(&rendered);
            let end = rendered_line.len();
            wikilink_positions.push((start, end, whole_match.start(), whole_match.end()));
            last_end = whole_match.end();
        }
    }
    rendered_line.push_str(&line[last_end..]);

    // Step 2 & 3: Check for exact matches and find overlapping wikilinks
    for pattern in simplify_patterns {
        let mut start_index = 0;
        while let Some(match_start) = rendered_line[start_index..].find(pattern) {
            let match_start = start_index + match_start;
            let match_end = match_start + pattern.len();

            // Check if the match is within an ignore pattern
            let should_ignore = ignore_patterns.iter().any(|ignore_pattern| {
                let ignore_regex =
                    Regex::new(&format!(r"{}.*", regex::escape(ignore_pattern))).unwrap();
                let ignore_match = ignore_regex.is_match(&rendered_line[match_start..]);
                ignore_match
            });

            if !should_ignore {
                let overlapping_wikilinks: Vec<_> = wikilink_positions
                    .iter()
                    .filter(|&&(start, end, _, _)| {
                        (start <= match_start && end > match_start)
                            || (start < match_end && end >= match_end)
                            || (start >= match_start && end <= match_end)
                    })
                    .collect();

                if !overlapping_wikilinks.is_empty() {
                    // Step 4 & 5: Create replacement
                    let original_start = if match_start < overlapping_wikilinks[0].0 {
                        match_start - (overlapping_wikilinks[0].0 - overlapping_wikilinks[0].2)
                    } else {
                        overlapping_wikilinks[0].2
                    };
                    let original_end = if match_end > overlapping_wikilinks.last().unwrap().1 {
                        overlapping_wikilinks.last().unwrap().3
                            + (match_end - overlapping_wikilinks.last().unwrap().1)
                    } else {
                        overlapping_wikilinks.last().unwrap().3
                    };

                    let search_text = line[original_start..original_end].to_string();
                    let replace_text = pattern.to_string();

                    file_info.wikilinks.push(WikilinkInfo {
                        line: line_number + 1,
                        line_text: line.to_string(),
                        search_text,
                        replace_text,
                    });
                }
            }
            start_index = match_end;
        }
    }
}

fn render_wikilink(wikilink: &str) -> String {
    wikilink.split('|').last().unwrap_or(wikilink).to_string()
}

fn collect_image_reference(
    image_regex: &Arc<Regex>,
    file_info: &mut MarkdownFileInfo,
    line: &String,
) {
    // Collect image references
    for capture in image_regex.captures_iter(&line) {
        if let Some(reference) = capture.get(0) {
            let reference_string = reference.as_str().to_string();
            file_info.image_links.push(reference_string);
        }
    }
}

fn count_image_types(image_map: &HashMap<PathBuf, ImageInfo>) -> Vec<(String, usize)> {
    let counts: HashMap<String, usize> = image_map
        .keys()
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
    writer.writeln("##", "file counts")?;
    writer.writeln(
        "###",
        &format!("markdown files: {}", collected_files.markdown_files.len()),
    )?;
    writer.writeln(
        "###",
        &format!("image files: {}", collected_files.image_map.len()),
    )?;

    let image_counts = count_image_types(&collected_files.image_map);

    // Create headers and rows for the image counts table
    let headers = &["Extension", "Count"];
    let rows: Vec<Vec<String>> = image_counts
        .iter()
        .map(|(ext, count)| vec![format!(".{}", ext), count.to_string()])
        .collect();

    // Write the image counts as a Markdown table
    writer.write_markdown_table(
        headers,
        &rows,
        Some(&[ColumnAlignment::Left, ColumnAlignment::Right]),
    )?;

    writer.writeln(
        "###",
        &format!("other files: {}", collected_files.other_files.len()),
    )?;

    if !collected_files.other_files.is_empty() {
        writer.writeln("####", "other files found:")?;
        for file in &collected_files.other_files {
            writer.writeln("- ", &format!("{}", file.display()))?;
        }
    }
    println!();
    Ok(())
}

fn write_scan_start(
    config: &ValidatedConfig,
    output: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    output.writeln("#", "scanning")?;
    output.writeln("## scan details", "")?;
    output.writeln("", &format!("scanning: {:?}", config.obsidian_path()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_collect_wikilink_info() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create a test markdown file with various wikilink scenarios
        let content = "[[Ed Barnes|Ed]]: music reco\n[[Éd Bârnes|Éd]]: mûsîc récô\nloves [[Bob]] [[Rock]] yeah\nBob [[Rock]] is cool";

        let mut file = File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Set up test environment
        let wikilink_regex = Arc::new(Regex::new(r"\[\[([^]]+)]]").unwrap());
        let simplify_patterns = vec!["Ed:".to_string(), "Éd:".to_string(), "Bob Rock".to_string()];
        let ignore_patterns = vec![]; // Add an empty ignore patterns vector
        let mut file_info = MarkdownFileInfo::default();

        // Read the file and process each line
        let file_content = fs::read_to_string(&file_path).unwrap();
        for (line_number, line) in file_content.lines().enumerate() {
            collect_wikilink_info(
                &wikilink_regex,
                &simplify_patterns,
                &ignore_patterns, // Pass the ignore patterns
                &mut file_info,
                line_number,
                line,
            );
        }

        // Assertions
        assert_eq!(file_info.wikilinks.len(), 4);

        // Check the first wikilink
        assert_eq!(file_info.wikilinks[0].line, 1);
        assert_eq!(file_info.wikilinks[0].search_text, "[[Ed Barnes|Ed]]:");
        assert_eq!(file_info.wikilinks[0].replace_text, "Ed:");

        // Check the second wikilink (with UTF-8 characters)
        assert_eq!(file_info.wikilinks[1].line, 2);
        assert_eq!(file_info.wikilinks[1].search_text, "[[Éd Bârnes|Éd]]:");
        assert_eq!(file_info.wikilinks[1].replace_text, "Éd:");

        // Check the third wikilink (adjacent wikilinks)
        assert_eq!(file_info.wikilinks[2].line, 3);
        assert_eq!(file_info.wikilinks[2].search_text, "[[Bob]] [[Rock]]");
        assert_eq!(file_info.wikilinks[2].replace_text, "Bob Rock");

        // Check the fourth wikilink (partial wikilink)
        assert_eq!(file_info.wikilinks[3].line, 4);
        assert_eq!(file_info.wikilinks[3].search_text, "Bob [[Rock]]");
        assert_eq!(file_info.wikilinks[3].replace_text, "Bob Rock");
    }

    #[test]
    fn test_collect_wikilink_info_with_ignore() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create a test markdown file with various wikilink scenarios
        let content =
            "[[Ed Barnes|Ed]]: music reco:\n[[Ed Barnes|Ed]]: is cool\nEd: something else";

        let mut file = File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Set up test environment
        let wikilink_regex = Arc::new(Regex::new(r"\[\[([^]]+)]]").unwrap());
        let simplify_patterns = vec!["Ed:".to_string()];
        let ignore_patterns = vec!["Ed: music reco:".to_string()];
        let mut file_info = MarkdownFileInfo::default();

        // Read the file and process each line
        let file_content = fs::read_to_string(&file_path).unwrap();
        for (line_number, line) in file_content.lines().enumerate() {
            collect_wikilink_info(
                &wikilink_regex,
                &simplify_patterns,
                &ignore_patterns,
                &mut file_info,
                line_number,
                line,
            );
        }

        // Assertions
        assert_eq!(
            file_info.wikilinks.len(),
            1,
            "Expected 1 wikilink, found {}",
            file_info.wikilinks.len()
        );

        // Check the wikilink (should be simplified, not ignored)
        assert_eq!(file_info.wikilinks[0].line, 2);
        assert_eq!(file_info.wikilinks[0].search_text, "[[Ed Barnes|Ed]]:");
        assert_eq!(file_info.wikilinks[0].replace_text, "Ed:");

        // The "Ed: something else" shouldn't be included as it's not a wikilink
    }
}

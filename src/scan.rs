use crate::sha256_cache::{CacheFileStatus, Sha256Cache};
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::{
    constants::IMAGE_EXTENSIONS, frontmatter, validated_config::ValidatedConfig, wikilink,
    CACHE_FILE, LEVEL3,
};

use rayon::prelude::*;

use crate::constants::{LEVEL1, LEVEL2};
use crate::frontmatter::FrontMatter;
use crate::wikilink::CompiledWikilink;
use regex::Regex;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub hash: String,
    pub(crate) references: Vec<String>,
}


#[derive(Debug)]
pub struct MarkdownFileInfo {
    pub frontmatter: Option<FrontMatter>,
    pub image_links: Vec<String>,
    pub property_error: Option<String>,
}

impl MarkdownFileInfo {
    pub fn new() -> Self {
        MarkdownFileInfo {
            frontmatter: None,
            image_links: Vec::new(),
            property_error: None,
        }
    }
}

#[derive(Default)]
pub struct ObsidianRepositoryInfo {
    pub all_wikilinks: HashSet<CompiledWikilink>, // New field for all unique wikilinks
    pub markdown_files: HashMap<PathBuf, MarkdownFileInfo>,
    pub image_map: HashMap<PathBuf, ImageInfo>,
    pub other_files: Vec<PathBuf>,
}

pub fn scan_obsidian_folder(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<ObsidianRepositoryInfo, Box<dyn Error + Send + Sync>> {
    write_scan_start(&config, writer)?;

    let obsidian_repository_info = scan_folders(&config, writer)?;

    write_file_info(writer, &obsidian_repository_info)?;

    Ok(obsidian_repository_info)
}

fn get_image_info_map(
    config: &ValidatedConfig,
    markdown_files: &HashMap<PathBuf, MarkdownFileInfo>,
    image_files: &[PathBuf],
    writer: &ThreadSafeWriter,
) -> Result<HashMap<PathBuf, ImageInfo>, Box<dyn Error + Send + Sync>> {
    let cache_file_path = config
        .obsidian_path()
        .join(crate::constants::CACHE_FOLDER)
        .join(CACHE_FILE);
    let (mut cache, cache_file_status) = Sha256Cache::new(cache_file_path.clone())?;

    write_cache_file_info(writer, &cache_file_path, cache_file_status)?;

    let mut image_info_map = HashMap::new();

    for image_path in image_files {
        let (hash, _) = cache.get_or_update(&image_path)?;

        let image_file_name = image_path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default();

        let references: Vec<String> = markdown_files
            .par_iter()
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
    writer.writeln(LEVEL2, "collecting image file info")?;

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
    writer.writeln(LEVEL3, "Cache Statistics")?;
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

fn scan_folders(
    config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<ObsidianRepositoryInfo, Box<dyn Error + Send + Sync>> {
    let ignore_folders = config.ignore_folders().unwrap_or(&[]);
    let mut obsidian_repository_info = ObsidianRepositoryInfo::default();
    let mut markdown_files = Vec::new();
    let mut image_files = Vec::new();

    // Create the list of files to operate on
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
                _ => obsidian_repository_info
                    .other_files
                    .push(path.to_path_buf()),
            }
        }
    }

    // Get markdown files info and accumulate all_wikilinks from scan_markdown_files
    let (markdown_info, all_wikilinks) = scan_markdown_files(&markdown_files)?;
    obsidian_repository_info.markdown_files = markdown_info;
    obsidian_repository_info.all_wikilinks = all_wikilinks;

    // Process image info
    obsidian_repository_info.image_map = get_image_info_map(
        &config,
        &obsidian_repository_info.markdown_files,
        &image_files,
        &writer,
    )?;

    Ok(obsidian_repository_info)
}

fn scan_markdown_files(
    markdown_files: &[PathBuf],
) -> Result<
    (
        HashMap<PathBuf, MarkdownFileInfo>,
        HashSet<CompiledWikilink>,
    ),
    Box<dyn Error + Send + Sync>,
> {
    let extensions_pattern = IMAGE_EXTENSIONS.join("|");
    let image_regex = Arc::new(Regex::new(&format!(
        r"(!\[(?:[^\]]*)\]\([^)]+\)|!\[\[([^\]]+\.(?:{}))(?:\|[^\]]+)?\]\])",
        extensions_pattern
    ))?);

    // Use Arc<Mutex<...>> for safe shared collection
    let markdown_info = Arc::new(Mutex::new(HashMap::new()));
    let all_wikilinks = Arc::new(Mutex::new(HashSet::new()));

    markdown_files.par_iter().for_each(|file_path| {
        if let Ok((file_info, wikilinks)) = scan_markdown_file(
            file_path,
            &image_regex,
        ) {
            // Collect results with locking to avoid race conditions
            markdown_info
                .lock()
                .unwrap()
                .insert(file_path.clone(), file_info);
            all_wikilinks.lock().unwrap().extend(wikilinks);
        }
    });

    // Extract data from Arc<Mutex<...>>
    let markdown_info = Arc::try_unwrap(markdown_info)
        .unwrap()
        .into_inner()
        .unwrap();
    let all_wikilinks = Arc::try_unwrap(all_wikilinks)
        .unwrap()
        .into_inner()
        .unwrap();

    Ok((markdown_info, all_wikilinks))
}

fn scan_markdown_file(
    file_path: &PathBuf,
    image_regex: &Arc<Regex>,
) -> Result<(MarkdownFileInfo, HashSet<CompiledWikilink>), Box<dyn Error + Send + Sync>> {
    let content = fs::read_to_string(file_path)?;

    let (frontmatter, property_error) = match frontmatter::deserialize_frontmatter(&content) {
        Ok(fm) => (Some(fm), None),
        Err(e) => (None, Some(e.to_string())),
    };

    let mut file_info = MarkdownFileInfo::new();
    file_info.frontmatter = frontmatter;
    file_info.property_error = property_error;

    // Get filename for wikilink collection
    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    // Collect wikilinks but return them separately
    let wikilinks = wikilink::collect_all_wikilinks(&content, &file_info.frontmatter, filename);

    let reader = BufReader::new(content.as_bytes());

    for (_, line) in reader.lines().enumerate() {
        let line = line?;
        collect_image_reference(image_regex, &mut file_info, &line);
    }

    Ok((file_info, wikilinks))
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
    collected_files: &ObsidianRepositoryInfo,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    println!();
    writer.writeln(LEVEL2, "file counts")?;
    writer.writeln(
        LEVEL3,
        &format!("markdown files: {}", collected_files.markdown_files.len()),
    )?;
    writer.writeln(
        LEVEL3,
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
        LEVEL3,
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
    output.writeln(LEVEL1, "scanning")?;
    output.writeln(LEVEL2, "scan details")?;
    output.writeln("", &format!("scanning: {:?}", config.obsidian_path()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_scan_markdown_file_wikilink_collection() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_note.md");

        // Create test content with different types of wikilinks
        let content = r#"---
aliases:
  - "Alias One"
  - "Second Alias"
---
# Test Note

Here's a [[Simple Link]] and [[Target Page|Display Text]].
Also linking to [[Alias One]] which is defined in frontmatter.
"#;

        // Write content to temporary file
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // Test patterns
        let image_regex = Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap());

        // Scan the markdown file
        let (_file_info, wikilinks) = scan_markdown_file(
            &file_path,
            &image_regex,
        )
        .unwrap();

        // Collect unique target-display pairs
        let wikilink_pairs: HashSet<(String, String)> = wikilinks
            .iter()
            .map(|w| (w.wikilink.target.clone(), w.wikilink.display_text.clone()))
            .collect();

        // Check for the expected unique wikilinks
        assert!(
            wikilink_pairs.contains(&("test_note".to_string(), "test_note".to_string())),
            "Should contain filename-based wikilink"
        );
        assert!(
            wikilink_pairs.contains(&("test_note".to_string(), "Alias One".to_string())),
            "Should contain first alias as [[test_note|Alias One]]"
        );
        assert!(
            wikilink_pairs.contains(&("test_note".to_string(), "Second Alias".to_string())),
            "Should contain second alias as [[test_note|Second Alias]]"
        );
        assert!(
            wikilink_pairs.contains(&("Simple Link".to_string(), "Simple Link".to_string())),
            "Should contain simple wikilink"
        );
        assert!(
            wikilink_pairs.contains(&("Target Page".to_string(), "Display Text".to_string())),
            "Should contain aliased display text"
        );

        // Verify total count of unique wikilinks
        assert_eq!(
            wikilink_pairs.len(),
            5,
            "Should have collected all unique wikilinks"
        );
    }

    #[test]
    fn test_scan_folders_wikilink_collection() {
        let temp_dir = TempDir::new().unwrap();

        // Create test files with different wikilinks
        let files = [
            (
                "note1.md",
                r#"---
aliases:
  - "Alias One"
---
# Note 1
[[Simple Link]]"#,
            ),
            (
                "note2.md",
                r#"---
aliases:
  - "Alias Two"
---
# Note 2
[[Target|Display Text]]
[[Simple Link]]"#,
            ),
        ];

        // Create the files in the temp directory
        for (filename, content) in files.iter() {
            let file_path = temp_dir.path().join(filename);
            let mut file = File::create(&file_path).unwrap();
            write!(file, "{}", content).unwrap();
        }

        // Create minimal validated config
        let config = ValidatedConfig::new(
            false,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        // Create writer for testing
        let writer = ThreadSafeWriter::new(temp_dir.path()).unwrap();

        // Scan the folders
        let repo_info = scan_folders(&config, &writer).unwrap();

        // Filter for .md files only and exclude "obsidian knife output" explicitly
        let wikilinks: HashSet<String> = repo_info
            .markdown_files
            .keys()
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
            .flat_map(|file_path| {
                let (_, file_wikilinks) = scan_markdown_file(
                    file_path,
                    &Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap()),
                )
                .unwrap();
                file_wikilinks.into_iter().map(|w| w.wikilink.display_text)
            })
            .filter(|link| link != "obsidian knife output") // Exclude "obsidian knife output"
            .collect();

        // Verify expected wikilinks are present
        assert!(wikilinks.contains("note1"), "Should contain first filename");
        assert!(
            wikilinks.contains("note2"),
            "Should contain second filename"
        );
        assert!(
            wikilinks.contains("Alias One"),
            "Should contain first alias"
        );
        assert!(
            wikilinks.contains("Alias Two"),
            "Should contain second alias"
        );
        assert!(
            wikilinks.contains("Simple Link"),
            "Should contain simple link"
        );
        assert!(
            wikilinks.contains("Display Text"),
            "Should contain display text from alias"
        );

        // Verify total count
        assert_eq!(
            wikilinks.len(),
            6,
            "Should have collected all unique wikilinks"
        );
    }

    #[test]
    fn test_parallel_image_reference_collection() {
        use rayon::prelude::*;

        let temp_dir = TempDir::new().unwrap();
        let mut markdown_files = HashMap::new();

        // Create test files
        for i in 0..100 {
            let filename = format!("note{}.md", i);
            let content = format!("![image{}](test{}.jpg)\n![shared](common.jpg)", i, i);
            let file_path = temp_dir.path().join(&filename);
            let mut info = MarkdownFileInfo::new();
            info.image_links = content.split('\n').map(|s| s.to_string()).collect();
            markdown_files.insert(file_path, info);
        }

        // Common filter logic
        fn has_common_image(info: &MarkdownFileInfo) -> bool {
            info.image_links
                .iter()
                .any(|link| link.contains("common.jpg"))
        }

        // Helper functions using shared filter
        fn process_parallel(files: &HashMap<PathBuf, MarkdownFileInfo>) -> Vec<PathBuf> {
            files
                .par_iter()
                .filter_map(|(path, info)| has_common_image(info).then(|| path.clone()))
                .collect()
        }

        fn process_sequential(files: &HashMap<PathBuf, MarkdownFileInfo>) -> Vec<PathBuf> {
            files
                .iter()
                .filter_map(|(path, info)| has_common_image(info).then(|| path.clone()))
                .collect()
        }

        // Test parallel processing
        let start_parallel = std::time::Instant::now();
        let parallel_results = process_parallel(&markdown_files);
        let parallel_time = start_parallel.elapsed();

        // Test sequential processing
        let start_sequential = std::time::Instant::now();
        let sequential_results = process_sequential(&markdown_files);
        let sequential_time = start_sequential.elapsed();

        // Verify results
        assert_eq!(parallel_results.len(), sequential_results.len());
        assert_eq!(
            parallel_results.len(),
            100,
            "Should find common image in all files"
        );

        println!(
            "Parallel: {:?}, Sequential: {:?}",
            parallel_time, sequential_time
        );
    }
}

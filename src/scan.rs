use crate::sha256_cache::{CacheFileStatus, Sha256Cache};
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::{constants::IMAGE_EXTENSIONS, frontmatter, validated_config::ValidatedConfig, wikilink, CACHE_FILE, LEVEL3};

use rayon::prelude::*;

use crate::constants::{LEVEL1, LEVEL2};
use crate::frontmatter::FrontMatter;
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
use crate::wikilink::CompiledWikilink;

#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub hash: String,
    pub(crate) references: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SimplifyWikilinkInfo {
    pub line: usize,
    pub line_text: String,
    pub search_text: String,
    pub replace_text: String,
}

#[derive(Debug)]
pub struct MarkdownFileInfo {
    pub frontmatter: Option<FrontMatter>,
    pub image_links: Vec<String>,
    pub property_error: Option<String>,
    pub simplify_wikilink_info: Vec<SimplifyWikilinkInfo>,  // Existing field for simplification targets
}

impl MarkdownFileInfo {
    pub fn new() -> Self {
        MarkdownFileInfo {
            frontmatter: None,
            image_links: Vec::new(),
            property_error: None,
            simplify_wikilink_info: Vec::new(),
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
    collected_files: &ObsidianRepositoryInfo,
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
                _ => obsidian_repository_info.other_files.push(path.to_path_buf()),
            }
        }
    }

    // Get markdown files info and accumulate all_wikilinks from scan_markdown_files
    let (markdown_info, all_wikilinks) = scan_markdown_files(&markdown_files, config)?;
    obsidian_repository_info.markdown_files = markdown_info;
    obsidian_repository_info.all_wikilinks = all_wikilinks;

    // Process image info
    obsidian_repository_info.image_map =
        get_image_info_map(&config, &obsidian_repository_info, &image_files, &writer)?;

    Ok(obsidian_repository_info)
}

fn scan_markdown_files(
    markdown_files: &[PathBuf],
    config: &ValidatedConfig,
) -> Result<(HashMap<PathBuf, MarkdownFileInfo>, HashSet<CompiledWikilink>), Box<dyn Error + Send + Sync>> {
    let extensions_pattern = IMAGE_EXTENSIONS.join("|");
    let image_regex = Arc::new(Regex::new(&format!(
        r"(!\[(?:[^\]]*)\]\([^)]+\)|!\[\[([^\]]+\.(?:{}))(?:\|[^\]]+)?\]\])",
        extensions_pattern
    ))?);

    let simplify_patterns = config.simplify_wikilinks().unwrap_or_default();
    let ignore_patterns = config.ignore_text().unwrap_or_default();

    // Use Arc<Mutex<...>> for safe shared collection
    let markdown_info = Arc::new(Mutex::new(HashMap::new()));
    let all_wikilinks = Arc::new(Mutex::new(HashSet::new()));

    markdown_files.par_iter().for_each(|file_path| {
        if let Ok((file_info, wikilinks)) = scan_markdown_file(
            file_path,
            &image_regex,
            &simplify_patterns,
            &ignore_patterns,
        ) {
            // Collect results with locking to avoid race conditions
            markdown_info.lock().unwrap().insert(file_path.clone(), file_info);
            all_wikilinks.lock().unwrap().extend(wikilinks);
        }
    });

    // Extract data from Arc<Mutex<...>>
    let markdown_info = Arc::try_unwrap(markdown_info).unwrap().into_inner().unwrap();
    let all_wikilinks = Arc::try_unwrap(all_wikilinks).unwrap().into_inner().unwrap();

    Ok((markdown_info, all_wikilinks))
}

fn scan_markdown_file(
    file_path: &PathBuf,
    image_regex: &Arc<Regex>,
    simplify_patterns: &[String],
    ignore_patterns: &[String],
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

    for (line_number, line) in reader.lines().enumerate() {
        let line = line?;

        collect_image_reference(image_regex, &mut file_info, &line);

        collect_simplify_wikilink_info(
            simplify_patterns,
            ignore_patterns,
            &mut file_info,
            line_number,
            &line,
        );
    }

    Ok((file_info, wikilinks))
}

fn collect_simplify_wikilink_info(
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
    for wikilink_match in wikilink::find_wikilinks_in_line(line) {
        rendered_line.push_str(&line[last_end..wikilink_match.start()]);
        let start = rendered_line.len();
        if let Some(wikilink) = wikilink::parse_wikilink(&line[wikilink_match.start()..wikilink_match.end()]) {
            let rendered = render_wikilink(&wikilink.display_text);
            rendered_line.push_str(&rendered);
            let end = rendered_line.len();
            wikilink_positions.push((start, end, wikilink_match.start(), wikilink_match.end()));
        }
        last_end = wikilink_match.end();
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

                    file_info.simplify_wikilink_info.push(SimplifyWikilinkInfo {
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
        let simplify_patterns = vec!["Ed:".to_string(), "Éd:".to_string(), "Bob Rock".to_string()];
        let ignore_patterns = vec![]; // Add an empty ignore patterns vector
        let mut file_info = MarkdownFileInfo::new();

        // Read the file and process each line
        let file_content = fs::read_to_string(&file_path).unwrap();
        for (line_number, line) in file_content.lines().enumerate() {
            collect_simplify_wikilink_info(
                &simplify_patterns,
                &ignore_patterns, // Pass the ignore patterns
                &mut file_info,
                line_number,
                line,
            );
        }

        // Assertions
        assert_eq!(file_info.simplify_wikilink_info.len(), 4);

        // Check the first wikilink
        assert_eq!(file_info.simplify_wikilink_info[0].line, 1);
        assert_eq!(file_info.simplify_wikilink_info[0].search_text, "[[Ed Barnes|Ed]]:");
        assert_eq!(file_info.simplify_wikilink_info[0].replace_text, "Ed:");

        // Check the second wikilink (with UTF-8 characters)
        assert_eq!(file_info.simplify_wikilink_info[1].line, 2);
        assert_eq!(file_info.simplify_wikilink_info[1].search_text, "[[Éd Bârnes|Éd]]:");
        assert_eq!(file_info.simplify_wikilink_info[1].replace_text, "Éd:");

        // Check the third wikilink (adjacent wikilinks)
        assert_eq!(file_info.simplify_wikilink_info[2].line, 3);
        assert_eq!(file_info.simplify_wikilink_info[2].search_text, "[[Bob]] [[Rock]]");
        assert_eq!(file_info.simplify_wikilink_info[2].replace_text, "Bob Rock");

        // Check the fourth wikilink (partial wikilink)
        assert_eq!(file_info.simplify_wikilink_info[3].line, 4);
        assert_eq!(file_info.simplify_wikilink_info[3].search_text, "Bob [[Rock]]");
        assert_eq!(file_info.simplify_wikilink_info[3].replace_text, "Bob Rock");
    }

    #[test]
    fn test_collect_simplify_wikilink_info_with_ignore() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create a test markdown file with various wikilink scenarios
        let content =
            "[[Ed Barnes|Ed]]: music reco:\n[[Ed Barnes|Ed]]: is cool\nEd: something else";

        let mut file = File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Set up test environment
        let simplify_patterns = vec!["Ed:".to_string()];
        let ignore_patterns = vec!["Ed: music reco:".to_string()];
        let mut file_info = MarkdownFileInfo::new();

        // Read the file and process each line
        let file_content = fs::read_to_string(&file_path).unwrap();
        for (line_number, line) in file_content.lines().enumerate() {
            collect_simplify_wikilink_info(
                &simplify_patterns,
                &ignore_patterns,
                &mut file_info,
                line_number,
                line,
            );
        }

        // Assertions
        assert_eq!(
            file_info.simplify_wikilink_info.len(),
            1,
            "Expected 1 wikilink, found {}",
            file_info.simplify_wikilink_info.len()
        );

        // Check the wikilink (should be simplified, not ignored)
        assert_eq!(file_info.simplify_wikilink_info[0].line, 2);
        assert_eq!(file_info.simplify_wikilink_info[0].search_text, "[[Ed Barnes|Ed]]:");
        assert_eq!(file_info.simplify_wikilink_info[0].replace_text, "Ed:");

        // The "Ed: something else" shouldn't be included as it's not a wikilink
    }

    #[test]
    fn test_collect_simplify_wikilink_info_with_aliases() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create a test markdown file with aliased wikilinks
        let content = "Here is a [[Simple Link]] and a [[Target|Aliased Link]] together";
        fs::write(&file_path, content).unwrap();

        let mut file_info = MarkdownFileInfo::new();
        let simplify_patterns = vec!["Simple".to_string(), "Aliased".to_string()];
        let ignore_patterns = vec![];

        collect_simplify_wikilink_info(
            &simplify_patterns,
            &ignore_patterns,
            &mut file_info,
            1,
            content,
        );

        assert_eq!(file_info.simplify_wikilink_info.len(), 2);
        assert!(file_info.simplify_wikilink_info.iter().any(|info|
            info.search_text.contains("Simple Link")));
        assert!(file_info.simplify_wikilink_info.iter().any(|info|
            info.search_text.contains("Target|Aliased Link")));
    }

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
        let simplify_patterns: Vec<String> = vec![];
        let ignore_patterns: Vec<String> = vec![];
        let image_regex = Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap());

        // Scan the markdown file
        let (file_info, wikilinks) = scan_markdown_file(
            &file_path,
            &image_regex,
            &simplify_patterns,
            &ignore_patterns,
        )
            .unwrap();

        // Collect display texts for verification
        let wikilink_texts: Vec<String> = wikilinks
            .iter()
            .map(|w| w.wikilink.display_text.clone())
            .collect();

        // Print collected wikilinks for debugging
        println!("Collected wikilinks: {:?}", wikilink_texts);

        // Check for the expected wikilinks
        assert!(wikilink_texts.contains(&"test_note".to_string()), "Should contain filename-based wikilink");
        assert!(wikilink_texts.contains(&"Alias One".to_string()), "Should contain first alias");
        assert!(wikilink_texts.contains(&"Second Alias".to_string()), "Should contain second alias");
        assert!(wikilink_texts.contains(&"Simple Link".to_string()), "Should contain simple wikilink");
        assert!(wikilink_texts.contains(&"Display Text".to_string()), "Should contain aliased display text");

        // Verify total count
        assert_eq!(wikilink_texts.len(), 5, "Should have collected all unique wikilinks");
    }

    #[test]
    fn test_scan_folders_wikilink_collection() {
        let temp_dir = TempDir::new().unwrap();

        // Create test files with different wikilinks
        let files = [
            ("note1.md", r#"---
aliases:
  - "Alias One"
---
# Note 1
[[Simple Link]]"#),
            ("note2.md", r#"---
aliases:
  - "Alias Two"
---
# Note 2
[[Target|Display Text]]
[[Simple Link]]"#),
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
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
            None,
        );

        // Create writer for testing
        let writer = ThreadSafeWriter::new(temp_dir.path()).unwrap();

        // Scan the folders
        let repo_info = scan_folders(&config, &writer).unwrap();

        // Filter for .md files only and exclude "obsidian knife output" explicitly
        let wikilinks: HashSet<String> = repo_info.markdown_files.keys()
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
            .flat_map(|file_path| {
                let (_, file_wikilinks) = scan_markdown_file(
                    file_path,
                    &Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap()),
                    &[],
                    &[],
                )
                    .unwrap();
                file_wikilinks.into_iter()
                    .map(|w| w.wikilink.display_text)
            })
            .filter(|link| link != "obsidian knife output") // Exclude "obsidian knife output"
            .collect();

        // Print all collected wikilinks for debugging
        println!("Collected wikilinks: {:?}", wikilinks);

        // Verify expected wikilinks are present
        assert!(wikilinks.contains("note1"), "Should contain first filename");
        assert!(wikilinks.contains("note2"), "Should contain second filename");
        assert!(wikilinks.contains("Alias One"), "Should contain first alias");
        assert!(wikilinks.contains("Alias Two"), "Should contain second alias");
        assert!(wikilinks.contains("Simple Link"), "Should contain simple link");
        assert!(wikilinks.contains("Display Text"), "Should contain display text from alias");

        // Verify total count
        assert_eq!(wikilinks.len(), 6, "Should have collected all unique wikilinks");
    }


}

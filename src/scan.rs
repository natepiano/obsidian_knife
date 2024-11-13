use crate::regex_utils::build_case_insensitive_word_finder;
use crate::{
    constants::*, frontmatter::*, sha256_cache::Sha256Cache, validated_config::ValidatedConfig,
    wikilink::collect_file_wikilinks, wikilink_types::Wikilink, yaml_frontmatter::YamlFrontMatter,
};

use crate::markdown_file_info::MarkdownFileInfo;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use itertools::Itertools;
use rayon::prelude::*;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub hash: String,
    pub(crate) references: Vec<String>,
}

#[derive(Default)]
pub struct ObsidianRepositoryInfo {
    pub markdown_files: HashMap<PathBuf, MarkdownFileInfo>,
    pub image_map: HashMap<PathBuf, ImageInfo>,
    pub other_files: Vec<PathBuf>,
    pub wikilinks_ac: Option<AhoCorasick>,
    pub wikilinks_sorted: Vec<Wikilink>,
}

pub fn scan_obsidian_folder(
    config: &ValidatedConfig,
) -> Result<ObsidianRepositoryInfo, Box<dyn Error + Send + Sync>> {
    let start = Instant::now();

    let obsidian_repository_info = scan_folders(config)?;

    let duration = start.elapsed();
    let duration_string = &format!("{:.2}", duration.as_millis());
    println!("scan took: {}ms", duration_string);

    Ok(obsidian_repository_info)
}

fn get_image_info_map(
    config: &ValidatedConfig,
    markdown_files: &HashMap<PathBuf, MarkdownFileInfo>,
    image_files: &[PathBuf],
) -> Result<HashMap<PathBuf, ImageInfo>, Box<dyn Error + Send + Sync>> {
    let cache_file_path = config.obsidian_path().join(CACHE_FOLDER).join(CACHE_FILE);
    let (mut cache, _) = Sha256Cache::new(cache_file_path.clone())?;

    let mut image_info_map = HashMap::new();

    for image_path in image_files {
        let (hash, _) = cache.get_or_update(image_path)?;

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

    Ok(image_info_map)
}

fn is_ignored_folder(entry: &DirEntry, ignore_folders: &[PathBuf]) -> bool {
    let path = entry.path();
    ignore_folders
        .iter()
        .any(|ignored| path.starts_with(ignored))
}

pub fn scan_folders(
    config: &ValidatedConfig,
) -> Result<ObsidianRepositoryInfo, Box<dyn Error + Send + Sync>> {
    let ignore_folders = config.ignore_folders().unwrap_or(&[]);
    let mut obsidian_repository_info = ObsidianRepositoryInfo::default();
    let mut markdown_files = Vec::new();
    let mut image_files = Vec::new();

    // Create the list of files to operate on
    for entry in WalkDir::new(config.obsidian_path())
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| !is_ignored_folder(e, ignore_folders))
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

    // Convert HashSet to sorted Vec and store in repository info
    // Modified sorting logic to ensure total ordering
    obsidian_repository_info.wikilinks_sorted = all_wikilinks
        .into_iter()
        .sorted_by(|a, b| {
            // First compare by display text length (longer first)
            let length_cmp = b.display_text.len().cmp(&a.display_text.len());
            if length_cmp != std::cmp::Ordering::Equal {
                return length_cmp;
            }

            // Then compare display texts
            let display_cmp = a.display_text.cmp(&b.display_text);
            if display_cmp != std::cmp::Ordering::Equal {
                return display_cmp;
            }

            // For same display text, prefer aliases over non-aliases
            let alias_cmp = match (a.is_alias, b.is_alias) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            };
            if alias_cmp != std::cmp::Ordering::Equal {
                return alias_cmp;
            }

            // If everything else is equal, compare targets to ensure total ordering
            a.target.cmp(&b.target)
        })
        .collect();

    // Build the AC pattern from sorted wikilinks
    // exclude image links because when they have a size specified (like the number 300)
    // they'll match too many possible replacement targets
    let patterns: Vec<&str> = obsidian_repository_info
        .wikilinks_sorted
        .iter()
        .map(|w| w.display_text.as_str())
        .collect();

    obsidian_repository_info.wikilinks_ac = Some(
        AhoCorasickBuilder::new()
            .ascii_case_insensitive(true)
            .match_kind(MatchKind::LeftmostLongest)
            .build(&patterns)
            .expect("Failed to build Aho-Corasick automaton for wikilinks"),
    );

    // Process image info
    obsidian_repository_info.image_map = get_image_info_map(
        config,
        &obsidian_repository_info.markdown_files,
        &image_files,
    )?;

    // if !cfg!(test) {
    //     print_file_info(&obsidian_repository_info);
    // }

    Ok(obsidian_repository_info)
}

fn scan_markdown_files(
    markdown_files: &[PathBuf],
) -> Result<(HashMap<PathBuf, MarkdownFileInfo>, HashSet<Wikilink>), Box<dyn Error + Send + Sync>> {
    let start = Instant::now();

    let extensions_pattern = IMAGE_EXTENSIONS.join("|");
    let image_regex = Arc::new(Regex::new(&format!(
        r"(!\[(?:[^\]]*)\]\([^)]+\)|!\[\[([^\]]+\.(?:{}))(?:\|[^\]]+)?\]\])",
        extensions_pattern
    ))?);

    // Use Arc<Mutex<...>> for safe shared collection
    let markdown_info = Arc::new(Mutex::new(HashMap::new()));
    let all_wikilinks = Arc::new(Mutex::new(HashSet::new()));

    markdown_files.par_iter().try_for_each(|file_path| {
        match scan_markdown_file(file_path, &image_regex) {
            Ok((file_info, wikilinks)) => {
                markdown_info
                    .lock()
                    .unwrap()
                    .insert(file_path.clone(), file_info);
                all_wikilinks.lock().unwrap().extend(wikilinks);
                Ok(())
            }
            Err(e) => {
                eprintln!("Error processing file {:?}: {}", file_path, e);
                Err(e)
            }
        }
    })?;

    // Extract data from Arc<Mutex<...>>
    let markdown_info = Arc::try_unwrap(markdown_info)
        .unwrap()
        .into_inner()
        .unwrap();
    let all_wikilinks = Arc::try_unwrap(all_wikilinks)
        .unwrap()
        .into_inner()
        .unwrap();

    if !cfg!(test) {
        let duration = start.elapsed();
        let duration_string = &format!("{:.2}", duration.as_millis());
        println!("collect_file_wikilinks took: {}ms", duration_string);
    }

    Ok((markdown_info, all_wikilinks))
}

fn scan_markdown_file(
    file_path: &PathBuf,
    image_regex: &Arc<Regex>,
) -> Result<(MarkdownFileInfo, Vec<Wikilink>), Box<dyn Error + Send + Sync>> {
    let content = read_file_content(file_path)?;

    let mut markdown_file_info = initialize_markdown_file_info(&content);
    markdown_file_info.file_path = file_path.clone();

    extract_do_not_back_populate(&mut markdown_file_info);

    let aliases = markdown_file_info
        .frontmatter
        .as_ref()
        .and_then(|fm| fm.aliases().cloned());

    // collect_file_wikilinks constructs a set of wikilinks from the content (&content),
    // the aliases (&aliases) in the frontmatter and the name of the file itself (file_path)
    let extracted_wikilinks = collect_file_wikilinks(&content, &aliases, file_path)?;

    // Store invalid wikilinks in markdown_file_info
    markdown_file_info.add_invalid_wikilinks(extracted_wikilinks.invalid);

    collect_image_references(&content, image_regex, &mut markdown_file_info)?;

    Ok((markdown_file_info, extracted_wikilinks.valid))
}

fn read_file_content(file_path: &PathBuf) -> Result<String, Box<dyn Error + Send + Sync>> {
    let content = fs::read_to_string(file_path)?;
    Ok(content)
}

fn initialize_markdown_file_info(content: &str) -> MarkdownFileInfo {
    let mut file_info = MarkdownFileInfo::new();

    match FrontMatter::from_markdown_str(content) {
        Ok(frontmatter) => {
            file_info.frontmatter = Some(frontmatter);
        }
        Err(error) => {
            file_info.frontmatter_error = Some(error);
        }
    }

    file_info
}

fn extract_do_not_back_populate(markdown_file_info: &mut MarkdownFileInfo) {
    if let Some(fm) = &markdown_file_info.frontmatter {
        let mut do_not_populate = fm.do_not_back_populate.clone().unwrap_or_default();
        if let Some(aliases) = fm.aliases() {
            do_not_populate.extend(aliases.iter().cloned());
        }
        if !do_not_populate.is_empty() {
            markdown_file_info.do_not_back_populate = Some(do_not_populate.clone());
            markdown_file_info.do_not_back_populate_regexes =
                build_case_insensitive_word_finder(&Some(do_not_populate));
        }
    }
}

fn collect_image_references(
    content: &str,
    image_regex: &Arc<Regex>,
    file_info: &mut MarkdownFileInfo,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let reader = BufReader::new(content.as_bytes());

    for line_result in reader.lines() {
        let line = line_result?;
        collect_image_reference(image_regex, file_info, &line);
    }

    Ok(())
}

fn collect_image_reference(
    image_regex: &Arc<Regex>,
    file_info: &mut MarkdownFileInfo,
    line: &String,
) {
    for capture in image_regex.captures_iter(line) {
        if let Some(reference) = capture.get(0) {
            let reference_string = reference.as_str().to_string();
            file_info.image_links.push(reference_string);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_files;
    use crate::wikilink_types::InvalidWikilinkReason;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_scan_markdown_file_with_invalid_wikilinks() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create test content with both valid and invalid wikilinks
        let content = r#"# Test File
[[Valid Link]]
[[invalid|link|extra]]
[[unmatched
[[]]"#;

        fs::write(&file_path, content).unwrap();

        let image_regex = Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap());
        let (file_info, valid_wikilinks) = scan_markdown_file(&file_path, &image_regex).unwrap();

        // Check valid wikilinks
        assert_eq!(valid_wikilinks.len(), 2); // file name and "Valid Link"
        assert!(valid_wikilinks
            .iter()
            .any(|w| w.display_text == "Valid Link"));

        // Check invalid wikilinks
        assert_eq!(file_info.invalid_wikilinks.len(), 3);

        // Verify specific invalid wikilinks
        let double_alias = file_info
            .invalid_wikilinks
            .iter()
            .find(|w| w.reason == InvalidWikilinkReason::DoubleAlias)
            .expect("Should have a double alias invalid wikilink");
        assert_eq!(double_alias.content, "[[invalid|link|extra]]");

        let unmatched = file_info
            .invalid_wikilinks
            .iter()
            .find(|w| w.reason == InvalidWikilinkReason::UnmatchedOpening)
            .expect("Should have an unmatched opening invalid wikilink");
        assert_eq!(unmatched.content, "[[unmatched");

        let empty = file_info
            .invalid_wikilinks
            .iter()
            .find(|w| w.reason == InvalidWikilinkReason::EmptyWikilink)
            .expect("Should have an empty wikilink");
        assert_eq!(empty.content, "[[]]");
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
        let image_regex = Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap());

        // Scan the markdown file
        let (_file_info, wikilinks) = scan_markdown_file(&file_path, &image_regex).unwrap();

        // Collect unique target-display pairs
        let wikilink_pairs: HashSet<(String, String)> = wikilinks
            .iter()
            .map(|w| (w.target.clone(), w.display_text.clone()))
            .collect();

        // Updated assertions
        assert!(
            wikilink_pairs.contains(&("test_note".to_string(), "test_note".to_string())),
            "Should contain filename-based wikilink"
        );
        assert!(
            wikilink_pairs.contains(&("test_note".to_string(), "Alias One".to_string())),
            "Should contain first alias from frontmatter"
        );
        assert!(
            wikilink_pairs.contains(&("test_note".to_string(), "Second Alias".to_string())),
            "Should contain second alias from frontmatter"
        );
        assert!(
            wikilink_pairs.contains(&("Simple Link".to_string(), "Simple Link".to_string())),
            "Should contain simple wikilink"
        );
        assert!(
            wikilink_pairs.contains(&("Target Page".to_string(), "Display Text".to_string())),
            "Should contain aliased display text"
        );
        assert!(
            wikilink_pairs.contains(&("Alias One".to_string(), "Alias One".to_string())),
            "Should contain content wikilink to Alias One"
        );

        // note Alias One is technically a mistake on the user's part but let's deal with that
        // with a scan to find wikilinks that target nothing
        assert_eq!(
            wikilink_pairs.len(),
            6,
            "Should have collected all unique wikilinks including content reference to Alias One"
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

        // Create the files using our utility function
        create_test_files(temp_dir.path(), &files).unwrap();

        // Create minimal validated config
        let config = ValidatedConfig::new(
            false,
            None,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        // Scan the folders
        let repo_info = scan_folders(&config).unwrap();

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
                file_wikilinks.into_iter().map(|w| w.display_text)
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
        let parallel_results = process_parallel(&markdown_files);

        // Test sequential processing
        let sequential_results = process_sequential(&markdown_files);

        // Verify results
        assert_eq!(parallel_results.len(), sequential_results.len());
        assert_eq!(
            parallel_results.len(),
            100,
            "Should find common image in all files"
        );
    }

    #[test]
    fn test_scan_markdown_file_with_do_not_back_populate() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let content = r#"---
do_not_back_populate:
 - "test phrase"
 - "another phrase"
---
# Test Content"#;

        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        let image_regex = Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap());
        let (file_info, _) = scan_markdown_file(&file_path, &image_regex).unwrap();

        assert!(file_info.do_not_back_populate.is_some());
        let patterns = file_info.do_not_back_populate.unwrap();
        assert_eq!(patterns.len(), 2);
        assert!(patterns.contains(&"test phrase".to_string()));
        assert!(patterns.contains(&"another phrase".to_string()));
    }

    #[test]
    fn test_scan_markdown_file_combines_aliases_with_do_not_back_populate() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let content = r#"---
aliases:
 - "First Alias"
 - "Second Alias"
do_not_back_populate:
 - "exclude this"
---
# Test Content"#;

        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        let image_regex = Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap());
        let (file_info, _) = scan_markdown_file(&file_path, &image_regex).unwrap();

        assert!(file_info.do_not_back_populate.is_some());
        let patterns = file_info.do_not_back_populate.unwrap();
        assert_eq!(patterns.len(), 3);
        assert!(patterns.contains(&"First Alias".to_string()));
        assert!(patterns.contains(&"Second Alias".to_string()));
        assert!(patterns.contains(&"exclude this".to_string()));
    }

    #[test]
    fn test_scan_markdown_file_aliases_only() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let content = r#"---
aliases:
 - "Only Alias"
---
# Test Content"#;

        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        let image_regex = Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap());
        let (file_info, _) = scan_markdown_file(&file_path, &image_regex).unwrap();

        assert!(file_info.do_not_back_populate.is_some());
        let patterns = file_info.do_not_back_populate.unwrap();
        assert_eq!(patterns.len(), 1);
        assert!(patterns.contains(&"Only Alias".to_string()));
    }
    #[test]
    fn test_wikilink_sorting_with_aliases() {
        let temp_dir = TempDir::new().unwrap();

        // Create test files with different wikilinks
        let files = [
            (
                "tomato.md",
                r#"---
aliases:
  - "tomatoes"
---
# Tomato
Basic tomato info"#,
            ),
            (
                "recipe.md",
                r#"# Recipe
Using tomatoes in cooking"#,
            ),
            (
                "other.md",
                r#"# Other
[[tomatoes]] reference that might confuse things"#,
            ),
        ];

        // Create test environment and files
        let config = ValidatedConfig::new(
            false,
            None,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        // Create the files in the temp directory
        create_test_files(temp_dir.path(), &files).unwrap();

        // Scan folders and check results
        let repo_info = scan_folders(&config).unwrap();

        // Find the wikilinks for "tomatoes" in the sorted list
        let tomatoes_wikilinks: Vec<_> = repo_info
            .wikilinks_sorted
            .iter()
            .filter(|w| w.display_text.eq_ignore_ascii_case("tomatoes"))
            .collect();

        // Verify we found the wikilinks
        assert!(
            !tomatoes_wikilinks.is_empty(),
            "Should find wikilinks for 'tomatoes'"
        );

        // The first occurrence should be the alias version
        let first_tomatoes = &tomatoes_wikilinks[0];
        assert!(
            first_tomatoes.is_alias && first_tomatoes.target == "tomato",
            "First 'tomatoes' wikilink should be the alias version targeting 'tomato'"
        );

        // Add test for total ordering property
        let sorted = repo_info.wikilinks_sorted;
        for i in 1..sorted.len() {
            let comparison = sorted[i - 1]
                .display_text
                .len()
                .cmp(&sorted[i].display_text.len());
            assert_ne!(
                comparison,
                std::cmp::Ordering::Less,
                "Sorting violates length ordering at index {}",
                i
            );
        }
    }

    #[test]
    fn test_cache_file_cleanup() {
        // Create scope to ensure TempDir is dropped
        {
            let temp_dir = TempDir::new().unwrap();
            let cache_path = temp_dir.path().join(CACHE_FOLDER).join(CACHE_FILE);

            // Create test files and trigger cache creation
            let files = [("test.md", "# Test")];
            create_test_files(temp_dir.path(), &files).unwrap();

            // Create config that will create cache in temp dir
            let config = ValidatedConfig::new(
                false,
                None,
                None,
                None,
                None,
                temp_dir.path().to_path_buf(),
                temp_dir.path().join("output"),
            );

            // This will create the cache file
            let _ = scan_folders(&config).unwrap();

            // Verify cache was created
            assert!(cache_path.exists(), "Cache file should exist");

            // temp_dir will be dropped here
        }

        // Try to create a new temp dir with the same path (this would fail if the old one wasn't cleaned up)
        let new_temp = TempDir::new().unwrap();
        assert!(
            new_temp.path().exists(),
            "Should be able to create new temp dir"
        );
    }
}

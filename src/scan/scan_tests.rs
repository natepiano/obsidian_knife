use super::*;

use crate::config::ValidatedConfig;
use crate::scan::{scan_folders, scan_markdown_file};
use crate::test_utils::{get_test_markdown_file_info, TestFileBuilder};
use crate::wikilink_types::InvalidWikilinkReason;

use crate::utils::CachedImageInfo;
use regex::Regex;
use std::collections::HashSet;
use std::sync::Arc;
use tempfile::TempDir;

#[test]
fn test_scan_markdown_file_with_invalid_wikilinks() {
    let temp_dir = TempDir::new().unwrap();

    let file_path = TestFileBuilder::new()
        .with_content(
            r#"# Test File
[[Valid Link]]
[[invalid|link|extra]]
[[unmatched
[[]]"#
                .to_string(),
        )
        .create(&temp_dir, "test.md");

    let image_regex = Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap());
    let (file_info, valid_wikilinks) =
        scan_markdown_file(&file_path, &image_regex, DEFAULT_TIMEZONE).unwrap();

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

    let file_path = TestFileBuilder::new()
        .with_aliases(vec!["Alias One".to_string(), "Second Alias".to_string()])
        .with_content(
            r#"# Test Note

Here's a [[Simple Link]] and [[Target Page|Display Text]].
Also linking to [[Alias One]] which is defined in frontmatter."#
                .to_string(),
        )
        .create(&temp_dir, "test_note.md");

    // Test patterns
    let image_regex = Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap());

    // Scan the markdown file
    let (_file_info, wikilinks) =
        scan_markdown_file(&file_path, &image_regex, DEFAULT_TIMEZONE).unwrap();

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
fn test_parallel_image_reference_collection() {
    use rayon::prelude::*;

    let temp_dir = TempDir::new().unwrap();
    let mut markdown_files = HashMap::new();

    for i in 0..100 {
        let filename = format!("note{}.md", i);
        let content = format!("![image{}](test{}.jpg)\n![shared](common.jpg)", i, i);
        let file_path = TestFileBuilder::new()
            .with_content(content.clone())
            .create(&temp_dir, &filename);
        let mut info = get_test_markdown_file_info(file_path.clone());
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
    let file_path = TestFileBuilder::new()
        .with_content("# Test Content".to_string())
        .with_custom_frontmatter(
            r#"do_not_back_populate:
- "test phrase"
- "another phrase"
"#
            .to_string(),
        )
        .create(&temp_dir, "test.md");

    let image_regex = Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap());
    let (file_info, _) = scan_markdown_file(&file_path, &image_regex, DEFAULT_TIMEZONE).unwrap();
    // println!("fm: {:?}", file_info.content);

    assert!(file_info.do_not_back_populate_regexes.is_some());
    let regexes = file_info.do_not_back_populate_regexes.unwrap();
    assert_eq!(regexes.len(), 2);

    let test_line = "here is a test phrase and another phrase";
    assert!(regexes[0].is_match(test_line));
    assert!(regexes[1].is_match(test_line));
}

#[test]
fn test_scan_markdown_file_combines_aliases_with_do_not_back_populate() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_aliases(vec!["First Alias".to_string(), "Second Alias".to_string()])
        .with_custom_frontmatter(
            r#"do_not_back_populate:
- "exclude this"
"#
            .to_string(),
        )
        .with_content("# Test Content".to_string())
        .create(&temp_dir, "test.md");

    let image_regex = Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap());
    let (file_info, _) = scan_markdown_file(&file_path, &image_regex, DEFAULT_TIMEZONE).unwrap();

    assert!(file_info.do_not_back_populate_regexes.is_some());
    let regexes = file_info.do_not_back_populate_regexes.unwrap();
    assert_eq!(regexes.len(), 3);

    let test_line = "First Alias and Second Alias and exclude this";
    assert!(regexes[0].is_match(test_line));
    assert!(regexes[1].is_match(test_line));
    assert!(regexes[2].is_match(test_line));
}

#[test]
fn test_scan_markdown_file_aliases_only() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_aliases(vec!["Only Alias".to_string()])
        .with_content("# Test Content".to_string())
        .create(&temp_dir, "test.md");

    let image_regex = Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap());
    let (file_info, _) = scan_markdown_file(&file_path, &image_regex, DEFAULT_TIMEZONE).unwrap();

    assert!(file_info.do_not_back_populate_regexes.is_some());
    let regexes = file_info.do_not_back_populate_regexes.unwrap();
    assert_eq!(regexes.len(), 1);

    let test_line = "Only Alias appears here";
    assert!(regexes[0].is_match(test_line));
}

#[test]
fn test_scan_folders_wikilink_collection() {
    let temp_dir = TempDir::new().unwrap();

    // Create first note using TestFileBuilder
    TestFileBuilder::new()
        .with_aliases(vec!["Alias One".to_string()])
        .with_content("# Note 1\n[[Simple Link]]".to_string())
        .create(&temp_dir, "note1.md");

    // Create second note using TestFileBuilder
    TestFileBuilder::new()
        .with_aliases(vec!["Alias Two".to_string()])
        .with_content("# Note 2\n[[Target|Display Text]]\n[[Simple Link]]".to_string())
        .create(&temp_dir, "note2.md");

    // Create minimal validated config
    let config = ValidatedConfig::new(
        false,
        None,
        None,
        None,
        None,
        temp_dir.path().to_path_buf(),
        None,
        temp_dir.path().join("output"),
    );

    // Scan the folders
    let repo_info = scan_folders(&config).unwrap();

    // Filter for .md files only and exclude "obsidian knife output" explicitly
    let wikilinks: HashSet<String> = repo_info
        .markdown_files
        .iter()
        .filter(|file_info| file_info.path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .flat_map(|file_info| {
            let (_, file_wikilinks) = scan_markdown_file(
                &file_info.path,
                &Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap()),
                DEFAULT_TIMEZONE,
            )
            .unwrap();
            file_wikilinks.into_iter().map(|w| w.display_text)
        })
        .filter(|link| link != "obsidian knife output")
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
fn test_wikilink_sorting_with_aliases() {
    let temp_dir = TempDir::new().unwrap();

    // Create tomato file with alias
    TestFileBuilder::new()
        .with_aliases(vec!["tomatoes".to_string()])
        .with_content("# Tomato\nBasic tomato info".to_string())
        .create(&temp_dir, "tomato.md");

    // Create recipe file
    TestFileBuilder::new()
        .with_content("# Recipe\nUsing tomatoes in cooking".to_string())
        .create(&temp_dir, "recipe.md");

    // Create other file with wikilink
    TestFileBuilder::new()
        .with_content("# Other\n[[tomatoes]] reference that might confuse things".to_string())
        .create(&temp_dir, "other.md");

    let config = ValidatedConfig::new(
        false,
        None,
        None,
        None,
        None,
        temp_dir.path().to_path_buf(),
        None,
        temp_dir.path().join("output"),
    );

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

// #[test]
// fn test_cache_file_cleanup() {
//     // Create scope to ensure TempDir is dropped
//     {
//         let temp_dir = TempDir::new().unwrap();
//         let cache_path = temp_dir.path().join(CACHE_FOLDER).join(CACHE_FILE);
//
//         // Create a test file using TestFileBuilder
//         TestFileBuilder::new()
//             .with_content("# Test".to_string())
//             .create(&temp_dir, "test.md");
//
//         // Create config that will create cache in temp dir
//         let config = ValidatedConfig::new(
//             false,
//             None,
//             None,
//             None,
//             None,
//             temp_dir.path().to_path_buf(),
//             temp_dir.path().join("output"),
//         );
//
//         // This will create the cache file
//         let _ = scan_folders(&config).unwrap();
//
//         // Verify cache was created
//         assert!(cache_path.exists(), "Cache file should exist");
//
//         // temp_dir will be dropped here
//     }
//
//     // Try to create a new temp dir with the same path (this would fail if the old one wasn't cleaned up)
//     let new_temp = TempDir::new().unwrap();
//     assert!(
//         new_temp.path().exists(),
//         "Should be able to create new temp dir"
//     );
// }
#[test]
fn test_cache_file_cleanup() {
    // Create scope to ensure TempDir is dropped
    {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join(CACHE_FOLDER).join(CACHE_FILE);

        // Create a test file and image using TestFileBuilder
        TestFileBuilder::new()
            .with_content("# Test\n![test](test.png)".to_string())
            .create(&temp_dir, "test.md");

        TestFileBuilder::new()
            .with_content(vec![0xFF, 0xD8, 0xFF, 0xE0]) // Simple PNG header
            .create(&temp_dir, "test.png");

        // Create config that will create cache in temp dir
        let config = ValidatedConfig::new(
            false,
            None,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            None,
            temp_dir.path().join("output"),
        );

        // First scan - creates cache with the image
        let _ = scan_folders(&config).unwrap();

        // Delete the image file
        std::fs::remove_file(temp_dir.path().join("test.png")).unwrap();

        // Second scan - should detect the deleted image
        let _ = scan_folders(&config).unwrap();

        // Verify cache was cleaned up
        let cache_content = std::fs::read_to_string(&cache_path).unwrap();
        let cache: HashMap<PathBuf, CachedImageInfo> =
            serde_json::from_str(&cache_content).unwrap();
        assert!(cache.is_empty(), "Cache should be empty after cleanup");

        // temp_dir will be dropped here
    }

    // Try to create a new temp dir with the same path
    let new_temp = TempDir::new().unwrap();
    assert!(
        new_temp.path().exists(),
        "Should be able to create new temp dir"
    );
}

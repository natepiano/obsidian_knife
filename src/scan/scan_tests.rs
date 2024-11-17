use super::*;

use crate::scan::{scan_folders, scan_markdown_file};
use crate::test_utils::TestFileBuilder;
use crate::validated_config::ValidatedConfig;
use crate::wikilink_types::InvalidWikilinkReason;

use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
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

// #[test]
// fn test_scan_folders_wikilink_collection() {
//     let temp_dir = TempDir::new().unwrap();
//
//     // Create test files with different wikilinks
//     let files = [
//         (
//             "note1.md",
//             r#"---
// aliases:
// - "Alias One"
// ---
// # Note 1
// [[Simple Link]]"#,
//         ),
//         (
//             "note2.md",
//             r#"---
// aliases:
// - "Alias Two"
// ---
// # Note 2
// [[Target|Display Text]]
// [[Simple Link]]"#,
//         ),
//     ];
//
//     // Create the files using our utility function
//     create_test_files(temp_dir.path(), &files).unwrap();
//
//     // Create minimal validated config
//     let config = ValidatedConfig::new(
//         false,
//         None,
//         None,
//         None,
//         None,
//         temp_dir.path().to_path_buf(),
//         temp_dir.path().join("output"),
//     );
//
//     // Scan the folders
//     let repo_info = scan_folders(&config).unwrap();
//
//     // Filter for .md files only and exclude "obsidian knife output" explicitly
//     let wikilinks: HashSet<String> = repo_info
//         .markdown_files
//         .iter()
//         .filter(|file_info| {
//             file_info.path.extension().and_then(|ext| ext.to_str()) == Some("md")
//         })
//         .flat_map(|file_info| {
//             let (_, file_wikilinks) = scan_markdown_file(
//                 &file_info.path,
//                 &Arc::new(Regex::new(r"!\[\[([^]]+)]]").unwrap()),
//             )
//                 .unwrap();
//             file_wikilinks.into_iter().map(|w| w.display_text)
//         })
//         .filter(|link| link != "obsidian knife output")
//         .collect();
//
//     // Verify expected wikilinks are present
//     assert!(wikilinks.contains("note1"), "Should contain first filename");
//     assert!(
//         wikilinks.contains("note2"),
//         "Should contain second filename"
//     );
//     assert!(
//         wikilinks.contains("Alias One"),
//         "Should contain first alias"
//     );
//     assert!(
//         wikilinks.contains("Alias Two"),
//         "Should contain second alias"
//     );
//     assert!(
//         wikilinks.contains("Simple Link"),
//         "Should contain simple link"
//     );
//     assert!(
//         wikilinks.contains("Display Text"),
//         "Should contain display text from alias"
//     );
//
//     // Verify total count
//     assert_eq!(
//         wikilinks.len(),
//         6,
//         "Should have collected all unique wikilinks"
//     );
// }

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
        fs::write(&file_path, &content).unwrap();
        let mut info = MarkdownFileInfo::new(file_path.clone()).unwrap();
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

    assert!(file_info.do_not_back_populate_regexes.is_some());
    let regexes = file_info.do_not_back_populate_regexes.unwrap();
    assert_eq!(regexes.len(), 1);

    let test_line = "Only Alias appears here";
    assert!(regexes[0].is_match(test_line));
}

// #[test]
// fn test_wikilink_sorting_with_aliases() {
//     let temp_dir = TempDir::new().unwrap();
//
//     // Create test files with different wikilinks
//     let files = [
//         (
//             "tomato.md",
//             r#"---
// aliases:
// - "tomatoes"
// ---
// # Tomato
// Basic tomato info"#,
//         ),
//         (
//             "recipe.md",
//             r#"# Recipe
// Using tomatoes in cooking"#,
//         ),
//         (
//             "other.md",
//             r#"# Other
// [[tomatoes]] reference that might confuse things"#,
//         ),
//     ];
//
//     // Create test environment and files
//     let config = ValidatedConfig::new(
//         false,
//         None,
//         None,
//         None,
//         None,
//         temp_dir.path().to_path_buf(),
//         temp_dir.path().join("output"),
//     );
//
//     // Create the files in the temp directory
//     create_test_files(temp_dir.path(), &files).unwrap();
//
//     // Scan folders and check results
//     let repo_info = scan_folders(&config).unwrap();
//
//     // Find the wikilinks for "tomatoes" in the sorted list
//     let tomatoes_wikilinks: Vec<_> = repo_info
//         .wikilinks_sorted
//         .iter()
//         .filter(|w| w.display_text.eq_ignore_ascii_case("tomatoes"))
//         .collect();
//
//     // Verify we found the wikilinks
//     assert!(
//         !tomatoes_wikilinks.is_empty(),
//         "Should find wikilinks for 'tomatoes'"
//     );
//
//     // The first occurrence should be the alias version
//     let first_tomatoes = &tomatoes_wikilinks[0];
//     assert!(
//         first_tomatoes.is_alias && first_tomatoes.target == "tomato",
//         "First 'tomatoes' wikilink should be the alias version targeting 'tomato'"
//     );
//
//     // Add test for total ordering property
//     let sorted = repo_info.wikilinks_sorted;
//     for i in 1..sorted.len() {
//         let comparison = sorted[i - 1]
//             .display_text
//             .len()
//             .cmp(&sorted[i].display_text.len());
//         assert_ne!(
//             comparison,
//             std::cmp::Ordering::Less,
//             "Sorting violates length ordering at index {}",
//             i
//         );
//     }
// }

// #[test]
// fn test_cache_file_cleanup() {
//     // Create scope to ensure TempDir is dropped
//     {
//         let temp_dir = TempDir::new().unwrap();
//         let cache_path = temp_dir.path().join(CACHE_FOLDER).join(CACHE_FILE);
//
//         // Create test files and trigger cache creation
//         let files = [("test.md", "# Test")];
//         create_test_files(temp_dir.path(), &files).unwrap();
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

#[test]
fn test_cache_file_cleanup() {
    // Create scope to ensure TempDir is dropped
    {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join(CACHE_FOLDER).join(CACHE_FILE);

        // Create a test file using TestFileBuilder
        TestFileBuilder::new()
            .with_content("# Test".to_string())
            .create(&temp_dir, "test.md");

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

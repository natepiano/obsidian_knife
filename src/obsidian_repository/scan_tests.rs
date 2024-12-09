use crate::constants::*;
use crate::test_utils::{get_test_markdown_file, TestFileBuilder};
use crate::utils::CachedImageInfo;
use crate::validated_config::get_test_validated_config;

use crate::markdown_file::{ImageLink, MarkdownFile};
use crate::obsidian_repository::ObsidianRepository;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_parallel_image_reference_collection() {
    use rayon::prelude::*;

    let temp_dir = TempDir::new().unwrap();
    let mut markdown_files = HashMap::new();

    for i in 0..100 {
        let filename = format!("note{}.md", i);
        let content = format!("![[test{}.jpg]]\n![[common.jpg]]", i);
        let file_path = TestFileBuilder::new()
            .with_content(content.clone())
            .create(&temp_dir, &filename);
        let mut info = get_test_markdown_file(file_path.clone());

        info.image_links.links = content
            .split('\n')
            .map(|s| ImageLink::new(s.to_string(), 1, 0))
            .collect();

        markdown_files.insert(file_path, info);
    }

    // Common filter logic
    fn has_common_image(info: &MarkdownFile) -> bool {
        info.image_links
            .links
            .iter()
            .any(|link| link.filename == "common.jpg")
    }

    // Helper functions using shared filter
    fn process_parallel(files: &HashMap<PathBuf, MarkdownFile>) -> Vec<PathBuf> {
        files
            .par_iter()
            .filter_map(|(path, info)| has_common_image(info).then(|| path.clone()))
            .collect()
    }

    fn process_sequential(files: &HashMap<PathBuf, MarkdownFile>) -> Vec<PathBuf> {
        files
            .iter()
            .filter_map(|(path, info)| {
                if has_common_image(info) {
                    Some(path.clone())
                } else {
                    None
                }
            })
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

    let config = get_test_validated_config(&temp_dir, None);

    // Scan folders and check results
    let repository = ObsidianRepository::new(&config).unwrap();

    // Find the wikilinks for "tomatoes" in the sorted list
    let tomatoes_wikilinks: Vec<_> = repository
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
    let sorted = repository.wikilinks_sorted;
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

        // Create a test file and image using TestFileBuilder
        TestFileBuilder::new()
            .with_content("# Test\n![test](test.png)".to_string())
            .create(&temp_dir, "test.md");

        TestFileBuilder::new()
            .with_content(vec![0xFF, 0xD8, 0xFF, 0xE0]) // Simple PNG header
            .create(&temp_dir, "test.png");

        // Create config that will create cache in temp dir
        let config = get_test_validated_config(&temp_dir, None);

        // First scan - creates cache with the image
        let _ = ObsidianRepository::new(&config).unwrap();

        // Delete the image file
        std::fs::remove_file(temp_dir.path().join("test.png")).unwrap();

        // Second scan - should detect the deleted image
        let _ = ObsidianRepository::new(&config).unwrap();

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

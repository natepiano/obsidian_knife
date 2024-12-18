use crate::markdown_file::back_populate_tests;
use crate::markdown_file::MarkdownFile;
use crate::obsidian_repository::ObsidianRepository;
use crate::test_utils::TestFileBuilder;
use crate::validated_config::validated_config_tests;
use crate::wikilink::{InvalidWikilinkReason, Wikilink};
use crate::DEFAULT_TIMEZONE;
use crate::{wikilink, MARKDOWN_EXTENSION};
use std::collections::HashSet;
use tempfile::TempDir;

#[test]
fn test_find_matches_with_existing_wikilinks() {
    let content = "[[Some Link]] and Test Link in same line\n\
           Test Link [[Other Link]] Test Link mixed\n\
           This don't match\n\
           This don't match either\n\
           But this Test Link should match";

    let (_temp_dir, config, mut repository) =
        back_populate_tests::create_test_environment(false, None, None, Some(content));

    // Find matches - this now stores them in repository.markdown_files
    repository.find_all_back_populate_matches(&config);

    // Get all matches from the first (and only) file
    let matches = &repository.markdown_files[0].matches;

    // We expect 4 matches for "Test Link" outside existing wikilinks and contractions
    assert_eq!(
        matches.unambiguous.len(),
        4,
        "Mismatch in number of matches"
    );

    // Verify that the matches are at the expected positions
    let expected_lines = vec![5, 6, 6, 9];
    let actual_lines: Vec<usize> = matches.unambiguous.iter().map(|m| m.line_number).collect();
    assert_eq!(
        actual_lines, expected_lines,
        "Mismatch in line numbers of matches"
    );
}

#[test]
fn test_overlapping_wikilink_matches() {
    let content = "[[Kyriana McCoy|Kyriana]] - Kyri and [[Kalina McCoy|Kali]]";
    let wikilinks = vec![
        Wikilink {
            display_text: "Kyri".to_string(),
            target: "Kyri".to_string(),
        },
        Wikilink {
            display_text: "Kyri".to_string(),
            target: "Kyriana McCoy".to_string(),
        },
    ];

    let (_temp_dir, config, mut repository) =
        back_populate_tests::create_test_environment(false, None, Some(wikilinks), Some(content));

    // Find matches - this now stores them in repository.markdown_files
    repository.find_all_back_populate_matches(&config);

    // Get matches from the first (and only) file
    let matches = &repository.markdown_files[0].matches;

    assert_eq!(matches.unambiguous.len(), 1, "Expected exactly one match");
    assert_eq!(
        matches.unambiguous[0].position, 28,
        "Expected match at position 28"
    );
}

#[test]
fn test_is_within_wikilink() {
    let test_cases = vec![
        // ASCII cases
        ("before [[link]] after", 7, false),
        ("before [[link]] after", 8, false),
        ("before [[link]] after", 9, true),
        ("before [[link]] after", 10, true),
        ("before [[link]] after", 11, true),
        ("before [[link]] after", 12, true),
        ("before [[link]] after", 13, false),
        ("before [[link]] after", 14, false),
        // Unicode cases
        ("привет [[ссылка]] текст", 13, false),
        ("привет [[ссылка]] текст", 14, false),
        ("привет [[ссылка]] текст", 15, true),
        ("привет [[ссылка]] текст", 25, true),
        ("привет [[ссылка]] текст", 27, false),
        ("привет [[ссылка]] текст", 28, false),
        ("привет [[ссылка]] текст", 12, false),
        ("привет [[ссылка]] текст", 29, false),
    ];

    for (text, pos, expected) in test_cases {
        assert_eq!(
            wikilink::is_within_wikilink(text, pos),
            expected,
            "Failed for text '{}' at position {}",
            text,
            pos
        );
    }
}

#[test]
fn test_markdown_file_with_invalid_wikilinks() {
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

    let file_info = MarkdownFile::new(file_path, DEFAULT_TIMEZONE).unwrap();
    let valid_wikilinks = file_info.wikilinks.valid;

    // Check valid wikilinks
    assert_eq!(valid_wikilinks.len(), 2); // file name and "Valid Link"
    assert!(valid_wikilinks
        .iter()
        .any(|w| w.display_text == "Valid Link"));

    // Check invalid wikilinks
    assert_eq!(file_info.wikilinks.invalid.len(), 3);

    // Verify specific invalid wikilinks
    let double_alias = file_info
        .wikilinks
        .invalid
        .iter()
        .find(|w| w.reason == InvalidWikilinkReason::DoubleAlias)
        .expect("Should have a double alias invalid wikilink");
    assert_eq!(double_alias.content, "[[invalid|link|extra]]");

    let unmatched = file_info
        .wikilinks
        .invalid
        .iter()
        .find(|w| w.reason == InvalidWikilinkReason::UnmatchedOpening)
        .expect("Should have an unmatched opening invalid wikilink");
    assert_eq!(unmatched.content, "[[unmatched");

    let empty = file_info
        .wikilinks
        .invalid
        .iter()
        .find(|w| w.reason == InvalidWikilinkReason::EmptyWikilink)
        .expect("Should have an empty wikilink");
    assert_eq!(empty.content, "[[]]");
}

#[test]
fn test_markdown_file_wikilink_collection() {
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

    let file_info = MarkdownFile::new(file_path, DEFAULT_TIMEZONE).unwrap();
    let wikilinks = file_info.wikilinks.valid;

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
    let config = validated_config_tests::get_test_validated_config(&temp_dir, None);

    // Scan the folders
    let repository = ObsidianRepository::new(&config).unwrap();

    // Filter for .md files only and exclude "obsidian knife output" explicitly
    let wikilinks: HashSet<String> = repository
        .markdown_files
        .iter()
        .filter(|file_info| {
            file_info.path.extension().and_then(|ext| ext.to_str()) == Some(MARKDOWN_EXTENSION)
        })
        .flat_map(|file_info| {
            let file_info = MarkdownFile::new(file_info.path.clone(), DEFAULT_TIMEZONE).unwrap();
            let file_wikilinks = file_info.wikilinks.valid;
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

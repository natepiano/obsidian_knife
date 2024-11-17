use crate::back_populate::back_populate_tests::{
    build_aho_corasick, create_markdown_test_file, create_test_environment,
};
use crate::back_populate::{
    apply_back_populate_changes, process_line, should_create_match, BackPopulateMatch,
};
use crate::markdown_file_info::MarkdownFileInfo;
use crate::test_utils::TestFileBuilder;
use crate::wikilink_types::Wikilink;
use std::fs;

#[test]
fn test_should_create_match_in_table() {
    // Set up the test environment
    let (temp_dir, _, _) = create_test_environment(false, None, None, None);
    let file_path = temp_dir.path().join("test.md");

    let markdown_info = MarkdownFileInfo::new(file_path.clone()).unwrap();

    // Test simple table cell match
    assert!(should_create_match(
        "| Test Link | description |",
        2,
        "Test Link",
        &file_path,
        &markdown_info,
    ));

    // Test match in table with existing wikilinks
    assert!(should_create_match(
        "| Test Link | [[Other]] |",
        2,
        "Test Link",
        &file_path,
        &markdown_info,
    ));
}

#[test]
fn test_back_populate_content() {
    // Initialize environment with `apply_changes` set to true
    let (temp_dir, config, mut repo_info) = create_test_environment(true, None, None, None);

    // Define test cases with various content structures
    let test_cases = vec![
        (
            "# Test Table\n|Name|Description|\n|---|---|\n|Test Link|Sample text|\n",
            vec![BackPopulateMatch {
                full_path: temp_dir.path().join("test.md"),
                relative_path: "test.md".into(),
                line_number: 4,
                line_text: "|Test Link|Sample text|".into(),
                found_text: "Test Link".into(),
                replacement: "[[Test Link\\|Another Name]]".into(),
                position: 1,
                in_markdown_table: true,
            }],
            "Table content replacement",
        ),
        (
            "# Mixed Content\n\
        Regular Test Link here\n\
        |Name|Description|\n\
        |---|---|\n\
        |Test Link|Sample|\n\
        More Test Link text",
            vec![
                BackPopulateMatch {
                    full_path: temp_dir.path().join("test.md"),
                    relative_path: "test.md".into(),
                    line_number: 2,
                    line_text: "Regular Test Link here".into(),
                    found_text: "Test Link".into(),
                    replacement: "[[Test Link]]".into(),
                    position: 8,
                    in_markdown_table: false,
                },
                BackPopulateMatch {
                    full_path: temp_dir.path().join("test.md"),
                    relative_path: "test.md".into(),
                    line_number: 5,
                    line_text: "|Test Link|Sample|".into(),
                    found_text: "Test Link".into(),
                    replacement: "[[Test Link\\|Display]]".into(),
                    position: 1,
                    in_markdown_table: true,
                },
            ],
            "Mixed table and regular content replacement",
        ),
    ];

    for (content, matches, description) in test_cases {
        let file_path = create_markdown_test_file(&temp_dir, "test.md", content, &mut repo_info);

        // Apply back-populate changes
        apply_back_populate_changes(&config, &matches).unwrap();

        // Verify changes
        let updated_content = fs::read_to_string(&file_path).unwrap();
        for match_info in matches {
            assert!(
                updated_content.contains(&match_info.replacement),
                "Failed for: {}",
                description
            );
        }
    }
}

#[test]
fn test_process_line_table_escaping_combined() {
    // Define multiple wikilinks
    let wikilinks = vec![
        Wikilink {
            display_text: "Test Link".to_string(),
            target: "Target Page".to_string(),
            is_alias: true,
        },
        Wikilink {
            display_text: "Another Link".to_string(),
            target: "Other Page".to_string(),
            is_alias: false,
        },
    ];

    // Initialize environment with custom wikilinks
    let (temp_dir, config, repo_info) =
        create_test_environment(false, None, Some(wikilinks.clone()), None);

    // Compile the wikilinks
    let sorted_wikilinks = &repo_info.wikilinks_sorted;

    let ac = build_aho_corasick(sorted_wikilinks);

    let markdown_info = repo_info.markdown_files.first().unwrap();

    // Define test cases with different table formats and expected replacements
    let test_cases = vec![
        (
            "| Test Link | Another Link | description |",
            vec![
                "[[Target Page\\|Test Link]]",
                "[[Other Page\\|Another Link]]",
            ],
            "Multiple matches in one row",
        ),
        (
            "| prefix Test Link suffix | Another Link |",
            vec![
                "[[Target Page\\|Test Link]]",
                "[[Other Page\\|Another Link]]",
            ],
            "Table cells with surrounding text",
        ),
        (
            "| column1 | Test Link | Another Link |",
            vec![
                "[[Target Page\\|Test Link]]",
                "[[Other Page\\|Another Link]]",
            ],
            "Different column positions",
        ),
        (
            "| Test Link | description | Another Link |",
            vec![
                "[[Target Page\\|Test Link]]",
                "[[Other Page\\|Another Link]]",
            ],
            "Multiple replacements in different columns",
        ),
    ];

    // Create references to the compiled wikilinks
    let wikilink_refs: Vec<&Wikilink> = sorted_wikilinks.iter().collect();
    for (line, expected_replacements, description) in test_cases {
        // Create test file using TestFileBuilder
        let _ = TestFileBuilder::new()
            .with_title("test".to_string())
            .with_content(line.to_string())
            .create(&temp_dir, "test.md");

        let matches = process_line(0, line, &ac, &wikilink_refs, &config, &markdown_info).unwrap();

        assert_eq!(
            matches.len(),
            expected_replacements.len(),
            "Incorrect number of replacements for: {}",
            description
        );

        for (match_info, expected) in matches.iter().zip(expected_replacements.iter()) {
            assert_eq!(
                match_info.replacement, *expected,
                "Incorrect replacement for: {}",
                description
            );
            assert!(
                match_info.in_markdown_table,
                "Should be marked as in table for: {}",
                description
            );
        }
    }
}

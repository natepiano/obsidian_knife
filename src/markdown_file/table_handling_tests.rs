use crate::markdown_file::back_populate_tests;
use crate::markdown_file::{BackPopulateMatch, MarkdownFile};
use crate::markdown_files::MarkdownFiles;
use crate::test_utils::TestFileBuilder;
use crate::wikilink::Wikilink;

#[test]
fn test_should_create_match_in_table() {
    // Set up the test environment
    let (temp_dir, config, _) =
        back_populate_tests::create_test_environment(false, None, None, None);
    let file_path = temp_dir.path().join("test.md");

    let markdown_file =
        MarkdownFile::new(file_path.clone(), config.operational_timezone()).unwrap();

    // Test simple table cell match
    assert!(markdown_file.should_create_match("| Test Link | description |", 2, "Test Link",));

    // Test match in table with existing wikilinks
    assert!(markdown_file.should_create_match("| Test Link | [[Other]] |", 2, "Test Link",));
}

#[test]
fn test_back_populate_content() {
    // Initialize environment with `apply_changes` set to true
    let (temp_dir, config, mut repository) =
        back_populate_tests::create_test_environment(true, None, None, None);

    let test_cases = vec![(
        "# Test Table\n|Name|Description|\n|---|---|\n|Test Link|Sample text|\n",
        vec![BackPopulateMatch {
            relative_path: "test.md".into(),
            line_number: 4,
            line_text: "|Test Link|Sample text|".into(),
            found_text: "Test Link".into(),
            replacement: "[[Test Link\\|Another Name]]".into(),
            position: 1,
            in_markdown_table: true,
        }],
        "Table content replacement",
    )];

    test_cases
        .into_iter()
        .for_each(|(content, matches, description)| {
            // Create and populate the test file
            let file = TestFileBuilder::new()
                .with_content(content.to_string())
                .with_title("test".to_string())
                .create(&temp_dir, "test.md");

            // Prepare markdown info and repository state
            let markdown_info = {
                let mut markdown_info =
                    MarkdownFile::new(file.clone(), config.operational_timezone()).unwrap();
                markdown_info.content = content.to_string();
                markdown_info.matches.unambiguous = matches.clone();
                markdown_info
            };

            repository.markdown_files = MarkdownFiles::new(vec![markdown_info], None);

            // Apply back-populate changes
            repository.apply_replaceable_matches(config.operational_timezone());

            // Validate replacements
            if let Some(file) = repository.markdown_files.iter().find(|f| f.path == file) {
                matches.iter().for_each(|match_info| {
                    assert!(
                        file.content.contains(&match_info.replacement),
                        "Failed for: {}\nReplacement '{}' not found in content:\n{}",
                        description,
                        match_info.replacement,
                        file.content
                    );
                });
            }
        });
}

#[test]
fn test_process_line_table_escaping_combined() {
    // Define multiple wikilinks
    let wikilinks = vec![
        Wikilink {
            display_text: "Another Link".to_string(),
            target: "Other Page".to_string(),
        },
        Wikilink {
            display_text: "Test Link".to_string(),
            target: "Target Page".to_string(),
        },
    ];

    // Initialize environment with custom wikilinks
    let (temp_dir, config, repository) =
        back_populate_tests::create_test_environment(false, None, Some(wikilinks.clone()), None);

    // Compile the wikilinks
    let sorted_wikilinks = &repository.wikilinks_sorted;

    let ac = back_populate_tests::build_aho_corasick(sorted_wikilinks);

    let markdown_info = repository.markdown_files.first().unwrap();

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

        let matches = markdown_info.process_line_for_back_populate_replacements(
            line,
            0,
            &ac,
            &wikilink_refs,
            &config,
        );

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

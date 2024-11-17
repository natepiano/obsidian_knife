use crate::back_populate::back_populate_tests::create_test_environment;
use crate::back_populate::{
    find_all_back_populate_matches, identify_ambiguous_matches, BackPopulateMatch,
};
use crate::scan::scan_folders;
use crate::test_utils::TestFileBuilder;
use crate::wikilink_types::Wikilink;
use std::path::PathBuf;

#[test]
fn test_identify_ambiguous_matches() {
    // Create test wikilinks
    let wikilinks = vec![
        Wikilink {
            display_text: "Ed".to_string(),
            target: "Ed Barnes".to_string(),
            is_alias: true,
        },
        Wikilink {
            display_text: "Ed".to_string(),
            target: "Ed Stanfield".to_string(),
            is_alias: true,
        },
        Wikilink {
            display_text: "Unique".to_string(),
            target: "Unique Target".to_string(),
            is_alias: false,
        },
    ];

    let matches = vec![
        BackPopulateMatch {
            full_path: PathBuf::from("test1.md"),
            relative_path: "test1.md".to_string(),
            line_number: 1,
            line_text: "Ed wrote this".to_string(),
            found_text: "Ed".to_string(),
            replacement: "[[Ed Barnes|Ed]]".to_string(),
            position: 0,
            in_markdown_table: false,
        },
        BackPopulateMatch {
            full_path: PathBuf::from("test2.md"),
            relative_path: "test2.md".to_string(),
            line_number: 1,
            line_text: "Unique wrote this".to_string(),
            found_text: "Unique".to_string(),
            replacement: "[[Unique Target]]".to_string(),
            position: 0,
            in_markdown_table: false,
        },
    ];

    let (ambiguous, unambiguous) = identify_ambiguous_matches(&matches, &wikilinks);

    // Check ambiguous matches
    assert_eq!(ambiguous.len(), 1, "Should have one ambiguous match group");
    assert_eq!(ambiguous[0].display_text, "ed");
    assert_eq!(ambiguous[0].targets.len(), 2);
    assert!(ambiguous[0].targets.contains(&"Ed Barnes".to_string()));
    assert!(ambiguous[0].targets.contains(&"Ed Stanfield".to_string()));

    // Check unambiguous matches
    assert_eq!(unambiguous.len(), 1, "Should have one unambiguous match");
    assert_eq!(unambiguous[0].found_text, "Unique");
}

#[test]
fn test_truly_ambiguous_targets() {
    // Create test wikilinks with actually different targets
    let wikilinks = vec![
        Wikilink {
            display_text: "Amazon".to_string(),
            target: "Amazon (company)".to_string(),
            is_alias: true,
        },
        Wikilink {
            display_text: "Amazon".to_string(),
            target: "Amazon (river)".to_string(),
            is_alias: true,
        },
    ];

    let matches = vec![BackPopulateMatch {
        full_path: PathBuf::from("test1.md"),
        relative_path: "test1.md".to_string(),
        line_number: 1,
        line_text: "Amazon is huge".to_string(),
        found_text: "Amazon".to_string(),
        replacement: "[[Amazon (company)|Amazon]]".to_string(),
        position: 0,
        in_markdown_table: false,
    }];

    let (ambiguous, unambiguous) = identify_ambiguous_matches(&matches, &wikilinks);

    assert_eq!(
        ambiguous.len(),
        1,
        "Different targets should be identified as ambiguous"
    );
    assert_eq!(
        unambiguous.len(),
        0,
        "No matches should be considered unambiguous"
    );
    assert_eq!(ambiguous[0].targets.len(), 2);
}

#[test]
fn test_mixed_case_and_truly_ambiguous() {
    let wikilinks = vec![
        // Case variations of one target
        Wikilink {
            display_text: "AWS".to_string(),
            target: "AWS".to_string(),
            is_alias: false,
        },
        Wikilink {
            display_text: "aws".to_string(),
            target: "aws".to_string(),
            is_alias: false,
        },
        // Truly different targets
        Wikilink {
            display_text: "Amazon".to_string(),
            target: "Amazon (company)".to_string(),
            is_alias: true,
        },
        Wikilink {
            display_text: "Amazon".to_string(),
            target: "Amazon (river)".to_string(),
            is_alias: true,
        },
    ];

    let matches = vec![
        BackPopulateMatch {
            full_path: PathBuf::from("test1.md"),
            relative_path: "test1.md".to_string(),
            line_number: 1,
            line_text: "AWS and aws are the same".to_string(),
            found_text: "AWS".to_string(),
            replacement: "[[AWS]]".to_string(),
            position: 0,
            in_markdown_table: false,
        },
        BackPopulateMatch {
            full_path: PathBuf::from("test1.md"),
            relative_path: "test1.md".to_string(),
            line_number: 2,
            line_text: "Amazon is ambiguous".to_string(),
            found_text: "Amazon".to_string(),
            replacement: "[[Amazon (company)|Amazon]]".to_string(),
            position: 0,
            in_markdown_table: false,
        },
    ];

    let (ambiguous, unambiguous) = identify_ambiguous_matches(&matches, &wikilinks);

    assert_eq!(
        ambiguous.len(),
        1,
        "Should only identify truly different targets as ambiguous"
    );
    assert_eq!(
        unambiguous.len(),
        1,
        "Case variations should be identified as unambiguous"
    );
}

// This test sets up an **ambiguous alias** (`"Nate"`) mapping to two different targets.
// It ensures that the `identify_ambiguous_matches` function correctly **classifies** both instances of `"Nate"` as **ambiguous**.
//
// Validate that the function can handle **both unambiguous and ambiguous matches simultaneously** without interference.
// prior to this the real world failure was that it would find Karen as an alias but not karen
// even though we have a case-insensitive search
// the problem with the old test is that when there wa sno ambiguous matches - then
// the lower case karen wasn't getting stripped out and the test would pass even though the real world failed
// so in this case we are creating a more realistic test that has a mix of ambiguous and unambiguous
#[test]
fn test_combined_ambiguous_and_unambiguous_matches() {
    // Create initial environment with empty wikilinks list
    let (temp_dir, config, _) = create_test_environment(
        false,
        None,
        Some(vec![]), // Empty initial wikilinks
        None,
    );

    // Create the files using TestFileBuilder
    TestFileBuilder::new()
        .with_content(
            r#"# Reference Page
Karen is here
karen is here too
Nate was here and so was Nate"#
                .to_string(),
        ) // Changed from "Test Page" to "Reference Page"
        .with_title("reference page".to_string())
        .create(&temp_dir, "other.md");

    TestFileBuilder::new()
        .with_content("# Karen McCoy's Page".to_string())
        .with_title("karen mccoy".to_string())
        .with_aliases(vec!["Karen".to_string()])
        .create(&temp_dir, "Karen McCoy.md");

    TestFileBuilder::new()
        .with_content("# Nate McCoy's Page".to_string())
        .with_title("nate mccoy".to_string())
        .with_aliases(vec!["Nate".to_string()])
        .create(&temp_dir, "Nate McCoy.md");

    TestFileBuilder::new()
        .with_content("# Nathan Dye's Page".to_string())
        .with_title("nathan dye".to_string())
        .with_aliases(vec!["Nate".to_string()])
        .create(&temp_dir, "Nathan Dye.md");

    // Let scan_folders find all the files and process them
    let repo_info = scan_folders(&config).unwrap();
    let matches = find_all_back_populate_matches(&config, &repo_info).unwrap();

    // Filter matches for other.md
    let other_matches: Vec<_> = matches
        .iter()
        .filter(|m| m.relative_path == "other.md")
        .collect();

    // Assert total matches
    assert_eq!(
        other_matches.len(),
        4,
        "Should match 'Karen', 'karen', and both 'Nate' instances"
    );

    // Verify unambiguous matches
    let karen_match = other_matches
        .iter()
        .find(|m| m.found_text == "Karen")
        .expect("Should find uppercase Karen");
    assert_eq!(
        karen_match.replacement, "[[Karen McCoy|Karen]]",
        "Should replace uppercase Karen correctly"
    );

    let karen_lower_match = other_matches
        .iter()
        .find(|m| m.found_text == "karen")
        .expect("Should find lowercase karen");
    assert_eq!(
        karen_lower_match.replacement, "[[Karen McCoy|karen]]",
        "Should replace lowercase karen correctly"
    );

    // Verify ambiguous matches
    let nate_matches: Vec<_> = other_matches
        .iter()
        .filter(|m| m.found_text == "Nate")
        .collect();
    assert_eq!(
        nate_matches.len(),
        2,
        "Should find both 'Nate' instances as ambiguous"
    );

    for m in &nate_matches {
        assert!(
            m.replacement.contains("[[Nate McCoy|Nate]]")
                || m.replacement.contains("[[Nathan Dye|Nate]]"),
            "Replacement should map to one of the ambiguous targets"
        );
    }
}

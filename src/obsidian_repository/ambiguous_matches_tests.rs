use crate::markdown_file::back_populate_tests;
use crate::markdown_file::{BackPopulateMatch, MarkdownFile};
use crate::obsidian_repository::ObsidianRepository;
use crate::test_utils::TestFileBuilder;
use crate::wikilink::Wikilink;

#[test]
fn test_identify_ambiguous_matches() {
    let (temp_dir, config, mut repository) =
        back_populate_tests::create_test_environment(false, None, Some(vec![]), None);

    // Set up aliases that make "Ed" ambiguous
    repository.wikilinks_sorted = vec![
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

    // Create test files
    TestFileBuilder::new()
        .with_content("Ed wrote this")
        .create(&temp_dir, "test1.md");

    TestFileBuilder::new()
        .with_content("Unique wrote this")
        .create(&temp_dir, "test2.md");

    // Set up initial matches in test1.md
    let mut test_file = MarkdownFile::new(
        temp_dir.path().join("test1.md"),
        config.operational_timezone(),
    )
    .unwrap();
    test_file.matches.unambiguous = vec![BackPopulateMatch {
        relative_path: "test1.md".to_string(),
        line_number: 1,
        line_text: "Ed wrote this".to_string(),
        found_text: "Ed".to_string(),
        replacement: "[[Ed Barnes|Ed]]".to_string(),
        position: 0,
        in_markdown_table: false,
    }];

    // Set up initial matches in test2.md
    let mut test_file2 = MarkdownFile::new(
        temp_dir.path().join("test2.md"),
        config.operational_timezone(),
    )
    .unwrap();
    test_file2.matches.unambiguous = vec![BackPopulateMatch {
        relative_path: "test2.md".to_string(),
        line_number: 1,
        line_text: "Unique wrote this".to_string(),
        found_text: "Unique".to_string(),
        replacement: "[[Unique Target]]".to_string(),
        position: 0,
        in_markdown_table: false,
    }];

    repository.markdown_files.push(test_file2);
    repository.markdown_files.push(test_file);

    repository.identify_ambiguous_matches();

    // Find test1.md to check its matches
    let test_file = repository
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("test1.md"))
        .expect("Should find test1.md");

    // Verify match was moved from unambiguous to ambiguous
    assert!(
        test_file.matches.unambiguous.is_empty(),
        "Ed match should be removed from unambiguous"
    );
    assert_eq!(
        test_file.matches.ambiguous.len(),
        1,
        "Ed match should be moved to ambiguous"
    );
    let ambiguous_match = &test_file.matches.ambiguous[0];
    assert_eq!(ambiguous_match.found_text, "Ed");
    assert_eq!(ambiguous_match.line_text, "Ed wrote this");

    // Verify unambiguous match for "Unique" remains unchanged
    let test_file2 = repository
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("test2.md"))
        .expect("Should find test2.md");
    assert_eq!(
        test_file2.matches.unambiguous.len(),
        1,
        "Should have one unambiguous match"
    );
    assert_eq!(test_file2.matches.unambiguous[0].found_text, "Unique");
    assert!(
        test_file2.matches.ambiguous.is_empty(),
        "Should have no ambiguous matches"
    );
}

#[test]
fn test_truly_ambiguous_targets() {
    let (temp_dir, config, _) =
        back_populate_tests::create_test_environment(false, None, Some(vec![]), None);

    // Create the test files using TestFileBuilder
    TestFileBuilder::new()
        .with_content("Amazon is huge")
        .create(&temp_dir, "test1.md");

    TestFileBuilder::new()
        .with_content("# Amazon (company)")
        .with_title("amazon (company)".to_string())
        .with_aliases(vec!["Amazon".to_string()])
        .create(&temp_dir, "Amazon (company).md");

    TestFileBuilder::new()
        .with_content("# Amazon (river)")
        .with_title("amazon (river)".to_string())
        .with_aliases(vec!["Amazon".to_string()])
        .create(&temp_dir, "Amazon (river).md");

    // Let scan_folders find all the files and process them
    let mut repository = ObsidianRepository::new(&config).unwrap();

    // Find test1.md and verify initial state
    let test_file = repository
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("test1.md"))
        .expect("Should find test1.md");

    // Verify initial match exists in unambiguous
    assert_eq!(
        test_file.matches.unambiguous.len(),
        1,
        "Should have one initial match in unambiguous"
    );
    assert!(
        test_file.matches.ambiguous.is_empty(),
        "Should start with no ambiguous matches"
    );

    // Process ambiguous matches
    repository.identify_ambiguous_matches();

    // Find test1.md again and verify final state
    let test_file = repository
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("test1.md"))
        .expect("Should find test1.md");

    // Verify the match was moved to ambiguous
    assert!(
        test_file.matches.unambiguous.is_empty(),
        "All matches should be moved from unambiguous"
    );
    assert_eq!(
        test_file.matches.ambiguous.len(),
        1,
        "Should have one match in ambiguous"
    );

    let ambiguous_match = &test_file.matches.ambiguous[0];
    assert_eq!(ambiguous_match.found_text, "Amazon");
    assert_eq!(ambiguous_match.line_text, "Amazon is huge");
}

#[test]
fn test_mixed_case_and_truly_ambiguous() {
    let (temp_dir, config, _) =
        back_populate_tests::create_test_environment(false, None, Some(vec![]), None);

    // Create test files for case variations
    TestFileBuilder::new()
        .with_content("# AWS")
        .with_title("aws".to_string())
        .create(&temp_dir, "AWS.md");

    TestFileBuilder::new()
        .with_content("# aws")
        .with_title("aws".to_string())
        .create(&temp_dir, "aws.md");

    // Create test files for truly ambiguous targets
    TestFileBuilder::new()
        .with_content("# Amazon (company)")
        .with_title("amazon (company)".to_string())
        .with_aliases(vec!["Amazon".to_string()])
        .create(&temp_dir, "Amazon (company).md");

    TestFileBuilder::new()
        .with_content("# Amazon (river)")
        .with_title("amazon (river)".to_string())
        .with_aliases(vec!["Amazon".to_string()])
        .create(&temp_dir, "Amazon (river).md");

    // Create the test file with both types of matches
    TestFileBuilder::new()
        .with_content(
            r#"AWS and aws are the same
Amazon is ambiguous"#,
        )
        .create(&temp_dir, "test1.md");

    // Let scan_folders find all the files and process them
    let mut repository = ObsidianRepository::new(&config).unwrap();

    // Find test1.md and verify initial state
    let test_file = repository
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("test1.md"))
        .expect("Should find test1.md");

    // Verify initial matches
    assert_eq!(
        test_file.matches.unambiguous.len(),
        3,
        "Should have both AWS cases and Amazon matches initially"
    );

    // Verify we found both cases of AWS and Amazon
    let aws_matches: Vec<_> = test_file
        .matches
        .unambiguous
        .iter()
        .filter(|m| m.found_text.to_lowercase() == "aws")
        .collect();
    assert_eq!(aws_matches.len(), 2, "Should have both cases of AWS");

    let amazon_matches: Vec<_> = test_file
        .matches
        .unambiguous
        .iter()
        .filter(|m| m.found_text == "Amazon")
        .collect();
    assert_eq!(amazon_matches.len(), 1, "Should have one Amazon match");

    // Process ambiguous matches
    repository.identify_ambiguous_matches();

    // Find test1.md again and verify final state
    let test_file = repository
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("test1.md"))
        .expect("Should find test1.md");

    // Verify final state of unambiguous matches
    assert_eq!(
        test_file.matches.unambiguous.len(),
        2,
        "Both AWS case variations should remain as unambiguous"
    );

    // Verify the remaining matches are both AWS-related
    let aws_matches: Vec<_> = test_file
        .matches
        .unambiguous
        .iter()
        .filter(|m| m.found_text.to_lowercase() == "aws")
        .collect();
    assert_eq!(
        aws_matches.len(),
        2,
        "Should have both AWS case variations remaining"
    );

    // Verify Amazon was moved to ambiguous
    assert_eq!(
        test_file.matches.ambiguous.len(),
        1,
        "Should have one ambiguous match"
    );
    assert_eq!(
        test_file.matches.ambiguous[0].found_text, "Amazon",
        "Amazon should be in ambiguous matches"
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
    let (temp_dir, config, _) =
        back_populate_tests::create_test_environment(false, None, Some(vec![]), None);

    // Create the files using TestFileBuilder
    TestFileBuilder::new()
        .with_content(
            r#"# Reference Page
Karen is here
karen is here too
Nate was here and so was Nate"#
                .to_string(),
        )
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
    let mut repository = ObsidianRepository::new(&config).unwrap();

    // Find other.md and verify initial state
    let other_file = repository
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("other.md"))
        .expect("Should find other.md");

    // Verify initial Karen matches
    let karen_matches: Vec<_> = other_file
        .matches
        .unambiguous
        .iter()
        .filter(|m| m.found_text.to_lowercase() == "karen")
        .collect();
    assert_eq!(karen_matches.len(), 2, "Should have both cases of Karen");

    // Verify we have both cases
    assert!(
        karen_matches.iter().any(|m| m.found_text == "Karen"),
        "Should find uppercase Karen"
    );
    assert!(
        karen_matches.iter().any(|m| m.found_text == "karen"),
        "Should find lowercase karen"
    );

    // Verify initial Nate matches
    let nate_matches: Vec<_> = other_file
        .matches
        .unambiguous
        .iter()
        .filter(|m| m.found_text == "Nate")
        .collect();
    assert_eq!(nate_matches.len(), 2, "Should have two Nate matches");

    // Process ambiguous matches
    repository.identify_ambiguous_matches();

    // Find other.md again and verify final state
    let other_file = repository
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("other.md"))
        .expect("Should find other.md");

    // Verify Karen matches remain unambiguous
    let karen_matches: Vec<_> = other_file
        .matches
        .unambiguous
        .iter()
        .filter(|m| m.found_text.to_lowercase() == "karen")
        .collect();
    assert_eq!(
        karen_matches.len(),
        2,
        "Both Karen case variations should remain as unambiguous"
    );

    // Verify Nate matches were moved to ambiguous
    let nate_ambiguous_matches: Vec<_> = other_file
        .matches
        .ambiguous
        .iter()
        .filter(|m| m.found_text == "Nate")
        .collect();
    assert_eq!(
        nate_ambiguous_matches.len(),
        2,
        "Should have both Nate matches in ambiguous"
    );

    // Verify correct line text for Nate matches
    let nate_line_texts: Vec<_> = nate_ambiguous_matches
        .iter()
        .map(|m| m.line_text.as_str())
        .collect();
    assert!(nate_line_texts.contains(&"Nate was here and so was Nate"));
}

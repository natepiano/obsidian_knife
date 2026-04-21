use super::ObsidianRepository;
use crate::markdown_file::BackPopulateMatch;
use crate::markdown_file::MarkdownFile;
use crate::markdown_file::MatchContext;
use crate::test_support;
use crate::test_support::TestFileBuilder;
use crate::validated_config::ChangeMode;
use crate::wikilink::Wikilink;

#[test]
fn test_identify_ambiguous_matches() {
    let (temp_dir, validated_config, mut obsidian_repository) =
        test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

    // Set up aliases that make "Ed" ambiguous
    obsidian_repository.wikilinks_sorted = vec![
        Wikilink {
            display_text: "Ed".to_string(),
            target:       "Ed Barnes".to_string(),
        },
        Wikilink {
            display_text: "Ed".to_string(),
            target:       "Ed Stanfield".to_string(),
        },
        Wikilink {
            display_text: "Unique".to_string(),
            target:       "Unique Target".to_string(),
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
        validated_config.operational_timezone(),
    )
    .unwrap();
    test_file.matches.unambiguous = vec![BackPopulateMatch {
        relative_path: "test1.md".to_string(),
        line_number:   1,
        line_text:     "Ed wrote this".to_string(),
        found_text:    "Ed".to_string(),
        replacement:   "[[Ed Barnes|Ed]]".to_string(),
        position:      0,
        match_context: MatchContext::Plaintext,
    }];

    // Set up initial matches in test2.md
    let mut test_file2 = MarkdownFile::new(
        temp_dir.path().join("test2.md"),
        validated_config.operational_timezone(),
    )
    .unwrap();
    test_file2.matches.unambiguous = vec![BackPopulateMatch {
        relative_path: "test2.md".to_string(),
        line_number:   1,
        line_text:     "Unique wrote this".to_string(),
        found_text:    "Unique".to_string(),
        replacement:   "[[Unique Target]]".to_string(),
        position:      0,
        match_context: MatchContext::Plaintext,
    }];

    obsidian_repository.markdown_files.push(test_file2);
    obsidian_repository.markdown_files.push(test_file);

    obsidian_repository.identify_ambiguous_matches();

    // Find test1.md to check its matches
    let test_file = obsidian_repository
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("test1.md"))
        .expect("Should find test1.md");

    // Verify match was moved from unambiguous to ambiguous
    assert!(
        !test_file.has_unambiguous_matches(),
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
    let test_file2 = obsidian_repository
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
        !test_file2.has_ambiguous_matches(),
        "Should have no ambiguous matches"
    );
}

#[test]
fn test_truly_ambiguous_targets() {
    let (temp_dir, validated_config, _) =
        test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

    // Create the test files using `TestFileBuilder`
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

    // Let `ObsidianRepository::new` find all the files and process them.
    let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

    // Find test1.md again and verify final state
    let test_file = obsidian_repository
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("test1.md"))
        .expect("Should find test1.md");

    // Verify the match was moved to ambiguous
    assert!(
        !test_file.has_unambiguous_matches(),
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
    let (temp_dir, validated_config, _) =
        test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

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
            r"AWS and aws are the same
Amazon is ambiguous",
        )
        .with_title("Test Document".to_string()) // This adds frontmatter with the title
        .create(&temp_dir, "test1.md");

    // Let `ObsidianRepository::new` find all the files and process them.
    let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

    // Find test1.md again and verify final state
    let test_file = obsidian_repository
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
    let aws_match_count = test_file
        .matches
        .unambiguous
        .iter()
        .filter(|m| m.found_text.to_lowercase() == "aws")
        .count();
    assert_eq!(
        aws_match_count, 2,
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
// It ensures that the `identify_ambiguous_matches` function correctly **classifies** both instances
// of `"Nate"` as **ambiguous**.
//
// Validate that the function can handle **both unambiguous and ambiguous matches simultaneously**
// without interference. Prior to this, the real-world failure was that it would find `Karen` as an
// alias but not `karen` even though we have a case-insensitive search.
// The problem with the old test is that when there were no ambiguous matches, the lowercase
// `karen` was not getting stripped out and the test would pass even though the real world failed.
// In this case we are creating a more realistic test that has a mix of ambiguous and unambiguous
// matches.
#[test]
fn test_combined_ambiguous_and_unambiguous_matches() {
    let (temp_dir, validated_config, _) =
        test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

    // Create the files using `TestFileBuilder`
    TestFileBuilder::new()
        .with_content(
            r"# Reference Page
Karen is here
karen is here too
Nate was here and so was Nate"
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

    // Let `ObsidianRepository::new` find all the files and process them.
    let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

    // Find other.md again and verify final state
    let other_file = obsidian_repository
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("other.md"))
        .expect("Should find other.md");

    // Verify Karen matches remain unambiguous
    let karen_match_count = other_file
        .matches
        .unambiguous
        .iter()
        .filter(|m| m.found_text.to_lowercase() == "karen")
        .count();
    assert_eq!(
        karen_match_count, 2,
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
    assert!(
        nate_ambiguous_matches
            .iter()
            .any(|m| m.line_text == "Nate was here and so was Nate")
    );
}

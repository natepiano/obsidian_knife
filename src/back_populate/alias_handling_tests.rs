use crate::back_populate::back_populate_tests::{
    build_aho_corasick, create_markdown_test_file, create_test_environment,
    create_test_markdown_file_info,
};
use crate::back_populate::{find_all_back_populate_matches, format_relative_path};
use crate::test_utils::TestFileBuilder;
use crate::wikilink_types::Wikilink;

#[test]
fn test_alias_priority() {
    // Initialize test environment with specific wikilinks
    let wikilinks = vec![
        // Define an alias relationship: "tomatoes" is an alias for "tomato"
        Wikilink {
            display_text: "tomatoes".to_string(),
            target: "tomato".to_string(),
            is_alias: true,
        },
        // Also include a direct "tomatoes" wikilink that should not be used
        Wikilink {
            display_text: "tomatoes".to_string(),
            target: "tomatoes".to_string(),
            is_alias: false,
        },
    ];

    let (temp_dir, config, mut repo_info) =
        create_test_environment(false, None, Some(wikilinks), None);

    // Create a test file that contains the word "tomatoes"
    let content = "I love tomatoes in my salad";
    create_markdown_test_file(&temp_dir, "salad.md", content, &mut repo_info);

    // Find matches
    let matches = find_all_back_populate_matches(&config, &mut repo_info).unwrap();

    // Verify we got exactly one match
    assert_eq!(matches.len(), 1, "Should find exactly one match");

    // Verify the match uses the alias form
    let match_info = &matches[0];
    assert_eq!(match_info.found_text, "tomatoes");
    assert_eq!(
        match_info.replacement, "[[tomato|tomatoes]]",
        "Should use the alias form [[tomato|tomatoes]] instead of [[tomatoes]]"
    );
}

#[test]
fn test_no_matches_for_frontmatter_aliases() {
    let (temp_dir, config, mut repo_info) = create_test_environment(false, None, None, None);

    // Create a wikilink for testing that includes an alias
    let wikilink = Wikilink {
        display_text: "Will".to_string(),
        target: "William.md".to_string(),
        is_alias: true,
    };

    // Clear and add to the sorted vec
    repo_info.wikilinks_sorted.clear();
    repo_info.wikilinks_sorted.push(wikilink);

    // Use the helper function to build the automaton
    repo_info.wikilinks_ac = Some(build_aho_corasick(&repo_info.wikilinks_sorted));

    // Create a test file with its own name using TestFileBuilder
    let content = "Will is mentioned here but should not be replaced";
    let file_path = TestFileBuilder::new()
        .with_title("Will".to_string())
        .with_content(content.to_string())
        .create(&temp_dir, "Will.md");

    repo_info
        .markdown_files
        .push(create_test_markdown_file_info(&file_path));

    // Now, use the config returned from create_test_environment
    let matches = find_all_back_populate_matches(&config, &mut repo_info).unwrap();

    assert_eq!(
        matches.len(),
        0,
        "Should not find matches on page's own name"
    );

    // Test with different file using same text
    let other_file_path = TestFileBuilder::new()
        .with_title("Other".to_string())
        .with_content(content.to_string())
        .create(&temp_dir, "Other.md");

    repo_info
        .markdown_files
        .push(create_test_markdown_file_info(&other_file_path));

    let matches = find_all_back_populate_matches(&config, &mut repo_info).unwrap();

    assert_eq!(matches.len(), 1, "Should find match on other pages");
}

#[test]
fn test_no_self_referential_back_population() {
    // Create test environment with apply_changes set to false
    let (temp_dir, config, mut repo_info) = create_test_environment(false, None, None, None);

    // Create a wikilink for testing that includes an alias
    let wikilink = Wikilink {
        display_text: "Will".to_string(),
        target: "William.md".to_string(),
        is_alias: true,
    };

    // Update repo_info with the custom wikilink
    repo_info.wikilinks_sorted.clear();
    repo_info.wikilinks_sorted.push(wikilink);
    repo_info.wikilinks_ac = Some(build_aho_corasick(&repo_info.wikilinks_sorted));

    // Create a test file with its own name using the helper function
    let content = "Will is mentioned here but should not be replaced";
    create_markdown_test_file(&temp_dir, "Will.md", content, &mut repo_info);

    // Find matches
    let matches = find_all_back_populate_matches(&config, &mut repo_info).unwrap();

    // Should not find matches in the file itself
    assert_eq!(
        matches.len(),
        0,
        "Should not find matches on page's own name"
    );

    // Create another file using the same content
    let other_file_path = create_markdown_test_file(&temp_dir, "Other.md", content, &mut repo_info);

    // Find matches again
    let matches = find_all_back_populate_matches(&config, &mut repo_info).unwrap();

    // Should find matches in other files
    assert_eq!(matches.len(), 1, "Should find match on other pages");
    assert_eq!(
        matches[0].relative_path,
        format_relative_path(&other_file_path, config.obsidian_path()),
        "Match should be in 'Other.md'"
    );
}

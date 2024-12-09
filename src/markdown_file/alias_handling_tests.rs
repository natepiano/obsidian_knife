use crate::markdown_file::back_populate_tests::{
    build_aho_corasick, create_markdown_test_file, create_test_environment,
};
use crate::markdown_file::{format_relative_path, MarkdownFile};
use crate::test_utils::{get_test_markdown_file_info, TestFileBuilder};
use crate::wikilink::Wikilink;
use crate::DEFAULT_TIMEZONE;
use tempfile::TempDir;

#[test]
fn test_alias_priority() {
    let wikilinks = vec![
        Wikilink {
            display_text: "tomatoes".to_string(),
            target: "tomato".to_string(),
            is_alias: true,
        },
        Wikilink {
            display_text: "tomatoes".to_string(),
            target: "tomatoes".to_string(),
            is_alias: false,
        },
    ];

    let (temp_dir, config, mut repository_info) =
        create_test_environment(false, None, Some(wikilinks), None);

    let content = "I love tomatoes in my salad";
    create_markdown_test_file(&temp_dir, "salad.md", content, &mut repository_info);

    repository_info.find_all_back_populate_matches(&config);

    // Get total matches across all files
    let total_matches: usize = repository_info
        .markdown_files
        .iter()
        .map(|file| file.matches.unambiguous.len())
        .sum();

    // Verify we got exactly one match
    assert_eq!(total_matches, 1, "Should find exactly one match");

    // Find the file that has matches
    let file_with_matches = repository_info
        .markdown_files
        .iter()
        .find(|file| !file.matches.unambiguous.is_empty())
        .expect("Should have a file with matches");

    // Verify the match uses the alias form
    let first_match = &file_with_matches.matches.unambiguous[0];
    assert_eq!(first_match.found_text, "tomatoes");
    assert_eq!(
        first_match.replacement, "[[tomato|tomatoes]]",
        "Should use the alias form [[tomato|tomatoes]] instead of [[tomatoes]]"
    );
}

#[test]
fn test_no_matches_for_frontmatter_aliases() {
    let (temp_dir, config, mut repo_info) = create_test_environment(false, None, None, None);

    let wikilink = Wikilink {
        display_text: "Will".to_string(),
        target: "William.md".to_string(),
        is_alias: true,
    };

    repo_info.wikilinks_sorted.clear();
    repo_info.wikilinks_sorted.push(wikilink);
    repo_info.wikilinks_ac = Some(build_aho_corasick(&repo_info.wikilinks_sorted));

    let content = "Will is mentioned here but should not be replaced";
    let file_path = TestFileBuilder::new()
        .with_title("Will".to_string())
        .with_content(content.to_string())
        .create(&temp_dir, "Will.md");

    repo_info
        .markdown_files
        .push(get_test_markdown_file_info(file_path));

    repo_info.find_all_back_populate_matches(&config);

    // Get total matches
    let total_matches: usize = repo_info
        .markdown_files
        .iter()
        .map(|file| file.matches.unambiguous.len())
        .sum();

    assert_eq!(
        total_matches, 0,
        "Should not find matches on page's own name"
    );

    // Test with different file using same text
    let other_file_path = TestFileBuilder::new()
        .with_title("Other".to_string())
        .with_content(content.to_string())
        .create(&temp_dir, "Other.md");

    repo_info
        .markdown_files
        .push(get_test_markdown_file_info(other_file_path));

    repo_info.find_all_back_populate_matches(&config);

    // Get total matches after adding other file
    let total_matches: usize = repo_info
        .markdown_files
        .iter()
        .map(|file| file.matches.unambiguous.len())
        .sum();

    assert_eq!(total_matches, 1, "Should find match on other pages");
}

#[test]
fn test_no_self_referential_back_population() {
    let (temp_dir, config, mut repo_info) = create_test_environment(false, None, None, None);

    let wikilink = Wikilink {
        display_text: "Will".to_string(),
        target: "William.md".to_string(),
        is_alias: true,
    };

    repo_info.wikilinks_sorted.clear();
    repo_info.wikilinks_sorted.push(wikilink);
    repo_info.wikilinks_ac = Some(build_aho_corasick(&repo_info.wikilinks_sorted));

    let content = "Will is mentioned here but should not be replaced";
    create_markdown_test_file(&temp_dir, "Will.md", content, &mut repo_info);

    repo_info.find_all_back_populate_matches(&config);

    // Get total matches
    let total_matches: usize = repo_info
        .markdown_files
        .iter()
        .map(|file| file.matches.unambiguous.len())
        .sum();

    assert_eq!(
        total_matches, 0,
        "Should not find matches on page's own name"
    );

    let other_file_path = create_markdown_test_file(&temp_dir, "Other.md", content, &mut repo_info);

    repo_info.find_all_back_populate_matches(&config);

    // Get total matches after adding other file
    let total_matches: usize = repo_info
        .markdown_files
        .iter()
        .map(|file| file.matches.unambiguous.len())
        .sum();

    assert_eq!(total_matches, 1, "Should find match on other pages");

    // Find the file with matches and check its path
    let file_with_matches = repo_info
        .markdown_files
        .iter()
        .find(|file| !file.matches.unambiguous.is_empty())
        .expect("Should have a file with matches");

    assert_eq!(
        format_relative_path(&file_with_matches.path, config.obsidian_path()),
        format_relative_path(&other_file_path, config.obsidian_path()),
        "Match should be in 'Other.md'"
    );
}

#[test]
fn test_markdown_file_aliases_only() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_aliases(vec!["Only Alias".to_string()])
        .with_content("# Test Content".to_string())
        .create(&temp_dir, "test.md");

    let file_info = MarkdownFile::new(file_path, DEFAULT_TIMEZONE).unwrap();

    assert!(file_info.do_not_back_populate_regexes.is_some());
    let regexes = file_info.do_not_back_populate_regexes.unwrap();
    assert_eq!(regexes.len(), 1);

    let test_line = "Only Alias appears here";
    assert!(regexes[0].is_match(test_line));
}

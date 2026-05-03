use tempfile::TempDir;

use super::MarkdownFile;
use crate::constants::DEFAULT_TIMEZONE;
use crate::obsidian_repository;
use crate::test_support as test_utils;
use crate::test_support::TestFileBuilder;
use crate::validated_config::ChangeMode;
use crate::wikilink::Wikilink;

#[test]
fn test_alias_priority() {
    let wikilinks = vec![
        Wikilink {
            display_text: "tomatoes".to_string(),
            target:       "tomato".to_string(),
        },
        Wikilink {
            display_text: "tomatoes".to_string(),
            target:       "tomatoes".to_string(),
        },
    ];

    let (temp_dir, validated_config, mut obsidian_repository) =
        test_utils::create_test_environment(ChangeMode::DryRun, None, Some(wikilinks), None);

    let content = "I love tomatoes in my salad";
    test_utils::create_markdown_test_file(&temp_dir, "salad.md", content, &mut obsidian_repository);

    obsidian_repository
        .find_all_back_populate_matches(&validated_config)
        .unwrap();

    // Get total matches across all files
    let total_matches: usize = obsidian_repository
        .markdown_files
        .iter()
        .map(|file| file.matches.unambiguous.len())
        .sum();

    // Verify we got exactly one match
    assert_eq!(total_matches, 1, "Should find exactly one match");

    // Find the file that has matches
    let file_with_matches = obsidian_repository
        .markdown_files
        .iter()
        .find(|file| file.has_unambiguous_matches())
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
    let (temp_dir, validated_config, mut obsidian_repository) =
        test_utils::create_test_environment(ChangeMode::DryRun, None, None, None);

    let wikilink = Wikilink {
        display_text: "Will".to_string(),
        target:       "William.md".to_string(),
    };

    obsidian_repository.wikilinks_sorted.clear();
    obsidian_repository.wikilinks_sorted.push(wikilink);
    obsidian_repository.wikilinks_automaton = Some(test_utils::build_aho_corasick(
        &obsidian_repository.wikilinks_sorted,
    ));

    let content = "Will is mentioned here but should not be replaced";
    let file_path = TestFileBuilder::new()
        .with_title("Will".to_string())
        .with_content(content.to_string())
        .create(&temp_dir, "Will.md");

    obsidian_repository
        .markdown_files
        .push(test_utils::get_test_markdown_file(file_path));

    obsidian_repository
        .find_all_back_populate_matches(&validated_config)
        .unwrap();

    // Get total matches
    let total_matches: usize = obsidian_repository
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

    obsidian_repository
        .markdown_files
        .push(test_utils::get_test_markdown_file(other_file_path));

    obsidian_repository
        .find_all_back_populate_matches(&validated_config)
        .unwrap();

    // Get total matches after adding other file
    let total_matches: usize = obsidian_repository
        .markdown_files
        .iter()
        .map(|file| file.matches.unambiguous.len())
        .sum();

    assert_eq!(total_matches, 1, "Should find match on other pages");
}

#[test]
fn test_no_self_referential_back_population() {
    let (temp_dir, validated_config, mut obsidian_repository) =
        test_utils::create_test_environment(ChangeMode::DryRun, None, None, None);

    let wikilink = Wikilink {
        display_text: "Will".to_string(),
        target:       "William.md".to_string(),
    };

    obsidian_repository.wikilinks_sorted.clear();
    obsidian_repository.wikilinks_sorted.push(wikilink);
    obsidian_repository.wikilinks_automaton = Some(test_utils::build_aho_corasick(
        &obsidian_repository.wikilinks_sorted,
    ));

    let content = "Will is mentioned here but should not be replaced";
    test_utils::create_markdown_test_file(&temp_dir, "Will.md", content, &mut obsidian_repository);

    obsidian_repository
        .find_all_back_populate_matches(&validated_config)
        .unwrap();

    // Get total matches
    let total_matches: usize = obsidian_repository
        .markdown_files
        .iter()
        .map(|file| file.matches.unambiguous.len())
        .sum();

    assert_eq!(
        total_matches, 0,
        "Should not find matches on page's own name"
    );

    let other_file_path = test_utils::create_markdown_test_file(
        &temp_dir,
        "Other.md",
        content,
        &mut obsidian_repository,
    );

    obsidian_repository
        .find_all_back_populate_matches(&validated_config)
        .unwrap();

    // Get total matches after adding other file
    let total_matches: usize = obsidian_repository
        .markdown_files
        .iter()
        .map(|file| file.matches.unambiguous.len())
        .sum();

    assert_eq!(total_matches, 1, "Should find match on other pages");

    // Find the file with matches and check its path
    let file_with_matches = obsidian_repository
        .markdown_files
        .iter()
        .find(|file| file.has_unambiguous_matches())
        .expect("Should have a file with matches");

    assert_eq!(
        obsidian_repository::format_relative_path(
            &file_with_matches.path,
            validated_config.obsidian_path(),
        ),
        obsidian_repository::format_relative_path(
            &other_file_path,
            validated_config.obsidian_path(),
        ),
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

    let markdown_file = MarkdownFile::new(file_path, DEFAULT_TIMEZONE).unwrap();

    assert!(markdown_file.do_not_back_populate_regexes.is_some());
    let regexes = markdown_file.do_not_back_populate_regexes.unwrap();
    assert_eq!(regexes.len(), 1);

    let test_line = "Only Alias appears here";
    assert!(regexes[0].is_match(test_line));
}

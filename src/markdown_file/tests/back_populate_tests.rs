use tempfile::TempDir;

use super::MarkdownFile;
use crate::constants::DEFAULT_TIMEZONE;
use crate::test_support;
use crate::test_support::TestFileBuilder;
use crate::validated_config::ChangeMode;

#[test]
fn test_apply_changes() {
    let initial_content = "This is Test Link in a sentence.";
    let (_temp_dir, validated_config, mut obsidian_repository) =
        test_support::create_test_environment(ChangeMode::Apply, None, None, Some(initial_content));

    // First find the matches
    obsidian_repository
        .find_all_back_populate_matches(&validated_config)
        .unwrap();

    // Apply the changes
    obsidian_repository
        .apply_replaceable_matches(validated_config.operational_timezone())
        .unwrap();

    // Verify changes by checking `MarkdownFile` content
    assert_eq!(
        obsidian_repository.markdown_files[0].content,
        "This is [[Test Link]] in a sentence."
    );
}

#[test]
fn test_config_creation() {
    // Basic usage with defaults
    let (_, basic_config, _) =
        test_support::create_test_environment(ChangeMode::DryRun, None, None, None);
    assert_eq!(basic_config.change_mode(), ChangeMode::DryRun);

    // With apply_changes set to true
    let (_, apply_config, _) =
        test_support::create_test_environment(ChangeMode::Apply, None, None, None);
    assert_eq!(apply_config.change_mode(), ChangeMode::Apply);

    // With do_not_back_populate patterns
    let patterns = vec!["pattern1".to_string(), "pattern2".to_string()];
    let (_, pattern_config, _) = test_support::create_test_environment(
        ChangeMode::DryRun,
        Some(patterns.clone()),
        None,
        None,
    );
    let Some(regexes) = pattern_config.do_not_back_populate_regexes() else {
        panic!("expected do-not-back-populate regexes")
    };
    assert_eq!(regexes.len(), patterns.len());
    for pattern in &patterns {
        assert!(
            regexes.iter().any(|regex| regex.is_match(pattern)),
            "missing regex for pattern {pattern}"
        );
    }

    // With both parameters
    let (_, full_config, _) = test_support::create_test_environment(
        ChangeMode::Apply,
        Some(vec!["pattern".to_string()]),
        None,
        None,
    );
    assert_eq!(full_config.change_mode(), ChangeMode::Apply);
    assert!(full_config.do_not_back_populate_regexes().is_some());
}

#[test]
fn test_scan_markdown_file_with_do_not_back_populate() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content("# Test Content".to_string())
        .with_custom_frontmatter(
            r#"do_not_back_populate:
- "test phrase"
- "another phrase"
"#
            .to_string(),
        )
        .create(&temp_dir, "test.md");

    let markdown_file = MarkdownFile::new(file_path, DEFAULT_TIMEZONE).unwrap();

    assert!(markdown_file.do_not_back_populate_regexes.is_some());
    let regexes = markdown_file.do_not_back_populate_regexes.unwrap();
    assert_eq!(regexes.len(), 2);

    let test_line = "here is a test phrase and another phrase";
    assert!(regexes[0].is_match(test_line));
    assert!(regexes[1].is_match(test_line));
}

#[test]
fn test_scan_markdown_file_combines_aliases_with_do_not_back_populate() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_aliases(vec!["First Alias".to_string(), "Second Alias".to_string()])
        .with_custom_frontmatter(
            r#"do_not_back_populate:
- "exclude this"
"#
            .to_string(),
        )
        .with_content("# Test Content".to_string())
        .create(&temp_dir, "test.md");

    let markdown_file = MarkdownFile::new(file_path, DEFAULT_TIMEZONE).unwrap();

    assert!(markdown_file.do_not_back_populate_regexes.is_some());
    let regexes = markdown_file.do_not_back_populate_regexes.unwrap();
    assert_eq!(regexes.len(), 3);

    let test_line = "First Alias and Second Alias and exclude this";
    assert!(regexes[0].is_match(test_line));
    assert!(regexes[1].is_match(test_line));
    assert!(regexes[2].is_match(test_line));
}

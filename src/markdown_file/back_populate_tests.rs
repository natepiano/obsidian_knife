use crate::markdown_file::{CodeBlockTracker, MarkdownFile};
use crate::obsidian_repository::ObsidianRepository;
use crate::wikilink::Wikilink;
use crate::{ValidatedConfig, DEFAULT_TIMEZONE};

use crate::test_utils;
use crate::test_utils::TestFileBuilder;
use crate::validated_config::ValidatedConfigBuilder;
use aho_corasick::AhoCorasick;
use aho_corasick::{AhoCorasickBuilder, MatchKind};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

// Common helper function to build Aho-Corasick automaton from CompiledWikilinks
pub fn build_aho_corasick(wikilinks: &[Wikilink]) -> AhoCorasick {
    let patterns: Vec<&str> = wikilinks.iter().map(|w| w.display_text.as_str()).collect();

    AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .match_kind(MatchKind::LeftmostLongest)
        .build(&patterns)
        .expect("Failed to build Aho-Corasick automaton")
}

pub(crate) fn create_test_environment(
    apply_changes: bool,
    do_not_back_populate: Option<Vec<String>>,
    wikilinks: Option<Vec<Wikilink>>,
    initial_content: Option<&str>,
) -> (TempDir, ValidatedConfig, ObsidianRepository) {
    let temp_dir = TempDir::new().unwrap();

    let config = ValidatedConfigBuilder::default()
        .apply_changes(apply_changes)
        .do_not_back_populate(do_not_back_populate)
        .obsidian_path(temp_dir.path().to_path_buf())
        .output_folder(temp_dir.path().join("output"))
        .build()
        .unwrap();

    let mut repository = ObsidianRepository::default();

    // Create test file using TestFileBuilder but WITHOUT frontmatter
    let file_path = TestFileBuilder::new()
        //.with_matching_dates(test_utils::parse_datetime("2024-01-02 00:00:00"))
        .with_matching_dates(test_utils::eastern_midnight(2024, 1, 2))
        .with_content(
            initial_content
                .unwrap_or("Initial test content")
                .to_string(),
        )
        .create(&temp_dir, "test.md");

    let markdown_info = MarkdownFile::new(file_path, config.operational_timezone()).unwrap();
    repository.markdown_files.push(markdown_info);

    // Set up wikilinks
    if let Some(wikilinks) = wikilinks {
        repository.wikilinks_sorted = wikilinks;
    } else {
        repository.wikilinks_sorted = vec![Wikilink {
            display_text: "Test Link".to_string(),
            target: "Test Link".to_string(),
        }];
    }

    repository.wikilinks_ac = Some(build_aho_corasick(&repository.wikilinks_sorted));

    (temp_dir, config, repository)
}

pub fn create_markdown_test_file(
    temp_dir: &TempDir,
    file_name: &str,
    content: &str,
    repository: &mut ObsidianRepository,
) -> PathBuf {
    let file_path = temp_dir.path().join(file_name);
    let mut file = File::create(&file_path).unwrap();
    write!(file, "{}", content).unwrap();

    let markdown_file = test_utils::get_test_markdown_file(file_path.clone());

    repository.markdown_files.push(markdown_file);

    file_path
}

#[test]
fn test_apply_changes() {
    let initial_content = "This is Test Link in a sentence.";
    let (_temp_dir, config, mut repository) =
        create_test_environment(true, None, None, Some(initial_content));

    // First find the matches
    repository.find_all_back_populate_matches(&config);

    // Apply the changes
    repository.apply_replaceable_matches(config.operational_timezone());

    // Verify changes by checking MarkdownFile content
    assert_eq!(
        repository.markdown_files[0].content,
        "This is [[Test Link]] in a sentence."
    );
}

#[test]
fn test_config_creation() {
    // Basic usage with defaults
    let (_, basic_config, _) = create_test_environment(false, None, None, None);
    assert!(!basic_config.apply_changes());

    // With apply_changes set to true
    let (_, apply_config, _) = create_test_environment(true, None, None, None);
    assert!(apply_config.apply_changes());

    // With do_not_back_populate patterns
    let patterns = vec!["pattern1".to_string(), "pattern2".to_string()];
    let (_, pattern_config, _) = create_test_environment(false, Some(patterns.clone()), None, None);
    assert_eq!(
        pattern_config.do_not_back_populate(),
        Some(patterns.as_slice())
    );

    // With both parameters
    let (_, full_config, _) =
        create_test_environment(true, Some(vec!["pattern".to_string()]), None, None);
    assert!(full_config.apply_changes());
    assert!(full_config.do_not_back_populate().is_some());
}

#[test]
fn test_code_block_tracking() {
    let mut tracker = CodeBlockTracker::new();

    // Initial state
    assert!(!tracker.should_skip_line(), "Initial state should not skip");

    tracker.update_for_line("```rust");
    assert!(tracker.should_skip_line(), "Should skip inside code block");
    tracker.update_for_line("let x = 42;");
    assert!(tracker.should_skip_line(), "Should still be in code block");
    tracker.update_for_line("```");
    assert!(
        !tracker.should_skip_line(),
        "Should not skip after code block"
    );

    // Regular content
    tracker.update_for_line("Regular text");
    assert!(!tracker.should_skip_line(), "Should not be in code block");

    // Nested code blocks (treated as toggles)
    tracker.update_for_line("```python");
    assert!(
        tracker.should_skip_line(),
        "Should skip in second code block"
    );
    tracker.update_for_line("print('hello')");
    tracker.update_for_line("```");
    assert!(
        !tracker.should_skip_line(),
        "Should not skip after second block"
    );
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

    let file_info = MarkdownFile::new(file_path, DEFAULT_TIMEZONE).unwrap();

    assert!(file_info.do_not_back_populate_regexes.is_some());
    let regexes = file_info.do_not_back_populate_regexes.unwrap();
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

    let file_info = MarkdownFile::new(file_path, DEFAULT_TIMEZONE).unwrap();

    assert!(file_info.do_not_back_populate_regexes.is_some());
    let regexes = file_info.do_not_back_populate_regexes.unwrap();
    assert_eq!(regexes.len(), 3);

    let test_line = "First Alias and Second Alias and exclude this";
    assert!(regexes[0].is_match(test_line));
    assert!(regexes[1].is_match(test_line));
    assert!(regexes[2].is_match(test_line));
}

use crate::markdown_file_info::{FileProcessingState, MarkdownFileInfo};
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::wikilink_types::Wikilink;
use crate::ValidatedConfig;

use crate::test_utils::{get_test_markdown_file_info, parse_datetime, TestFileBuilder};
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
) -> (TempDir, ValidatedConfig, ObsidianRepositoryInfo) {
    let temp_dir = TempDir::new().unwrap();

    let config = ValidatedConfigBuilder::default()
        .apply_changes(apply_changes)
        .do_not_back_populate(do_not_back_populate)
        .obsidian_path(temp_dir.path().to_path_buf())
        .output_folder(temp_dir.path().join("output"))
        .build()
        .unwrap();

    let mut repo_info = ObsidianRepositoryInfo::default();

    // Create test file using TestFileBuilder but WITHOUT frontmatter
    let file_path = TestFileBuilder::new()
        .with_matching_dates(parse_datetime("2024-01-02 00:00:00"))
        .with_content(
            initial_content
                .unwrap_or("Initial test content")
                .to_string(),
        )
        .create(&temp_dir, "test.md");

    let markdown_info = MarkdownFileInfo::new(file_path, config.operational_timezone()).unwrap();
    repo_info.markdown_files.push(markdown_info);

    // Set up wikilinks
    if let Some(wikilinks) = wikilinks {
        repo_info.wikilinks_sorted = wikilinks;
    } else {
        repo_info.wikilinks_sorted = vec![Wikilink {
            display_text: "Test Link".to_string(),
            target: "Test Link".to_string(),
            is_alias: false,
        }];
    }

    repo_info.wikilinks_ac = Some(build_aho_corasick(&repo_info.wikilinks_sorted));

    (temp_dir, config, repo_info)
}

pub fn create_markdown_test_file(
    temp_dir: &TempDir,
    file_name: &str,
    content: &str,
    repo_info: &mut ObsidianRepositoryInfo,
) -> PathBuf {
    let file_path = temp_dir.path().join(file_name);
    let mut file = File::create(&file_path).unwrap();
    write!(file, "{}", content).unwrap();

    let markdown_file_info = get_test_markdown_file_info(file_path.clone());

    repo_info.markdown_files.push(markdown_file_info);

    file_path
}

#[test]
fn test_apply_changes() {
    let initial_content = "This is Test Link in a sentence.";
    let (_temp_dir, config, mut repo_info) =
        create_test_environment(true, None, None, Some(initial_content));

    // First find the matches
    repo_info.find_all_back_populate_matches(&config);

    // Apply the changes
    repo_info.apply_back_populate_changes();

    // Verify changes by checking MarkdownFileInfo content
    assert_eq!(
        repo_info.markdown_files[0].content,
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
fn test_file_processing_state() {
    let mut state = FileProcessingState::new();

    // Initial state
    assert!(!state.should_skip_line(), "Initial state should not skip");

    // Frontmatter
    state.update_for_line("---");
    assert!(state.should_skip_line(), "Should skip in frontmatter");
    state.update_for_line("title: Test");
    assert!(state.should_skip_line(), "Should skip frontmatter content");
    state.update_for_line("---");
    assert!(
        !state.should_skip_line(),
        "Should not skip after frontmatter"
    );

    // Code block
    state.update_for_line("```rust");
    assert!(state.should_skip_line(), "Should skip in code block");
    state.update_for_line("let x = 42;");
    assert!(state.should_skip_line(), "Should skip code block content");
    state.update_for_line("```");
    assert!(
        !state.should_skip_line(),
        "Should not skip after code block"
    );

    // Combined frontmatter and code block
    state.update_for_line("---");
    assert!(state.should_skip_line(), "Should skip in frontmatter again");
    state.update_for_line("description: complex");
    assert!(state.should_skip_line(), "Should skip frontmatter content");
    state.update_for_line("---");
    assert!(
        !state.should_skip_line(),
        "Should not skip after frontmatter"
    );

    state.update_for_line("```");
    assert!(
        state.should_skip_line(),
        "Should skip in another code block"
    );
    state.update_for_line("print('Hello')");
    assert!(state.should_skip_line(), "Should skip code block content");
    state.update_for_line("```");
    assert!(
        !state.should_skip_line(),
        "Should not skip after code block"
    );
}

use crate::back_populate::{apply_back_populate_changes, find_all_back_populate_matches};
use crate::config::ValidatedConfig;
use crate::markdown_file_info::MarkdownFileInfo;
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::wikilink_types::Wikilink;

use crate::test_utils::TestFileBuilder;
use aho_corasick::AhoCorasick;
use aho_corasick::{AhoCorasickBuilder, MatchKind};
use std::fs;
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

    let config = ValidatedConfig::new(
        apply_changes,
        None,
        None,
        do_not_back_populate,
        None,
        temp_dir.path().to_path_buf(),
        temp_dir.path().join("output"),
    );

    let mut repo_info = ObsidianRepositoryInfo::default();

    // Create test file using TestFileBuilder but WITHOUT frontmatter
    let file_path = TestFileBuilder::new()
        .with_content(
            initial_content
                .unwrap_or("Initial test content")
                .to_string(),
        )
        .create(&temp_dir, "test.md");

    let markdown_info = MarkdownFileInfo::new(file_path).unwrap();
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

    let markdown_file_info = create_test_markdown_file_info(&file_path);

    repo_info.markdown_files.push(markdown_file_info);

    file_path
}

pub(crate) fn create_test_markdown_file_info(file_path: &PathBuf) -> MarkdownFileInfo {
    MarkdownFileInfo::new(file_path.clone()).unwrap()
}

#[test]
fn test_apply_changes() {
    let content = "Here is Test Link\nNo change here\nAnother Test Link";
    let (temp_dir, config, mut repo_info) =
        create_test_environment(true, None, None, Some(content));

    // Find matches
    let matches = find_all_back_populate_matches(&config, &mut repo_info).unwrap();

    // Apply changes
    apply_back_populate_changes(&config, &matches).unwrap();

    // Verify changes
    let updated_content = fs::read_to_string(temp_dir.path().join("test.md")).unwrap();
    assert!(updated_content.contains("[[Test Link]]"));
    assert!(updated_content.contains("No change here"));
    assert_eq!(
        updated_content.matches("[[Test Link]]").count(),
        2,
        "Should have replaced both instances"
    );
}

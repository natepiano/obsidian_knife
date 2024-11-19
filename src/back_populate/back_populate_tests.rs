use crate::back_populate::{apply_back_populate_changes, find_all_back_populate_matches};
use crate::config::ValidatedConfig;
use crate::markdown_file_info::MarkdownFileInfo;
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::wikilink_types::Wikilink;

use crate::test_utils::{parse_datetime, TestFileBuilder};
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
        .with_matching_dates(parse_datetime("2024-01-02 00:00:00"))
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
    let initial_content = "This is Test Link in a sentence.";
    let (_temp_dir, config, mut repo_info) =
        create_test_environment(true, None, None, Some(initial_content));

    // First find the matches
    find_all_back_populate_matches(&config, &mut repo_info).unwrap();

    // Apply the changes
    apply_back_populate_changes(&mut repo_info).unwrap();

    // Verify changes by checking MarkdownFileInfo content
    assert_eq!(
        repo_info.markdown_files[0].content,
        "This is [[Test Link]] in a sentence."
    );
}

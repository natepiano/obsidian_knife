use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use aho_corasick::AhoCorasick;
use aho_corasick::AhoCorasickBuilder;
use aho_corasick::MatchKind;
use tempfile::TempDir;

use crate::markdown_file::MarkdownFile;
use crate::obsidian_repository::ObsidianRepository;
use crate::test_support;
use crate::validated_config::ChangeMode;
use crate::validated_config::ValidatedConfig;
use crate::validated_config::ValidatedConfigBuilder;
use crate::wikilink::Wikilink;

pub fn build_aho_corasick(wikilinks: &[Wikilink]) -> AhoCorasick {
    let patterns: Vec<&str> = wikilinks.iter().map(|w| w.display_text.as_str()).collect();

    AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .match_kind(MatchKind::LeftmostLongest)
        .build(&patterns)
        .expect("Failed to build Aho-Corasick automaton")
}

pub fn create_test_environment(
    change_mode: ChangeMode,
    do_not_back_populate: Option<Vec<String>>,
    wikilinks: Option<Vec<Wikilink>>,
    initial_content: Option<&str>,
) -> (TempDir, ValidatedConfig, ObsidianRepository) {
    let temp_dir = TempDir::new().unwrap();

    let validated_config = ValidatedConfigBuilder::default()
        .change_mode(change_mode)
        .do_not_back_populate(do_not_back_populate)
        .obsidian_path(temp_dir.path().to_path_buf())
        .output_folder(temp_dir.path().join("output"))
        .build()
        .unwrap();

    let mut obsidian_repository = ObsidianRepository::default();

    let file_path = test_support::TestFileBuilder::new()
        .with_matching_dates(test_support::eastern_midnight(2024, 1, 2))
        .with_content(
            initial_content
                .unwrap_or("Initial test content")
                .to_string(),
        )
        .create(&temp_dir, "test.md");

    let markdown_file =
        MarkdownFile::new(file_path, validated_config.operational_timezone()).unwrap();
    obsidian_repository.markdown_files.push(markdown_file);

    if let Some(wikilinks) = wikilinks {
        obsidian_repository.wikilinks_sorted = wikilinks;
    } else {
        obsidian_repository.wikilinks_sorted = vec![Wikilink {
            display_text: "Test Link".to_string(),
            target:       "Test Link".to_string(),
        }];
    }

    obsidian_repository.wikilinks_automaton =
        Some(build_aho_corasick(&obsidian_repository.wikilinks_sorted));

    (temp_dir, validated_config, obsidian_repository)
}

pub fn create_markdown_test_file(
    temp_dir: &TempDir,
    file_name: &str,
    content: &str,
    repository: &mut ObsidianRepository,
) -> PathBuf {
    let file_path = temp_dir.path().join(file_name);
    let mut file = File::create(&file_path).unwrap();
    write!(file, "{content}").unwrap();

    let markdown_file = test_support::get_test_markdown_file(file_path.clone());

    repository.markdown_files.push(markdown_file);

    file_path
}

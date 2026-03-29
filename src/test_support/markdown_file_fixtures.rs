use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use aho_corasick::AhoCorasick;
use aho_corasick::AhoCorasickBuilder;
use aho_corasick::MatchKind;
use tempfile::TempDir;

use crate::ValidatedConfig;
use crate::markdown_file::MarkdownFile;
use crate::obsidian_repository::ObsidianRepository;
use crate::test_support;
use crate::validated_config::ChangeMode;
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
    apply_changes: bool,
    do_not_back_populate: Option<Vec<String>>,
    wikilinks: Option<Vec<Wikilink>>,
    initial_content: Option<&str>,
) -> (TempDir, ValidatedConfig, ObsidianRepository) {
    let temp_dir = TempDir::new().unwrap();

    let config = ValidatedConfigBuilder::default()
        .change_mode(if apply_changes {
            ChangeMode::Apply
        } else {
            ChangeMode::DryRun
        })
        .do_not_back_populate(do_not_back_populate)
        .obsidian_path(temp_dir.path().to_path_buf())
        .output_folder(temp_dir.path().join("output"))
        .build()
        .unwrap();

    let mut repository = ObsidianRepository::default();

    let file_path = test_support::TestFileBuilder::new()
        .with_matching_dates(test_support::eastern_midnight(2024, 1, 2))
        .with_content(
            initial_content
                .unwrap_or("Initial test content")
                .to_string(),
        )
        .create(&temp_dir, "test.md");

    let markdown_info = MarkdownFile::new(file_path, config.operational_timezone()).unwrap();
    repository.markdown_files.push(markdown_info);

    if let Some(wikilinks) = wikilinks {
        repository.wikilinks_sorted = wikilinks;
    } else {
        repository.wikilinks_sorted = vec![Wikilink {
            display_text: "Test Link".to_string(),
            target:       "Test Link".to_string(),
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
    write!(file, "{content}").unwrap();

    let markdown_file = test_support::get_test_markdown_file(file_path.clone());

    repository.markdown_files.push(markdown_file);

    file_path
}

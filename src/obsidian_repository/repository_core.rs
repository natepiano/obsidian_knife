use std::collections::HashSet;
use std::error::Error;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use aho_corasick::AhoCorasick;
use aho_corasick::AhoCorasickBuilder;
use aho_corasick::MatchKind;
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;

use crate::image_file::ImageFiles;
use crate::markdown_file::MarkdownFile;
use crate::markdown_files::MarkdownFiles;
use crate::utils;
use crate::utils::Timer;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::Wikilink;

#[derive(Default)]
pub struct ObsidianRepository {
    pub markdown_files:      MarkdownFiles,
    pub image_files:         ImageFiles,
    pub wikilinks_automaton: Option<AhoCorasick>,
    pub wikilinks_sorted:    Vec<Wikilink>,
}

impl ObsidianRepository {
    pub fn new(validated_config: &ValidatedConfig) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let _timer = Timer::new("prescan+analyze");
        let ignore_folders = validated_config.ignore_folders().unwrap_or(&[]);

        let repository_files = utils::collect_repository_files(validated_config, ignore_folders)?;

        // Process markdown files
        let markdown_files = Self::initialize_markdown_files(
            &repository_files.markdown,
            validated_config.operational_timezone(),
            validated_config.file_limit(),
        )?;

        let (sorted, automaton) = Self::initialize_wikilinks(&markdown_files);

        // Initialize instance with defaults
        let mut repository = Self {
            markdown_files,
            image_files: ImageFiles::default(),
            wikilinks_automaton: Some(automaton),
            wikilinks_sorted: sorted,
        };

        repository.image_files =
            repository.initialize_image_files(&repository_files.images, validated_config)?;

        repository.analyze_repository(validated_config);

        Ok(repository)
    }

    #[allow(
        clippy::unwrap_used,
        reason = "mutex poisoning is unrecoverable — unwrap is the standard pattern"
    )]
    fn initialize_markdown_files(
        markdown_paths: &[PathBuf],
        timezone: &str,
        file_limit: Option<usize>,
    ) -> Result<MarkdownFiles, Box<dyn Error + Send + Sync>> {
        // Use `Arc<Mutex<...>>` for safe shared collection
        let markdown_files = Arc::new(Mutex::new(MarkdownFiles::default()));

        markdown_paths.par_iter().try_for_each(|file_path| {
            match MarkdownFile::new(file_path.clone(), timezone) {
                Ok(markdown_file) => {
                    markdown_files.lock().unwrap().push(markdown_file);
                    Ok(())
                },
                Err(e) => {
                    eprintln!("Error processing file {}: {e}", file_path.display());
                    Err(e)
                },
            }
        })?;

        // Extract data from `Arc<Mutex<...>>`
        let mut markdown_files = Arc::try_unwrap(markdown_files)
            .unwrap()
            .into_inner()
            .unwrap();

        markdown_files.file_limit = file_limit;

        Ok(markdown_files)
    }

    fn initialize_wikilinks(markdown_files: &MarkdownFiles) -> (Vec<Wikilink>, AhoCorasick) {
        let all_wikilinks: HashSet<Wikilink> = markdown_files
            .iter()
            .flat_map(|markdown_file| markdown_file.wikilinks.valid.clone())
            .collect();
        sort_and_build_wikilinks_automaton(all_wikilinks)
    }

    fn analyze_repository(&mut self, validated_config: &ValidatedConfig) {
        let _timer = Timer::new("analyze");
        self.find_all_back_populate_matches(validated_config);
        self.identify_ambiguous_matches();
        self.identify_image_reference_replacements();
        self.apply_replaceable_matches(validated_config.operational_timezone());
        self.mark_image_files_for_deletion();
    }

    pub fn persist(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.image_files.delete_marked()?;
        self.markdown_files.files_to_persist().persist_all()
    }
}

#[allow(
    clippy::expect_used,
    reason = "AhoCorasick build from valid wikilink strings cannot fail"
)]
fn sort_and_build_wikilinks_automaton(
    all_wikilinks: HashSet<Wikilink>,
) -> (Vec<Wikilink>, AhoCorasick) {
    let mut wikilinks: Vec<_> = all_wikilinks.into_iter().collect();
    // uses
    wikilinks.sort_unstable();

    let mut patterns = Vec::with_capacity(wikilinks.len());
    patterns.extend(wikilinks.iter().map(|w| w.display_text.as_str()));

    let automaton = AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .match_kind(MatchKind::LeftmostLongest)
        .build(&patterns)
        .expect("Failed to build Aho-Corasick automaton for wikilinks");

    (wikilinks, automaton)
}

pub fn format_relative_path(path: &Path, base_path: &Path) -> String {
    path.strip_prefix(base_path)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

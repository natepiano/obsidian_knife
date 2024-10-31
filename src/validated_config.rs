use std::path::{Path, PathBuf};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};

#[derive(Debug)]
pub struct ValidatedConfig {
    apply_changes: bool,
    back_populate_file_count: Option<usize>,
    do_not_back_populate: Option<Vec<String>>,
    do_not_back_populate_ac: Option<AhoCorasick>,
    ignore_folders: Option<Vec<PathBuf>>, // Changed from String to PathBuf
    obsidian_path: PathBuf,
    output_folder: PathBuf,
}

impl ValidatedConfig {
    pub fn new(
        apply_changes: bool,
        back_populate_file_count: Option<usize>,
        do_not_back_populate: Option<Vec<String>>,
        ignore_folders: Option<Vec<PathBuf>>,
        obsidian_path: PathBuf,
        output_folder: PathBuf,
    ) -> Self {
        // Build AC automaton if we have patterns to exclude
        let do_not_back_populate_ac = do_not_back_populate.as_ref().map(|patterns| {
            AhoCorasickBuilder::new()
                .ascii_case_insensitive(true)
                .match_kind(MatchKind::LeftmostLongest)
                .build(patterns)
                .expect("Failed to build Aho-Corasick automaton for exclusion patterns")
        });

        Self {
            apply_changes,
            back_populate_file_count,
            do_not_back_populate,
            do_not_back_populate_ac,
            ignore_folders,
            obsidian_path,
            output_folder,
        }
    }

    pub fn apply_changes(&self) -> bool {
        self.apply_changes
    }

    pub fn back_populate_file_count(&self) -> Option<usize> {
        self.back_populate_file_count
    }

    pub fn do_not_back_populate(&self) -> Option<&[String]> {
        self.do_not_back_populate.as_deref()
    }

    pub fn do_not_back_populate_ac(&self) -> Option<&AhoCorasick> {
        self.do_not_back_populate_ac.as_ref()
    }

    pub fn ignore_folders(&self) -> Option<&[PathBuf]> {  // Changed return type
        self.ignore_folders.as_deref()
    }

    pub fn obsidian_path(&self) -> &Path {
        &self.obsidian_path
    }

    pub fn output_folder(&self) -> &Path {
        &self.output_folder
    }

}

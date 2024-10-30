use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ValidatedConfig {
    apply_changes: bool,
    back_populate_file_count: Option<usize>,
    do_not_back_populate: Option<Vec<String>>,
    ignore_folders: Option<Vec<PathBuf>>,
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
        ValidatedConfig {
            apply_changes,
            back_populate_file_count,
            do_not_back_populate,
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

    pub fn ignore_folders(&self) -> Option<&[PathBuf]> {
        self.ignore_folders.as_deref()
    }

    pub fn obsidian_path(&self) -> &Path {
        &self.obsidian_path
    }

    pub fn output_folder(&self) -> &Path {
        &self.output_folder
    }
}

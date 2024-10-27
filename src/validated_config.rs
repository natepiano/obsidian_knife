use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ValidatedConfig {
    apply_changes: bool,
    back_populate_file_count: Option<usize>,
    do_not_back_populate: Option<Vec<String>>,
    ignore_folders: Option<Vec<PathBuf>>,
    ignore_rendered_text: Option<Vec<String>>,
    obsidian_path: PathBuf,
    output_folder: PathBuf,
    simplify_wikilinks: Option<Vec<String>>,
}

impl ValidatedConfig {
    pub fn new(
        apply_changes: bool,
        back_populate_file_count: Option<usize>,
        do_not_back_populate: Option<Vec<String>>,
        ignore_folders: Option<Vec<PathBuf>>,
        ignore_rendered_text: Option<Vec<String>>,
        obsidian_path: PathBuf,
        output_folder: PathBuf,
        simplify_wikilinks: Option<Vec<String>>,
    ) -> Self {
        ValidatedConfig {
            apply_changes,
            back_populate_file_count,
            do_not_back_populate,
            ignore_folders,
            ignore_rendered_text,
            obsidian_path,
            output_folder,
            simplify_wikilinks,
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

    pub fn ignore_rendered_text(&self) -> Option<&[String]> {
        self.ignore_rendered_text.as_deref()
    }

    pub fn obsidian_path(&self) -> &Path {
        &self.obsidian_path
    }

    pub fn output_folder(&self) -> &Path {
        &self.output_folder
    }

    pub fn simplify_wikilinks(&self) -> Option<&[String]> {
        self.simplify_wikilinks.as_deref()
    }
}

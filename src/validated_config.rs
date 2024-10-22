use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ValidatedConfig {
    apply_changes: bool,
    cleanup_image_files: bool,
    ignore_folders: Option<Vec<PathBuf>>,
    obsidian_path: PathBuf,
    output_folder: PathBuf,
    simplify_wikilinks: Option<Vec<String>>,
    ignore_text: Option<Vec<String>>,
}

impl ValidatedConfig {
    pub fn new(
        apply_changes: bool,
        cleanup_image_files: bool,
        ignore_folders: Option<Vec<PathBuf>>,
        obsidian_path: PathBuf,
        output_folder: PathBuf,
        simplify_wikilinks: Option<Vec<String>>,
        ignore_text: Option<Vec<String>>,
    ) -> Self {
        ValidatedConfig {
            apply_changes,
            cleanup_image_files,
            ignore_folders,
            obsidian_path,
            output_folder,
            simplify_wikilinks,
            ignore_text,
        }
    }

    pub fn apply_changes(&self) -> bool {
        self.apply_changes
    }

    pub fn cleanup_image_files(&self) -> bool {
        self.cleanup_image_files
    }

    pub fn ignore_folders(&self) -> Option<&[PathBuf]> {
        self.ignore_folders.as_deref()
    }

    pub fn ignore_text(&self) -> Option<&[String]> {
        self.ignore_text.as_deref()
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

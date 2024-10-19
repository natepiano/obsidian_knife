use std::path::{Path, PathBuf};

pub struct ValidatedConfig {
    apply_changes: bool,
    cleanup_image_files: bool,
    ignore_folders: Option<Vec<PathBuf>>,
    obsidian_path: PathBuf,
    simplify_wikilinks: Option<Vec<String>>,
}

impl ValidatedConfig {
    pub fn new(
        apply_changes: bool,
        cleanup_image_files: bool,
        ignore_folders: Option<Vec<PathBuf>>,
        obsidian_path: PathBuf,
        simplify_wikilinks: Option<Vec<String>>,
    ) -> Self {
        ValidatedConfig {
            apply_changes,
            cleanup_image_files,
            ignore_folders,
            obsidian_path,
            simplify_wikilinks,
        }
    }

    pub fn obsidian_path(&self) -> &Path {
        &self.obsidian_path
    }

    pub fn ignore_folders(&self) -> Option<&[PathBuf]> {
        self.ignore_folders.as_deref()
    }

    pub fn cleanup_image_files(&self) -> bool {
        self.cleanup_image_files
    }

    pub fn simplify_wikilinks(&self) -> Option<&[String]> {
        self.simplify_wikilinks.as_deref()
    }

    pub fn apply_changes(&self) -> bool {
        self.apply_changes
    }
}

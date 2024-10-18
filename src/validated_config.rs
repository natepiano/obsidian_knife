use std::path::{Path, PathBuf};

pub struct ValidatedConfig {
    apply_changes: bool,
    obsidian_path: PathBuf,
    ignore_folders: Option<Vec<PathBuf>>,
    cleanup_image_files: bool,
}

impl ValidatedConfig {
    pub fn new(
        destructive: bool,
        obsidian_path: PathBuf,
        ignore_folders: Option<Vec<PathBuf>>,
        cleanup_image_files: bool,
    ) -> Self {
        ValidatedConfig {
            obsidian_path,
            ignore_folders,
            cleanup_image_files,
            apply_changes: destructive,
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

    pub fn destructive(&self) -> bool {
        self.apply_changes
    }
}

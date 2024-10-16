use std::path::{Path, PathBuf};

pub struct ValidatedConfig {
    apply_changes: bool,
    obsidian_path: PathBuf,
    ignore_folders: Option<Vec<PathBuf>>,
    dedupe_images: bool,
}

impl ValidatedConfig {
    pub fn new(
        destructive: bool,
        obsidian_path: PathBuf,
        ignore_folders: Option<Vec<PathBuf>>,
        dedupe_images: bool,
    ) -> Self {
        ValidatedConfig {
            obsidian_path,
            ignore_folders,
            dedupe_images,
            apply_changes: destructive,
        }
    }

    pub fn obsidian_path(&self) -> &Path {
        &self.obsidian_path
    }

    pub fn ignore_folders(&self) -> Option<&[PathBuf]> {
        self.ignore_folders.as_deref()
    }

    pub fn dedupe_images(&self) -> bool {
        self.dedupe_images
    }

    pub fn destructive(&self) -> bool {
        self.apply_changes
    }
}

use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ValidatedConfig {
    apply_changes: bool,
    creation_date_property: Option<String>,
    ignore_folders: Option<Vec<PathBuf>>,
    ignore_text: Option<Vec<String>>,
    obsidian_path: PathBuf,
    output_folder: PathBuf,
    simplify_wikilinks: Option<Vec<String>>,
}

impl ValidatedConfig {
    pub fn new(
        apply_changes: bool,
        creation_date_property: Option<String>,
        ignore_folders: Option<Vec<PathBuf>>,
        ignore_text: Option<Vec<String>>,
        obsidian_path: PathBuf,
        output_folder: PathBuf,
        simplify_wikilinks: Option<Vec<String>>,
    ) -> Self {
        ValidatedConfig {
            apply_changes,
            creation_date_property,
            ignore_folders,
            ignore_text,
            obsidian_path,
            output_folder,
            simplify_wikilinks,
        }
    }

    pub fn apply_changes(&self) -> bool {
        self.apply_changes
    }

    // Add new getter
    pub fn creation_date_property(&self) -> Option<&str> {
        self.creation_date_property.as_deref()
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

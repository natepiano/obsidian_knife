#[cfg(test)]
mod image_file_tests;

use crate::obsidian_repository::obsidian_repository_types::ImageReferences;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct ImageFile {
    pub path: PathBuf,
    pub hash: String,
    pub references: Vec<PathBuf>,
    pub size: u64,
    pub file_type: ImageFileType,
    pub image_state: ImageState,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageFileType {
    Tiff,
    Jpeg,
    Png,
    Gif,
    WebP,
    Other(String),
}

impl ImageFileType {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "tiff" | "tif" => ImageFileType::Tiff,
            "jpg" | "jpeg" => ImageFileType::Jpeg,
            "png" => ImageFileType::Png,
            "gif" => ImageFileType::Gif,
            "webp" => ImageFileType::WebP,
            other => ImageFileType::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum ImageState {
    Tiff,
    ZeroByte,
    Unreferenced,
    #[default]
    DuplicateCandidate,
}

impl ImageFile {
    //    pub fn new(path: PathBuf, hash: String, size: u64, image_refs: &ImageReferences) -> Self {
    pub fn new(path: PathBuf, hash: String, image_refs: &ImageReferences) -> Self {
        let metadata = fs::metadata(&path).expect("Failed to get metadata");
        let size = metadata.len();

        let file_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(ImageFileType::from_extension)
            .unwrap_or_else(|| ImageFileType::Other("unknown".to_string()));

        let initial_state = if matches!(file_type, ImageFileType::Tiff) {
            ImageState::Tiff
        } else if size == 0 {
            ImageState::ZeroByte
        } else {
            ImageState::DuplicateCandidate
        };

        // Copy references from the image_refs
        let references = image_refs
            .markdown_file_references
            .iter()
            .map(|s| PathBuf::from(s))
            .collect();

        ImageFile {
            path,
            hash,
            references,
            size,
            file_type,
            image_state: initial_state,
        }
    }

    pub fn mark_as_unreferenced(&mut self) {
        self.image_state = ImageState::Unreferenced;
    }
}

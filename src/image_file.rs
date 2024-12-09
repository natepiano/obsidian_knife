#[cfg(test)]
mod image_file_tests;

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
    pub fn new(path: PathBuf, hash: String, size: u64) -> Self {
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

        ImageFile {
            path,
            hash,
            references: Vec::new(),
            size,
            file_type,
            image_state: initial_state,
        }
    }

    pub fn mark_as_unreferenced(&mut self) {
        self.image_state = ImageState::Unreferenced;
    }
}

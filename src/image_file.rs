#[cfg(test)]
mod image_file_tests;

use crate::obsidian_repository::ImageReferences;
use std::fs;
use std::path::PathBuf;

use crate::utils::EnumFilter;
use vecollect::collection;

#[derive(Default, Debug)]
#[collection(field = "files")]
pub struct ImageFiles {
    pub(crate) files: Vec<ImageFile>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageFile {
    pub path: PathBuf,
    pub hash: String,
    pub references: Vec<PathBuf>,
    pub size: u64,
    pub file_type: ImageFileType,
    pub image_state: ImageFileState,
}

impl EnumFilter for ImageFile {
    type EnumType = ImageFileState;

    fn as_enum(&self) -> &Self::EnumType {
        &self.image_state
    }
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
pub enum ImageFileState {
    #[default]
    Valid,
    Incompatible {
        reason: IncompatibilityReason,
    },
    Unreferenced,
    Duplicate {
        hash: String,
    },
    DuplicateKeeper {
        hash: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum IncompatibilityReason {
    TiffFormat,
    ZeroByte,
}

impl ImageFile {
    pub fn new(
        path: PathBuf,
        hash: String,
        image_refs: &ImageReferences,
        in_duplicate_group: bool,
        is_keeper: bool,
    ) -> Self {
        let metadata = fs::metadata(&path).expect("Failed to get metadata");
        let size = metadata.len();

        let file_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(ImageFileType::from_extension)
            .unwrap_or_else(|| ImageFileType::Other("unknown".to_string()));

        let initial_state = if matches!(file_type, ImageFileType::Tiff) {
            ImageFileState::Incompatible {
                reason: IncompatibilityReason::TiffFormat,
            }
        } else if size == 0 {
            ImageFileState::Incompatible {
                reason: IncompatibilityReason::ZeroByte,
            }
        } else if in_duplicate_group {  // Check duplicate status first!
            if is_keeper {
                ImageFileState::DuplicateKeeper { hash: hash.clone() }
            } else {
                ImageFileState::Duplicate { hash: hash.clone() }
            }
        } else if image_refs.markdown_file_references.is_empty() {  // Only check unreferenced if not a duplicate
            ImageFileState::Unreferenced
        } else {
            ImageFileState::Valid
        };

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
}

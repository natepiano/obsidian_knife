#[cfg(test)]
mod image_file_tests;

use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;

use derive_more::Deref;
use derive_more::DerefMut;
use derive_more::IntoIterator;
use serde::Deserialize;
use serde::Serialize;

use crate::utils::EnumFilter;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ImageHash(pub String);

impl From<&str> for ImageHash {
    fn from(hash: &str) -> Self { ImageHash(hash.to_string()) }
}

impl From<String> for ImageHash {
    fn from(hash: String) -> Self { ImageHash(hash) }
}

impl fmt::Display for ImageHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

#[derive(Default, Debug, PartialEq, Deref, DerefMut, IntoIterator)]
pub struct ImageFiles {
    #[deref]
    #[deref_mut]
    #[into_iterator]
    pub(crate) files: Vec<ImageFile>,
}

impl FromIterator<ImageFile> for ImageFiles {
    fn from_iter<I: IntoIterator<Item = ImageFile>>(iter: I) -> Self {
        Self {
            files: iter.into_iter().collect(),
        }
    }
}

impl ImageFiles {
    pub fn delete_marked(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.files
            .iter()
            .filter(|file| file.delete)
            .try_for_each(|file| fs::remove_file(&file.path))?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageFile {
    pub delete:                   bool,
    pub file_type:                ImageFileType,
    pub hash:                     ImageHash,
    pub image_state:              ImageFileState,
    pub path:                     PathBuf,
    pub markdown_file_references: Vec<PathBuf>,
    pub size:                     u64,
}

impl EnumFilter for ImageFile {
    type EnumType = ImageFileState;

    fn as_enum(&self) -> &Self::EnumType { &self.image_state }
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
        hash: ImageHash,
    },
    DuplicateKeeper {
        hash: ImageHash,
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
        hash: ImageHash,
        markdown_file_references: Vec<PathBuf>,
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
        } else if in_duplicate_group {
            // Check duplicate status first!
            if is_keeper {
                ImageFileState::DuplicateKeeper { hash: hash.clone() }
            } else {
                ImageFileState::Duplicate { hash: hash.clone() }
            }
        } else if markdown_file_references.is_empty() {
            // Only check unreferenced if not a duplicate
            ImageFileState::Unreferenced
        } else {
            ImageFileState::Valid
        };

        ImageFile {
            delete: false,
            file_type,
            hash,
            image_state: initial_state,
            path,
            markdown_file_references,
            size,
        }
    }
}

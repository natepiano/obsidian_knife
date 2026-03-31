#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions use unwrap/expect/panic for clarity"
)]
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
    fn from(hash: &str) -> Self { Self(hash.to_string()) }
}

impl From<String> for ImageHash {
    fn from(hash: String) -> Self { Self(hash) }
}

impl fmt::Display for ImageHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DeletionStatus {
    #[default]
    Keep,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DuplicateRole {
    NotDuplicate,
    Duplicate,
    Original,
}

#[derive(Default, Debug, PartialEq, Eq, Deref, DerefMut, IntoIterator)]
pub struct ImageFiles {
    #[deref]
    #[deref_mut]
    #[into_iterator]
    pub(super) files: Vec<ImageFile>,
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
            .filter(|file| file.deletion == DeletionStatus::Delete)
            .try_for_each(|file| fs::remove_file(&file.path))?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFile {
    pub deletion:                 DeletionStatus,
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
            "tiff" | "tif" => Self::Tiff,
            "jpg" | "jpeg" => Self::Jpeg,
            "png" => Self::Png,
            "gif" => Self::Gif,
            "webp" => Self::WebP,
            other => Self::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncompatibilityReason {
    TiffFormat,
    ZeroByte,
}

impl ImageFile {
    #[allow(
        clippy::expect_used,
        reason = "path existence is validated before ImageFile construction"
    )]
    pub fn new(
        path: PathBuf,
        hash: ImageHash,
        markdown_file_references: Vec<PathBuf>,
        duplicate_role: DuplicateRole,
    ) -> Self {
        let metadata = fs::metadata(&path).expect("Failed to get metadata");
        let size = metadata.len();

        let file_type = path.extension().and_then(|ext| ext.to_str()).map_or_else(
            || ImageFileType::Other("unknown".to_string()),
            ImageFileType::from_extension,
        );

        let initial_state = if matches!(file_type, ImageFileType::Tiff) {
            ImageFileState::Incompatible {
                reason: IncompatibilityReason::TiffFormat,
            }
        } else if size == 0 {
            ImageFileState::Incompatible {
                reason: IncompatibilityReason::ZeroByte,
            }
        } else {
            match duplicate_role {
                DuplicateRole::Original => ImageFileState::DuplicateKeeper { hash: hash.clone() },
                DuplicateRole::Duplicate => ImageFileState::Duplicate { hash: hash.clone() },
                DuplicateRole::NotDuplicate => {
                    if markdown_file_references.is_empty() {
                        ImageFileState::Unreferenced
                    } else {
                        ImageFileState::Valid
                    }
                },
            }
        };

        Self {
            deletion: DeletionStatus::Keep,
            file_type,
            hash,
            image_state: initial_state,
            path,
            markdown_file_references,
            size,
        }
    }
}

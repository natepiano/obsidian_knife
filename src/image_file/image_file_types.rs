use std::fmt;

use serde::Deserialize;
use serde::Serialize;

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

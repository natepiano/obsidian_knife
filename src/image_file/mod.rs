mod image;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod image_file_tests;

use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use derive_more::Deref;
use derive_more::DerefMut;
use derive_more::IntoIterator;

pub use self::image::DeletionStatus;
pub use self::image::DuplicateRole;
pub use self::image::ImageFileState;
use self::image::ImageFileType;
pub use self::image::ImageHash;
pub use self::image::IncompatibilityReason;
use crate::utils::EnumFilter;

#[derive(Default, Debug, PartialEq, Eq, Deref, DerefMut, IntoIterator)]
pub(crate) struct ImageFiles {
    #[deref]
    #[deref_mut]
    #[into_iterator]
    pub(super) images: Vec<ImageFile>,
}

impl FromIterator<ImageFile> for ImageFiles {
    fn from_iter<I: IntoIterator<Item = ImageFile>>(iter: I) -> Self {
        Self {
            images: iter.into_iter().collect(),
        }
    }
}

impl ImageFiles {
    pub(crate) fn delete_marked(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.images
            .iter()
            .filter(|file| file.deletion == DeletionStatus::Delete)
            .try_for_each(|file| fs::remove_file(&file.path))?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImageFile {
    pub deletion:                 DeletionStatus,
    pub file_type:                ImageFileType,
    pub hash:                     ImageHash,
    pub state:                    ImageFileState,
    pub path:                     PathBuf,
    pub markdown_file_references: Vec<PathBuf>,
    pub size:                     u64,
}

impl EnumFilter for ImageFile {
    type EnumType = ImageFileState;

    fn as_enum(&self) -> &Self::EnumType { &self.state }
}

impl ImageFile {
    #[allow(
        clippy::expect_used,
        reason = "path existence is validated before ImageFile construction"
    )]
    pub(crate) fn new(
        path: PathBuf,
        hash: ImageHash,
        markdown_file_references: Vec<PathBuf>,
        duplicate_role: DuplicateRole,
    ) -> Self {
        let metadata = fs::metadata(&path).expect("Failed to get metadata");
        let size = metadata.len();

        let file_type = path.extension().and_then(OsStr::to_str).map_or_else(
            || ImageFileType::Other("unknown".to_string()),
            ImageFileType::from,
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
            state: initial_state,
            path,
            markdown_file_references,
            size,
        }
    }
}

use std::path::PathBuf;

use super::ImageFileState;
use super::ImageFileType;
use super::ImageHash;
use super::IncompatibilityReason;
use crate::constants::EMPTY_FILE_SIZE_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageRole {
    Unique,
    Duplicate,
    Original,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReferencePresence {
    Present,
    Empty,
}

impl ReferencePresence {
    const fn from_references(references: &[PathBuf]) -> Self {
        if references.is_empty() {
            Self::Empty
        } else {
            Self::Present
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InitialImageState {
    Tiff,
    EmptyFile,
    Role {
        image_role:         ImageRole,
        reference_presence: ReferencePresence,
    },
}

impl InitialImageState {
    const fn from_parts(
        kind: &ImageFileType,
        size: u64,
        references: &[PathBuf],
        image_role: ImageRole,
    ) -> Self {
        match kind {
            ImageFileType::Tiff => Self::Tiff,
            _ => match size {
                EMPTY_FILE_SIZE_BYTES => Self::EmptyFile,
                _ => Self::Role {
                    image_role,
                    reference_presence: ReferencePresence::from_references(references),
                },
            },
        }
    }

    fn into_image_file_state(self, image_hash: &ImageHash) -> ImageFileState {
        match self {
            Self::Tiff => ImageFileState::Incompatible {
                reason: IncompatibilityReason::TiffFormat,
            },
            Self::EmptyFile => ImageFileState::Incompatible {
                reason: IncompatibilityReason::ZeroByte,
            },
            Self::Role {
                image_role: ImageRole::Original,
                ..
            } => ImageFileState::DuplicateKeeper {
                image_hash: image_hash.clone(),
            },
            Self::Role {
                image_role: ImageRole::Duplicate,
                ..
            } => ImageFileState::Duplicate {
                image_hash: image_hash.clone(),
            },
            Self::Role {
                image_role: ImageRole::Unique,
                reference_presence: ReferencePresence::Empty,
            } => ImageFileState::Unreferenced,
            Self::Role {
                image_role: ImageRole::Unique,
                reference_presence: ReferencePresence::Present,
            } => ImageFileState::Valid,
        }
    }
}

pub(super) fn image_file_state_from_parts(
    kind: &ImageFileType,
    size: u64,
    references: &[PathBuf],
    image_role: ImageRole,
    image_hash: &ImageHash,
) -> ImageFileState {
    InitialImageState::from_parts(kind, size, references, image_role)
        .into_image_file_state(image_hash)
}

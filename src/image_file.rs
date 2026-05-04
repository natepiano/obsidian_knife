use std::error::Error;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::PathBuf;

use derive_more::Deref;
use derive_more::DerefMut;
use derive_more::IntoIterator;
use serde::Deserialize;
use serde::Serialize;

use crate::constants::GIF_EXTENSION;
use crate::constants::JPEG_EXTENSION;
use crate::constants::JPG_EXTENSION;
use crate::constants::PNG_EXTENSION;
use crate::constants::TIF_EXTENSION;
use crate::constants::TIFF_EXTENSION;
use crate::constants::UNKNOWN;
use crate::constants::WEBP_EXTENSION;
use crate::vec_enum_filter::EnumFilter;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct ImageHash(pub String);

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
pub(crate) enum DeletionStatus {
    #[default]
    Keep,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DuplicateRole {
    NotDuplicate,
    Duplicate,
    Original,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ImageFileType {
    Tiff,
    Jpeg,
    Png,
    Gif,
    WebP,
    Other(String),
}

impl From<&str> for ImageFileType {
    fn from(extension: &str) -> Self {
        match extension.to_lowercase().as_str() {
            TIFF_EXTENSION | TIF_EXTENSION => Self::Tiff,
            JPG_EXTENSION | JPEG_EXTENSION => Self::Jpeg,
            PNG_EXTENSION => Self::Png,
            GIF_EXTENSION => Self::Gif,
            WEBP_EXTENSION => Self::WebP,
            other => Self::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) enum ImageFileState {
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
pub(crate) enum IncompatibilityReason {
    TiffFormat,
    ZeroByte,
}

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
    pub deletion:   DeletionStatus,
    pub kind:       ImageFileType,
    pub hash:       ImageHash,
    pub state:      ImageFileState,
    pub path:       PathBuf,
    pub references: Vec<PathBuf>,
    pub size:       u64,
}

impl EnumFilter for ImageFile {
    type EnumType = ImageFileState;

    fn as_enum(&self) -> &Self::EnumType { &self.state }
}

impl ImageFile {
    pub(crate) fn new(
        path: PathBuf,
        hash: ImageHash,
        references: Vec<PathBuf>,
        duplicate_role: DuplicateRole,
    ) -> std::io::Result<Self> {
        let metadata = fs::metadata(&path)?;
        let size = metadata.len();

        let kind = path.extension().and_then(OsStr::to_str).map_or_else(
            || ImageFileType::Other(UNKNOWN.to_string()),
            ImageFileType::from,
        );

        let initial_state = if matches!(kind, ImageFileType::Tiff) {
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
                    if references.is_empty() {
                        ImageFileState::Unreferenced
                    } else {
                        ImageFileState::Valid
                    }
                },
            }
        };

        Ok(Self {
            deletion: DeletionStatus::Keep,
            kind,
            hash,
            state: initial_state,
            path,
            references,
            size,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::ffi::OsStr;
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;
    use crate::test_support::TestFileBuilder;

    #[test]
    fn test_image_file_type_from_extension() {
        let test_cases = vec![
            ("test.jpg", ImageFileType::Jpeg),
            ("test.jpeg", ImageFileType::Jpeg),
            ("test.png", ImageFileType::Png),
            ("test.tiff", ImageFileType::Tiff),
            ("test.tif", ImageFileType::Tiff),
            ("test.gif", ImageFileType::Gif),
            ("test.webp", ImageFileType::WebP),
            ("test.xyz", ImageFileType::Other("xyz".to_string())),
            ("test", ImageFileType::Other("unknown".to_string())),
        ];

        for (filename, expected_type) in test_cases {
            let path = PathBuf::from(filename);
            let extension = path
                .extension()
                .and_then(OsStr::to_str)
                .unwrap_or("unknown");

            let image_type = ImageFileType::from(extension);
            assert_eq!(
                image_type, expected_type,
                "Failed for extension: {extension}"
            );
        }
    }

    #[test]
    fn test_create_image_file() {
        let temp_dir = TempDir::new().unwrap();

        let test_cases = vec![
            (
                "image1.jpg",
                "hash1",
                vec![0xFF, 0xD8, 0xFF, 0xE0],
                vec!["note1.md", "note2.md"],
                ImageFileType::Jpeg,
                ImageFileState::Valid,
                DuplicateRole::NotDuplicate,
            ),
            (
                "image2.png",
                "hash2",
                vec![0x89, 0x50, 0x4E, 0x47],
                vec![],
                ImageFileType::Png,
                ImageFileState::Unreferenced,
                DuplicateRole::NotDuplicate,
            ),
            (
                "image3.tiff",
                "hash3",
                vec![0x4D, 0x4D, 0x00, 0x2A],
                vec!["note3.md"],
                ImageFileType::Tiff,
                ImageFileState::Incompatible {
                    reason: IncompatibilityReason::TiffFormat,
                },
                DuplicateRole::NotDuplicate,
            ),
            (
                "image4.jpg",
                "hash4",
                vec![],
                vec!["note4.md"],
                ImageFileType::Jpeg,
                ImageFileState::Incompatible {
                    reason: IncompatibilityReason::ZeroByte,
                },
                DuplicateRole::NotDuplicate,
            ),
            (
                "image5",
                "hash5",
                vec![0x00, 0x01, 0x02, 0x03],
                vec!["note5.md"],
                ImageFileType::Other("unknown".to_string()),
                ImageFileState::Valid,
                DuplicateRole::NotDuplicate,
            ),
        ];

        for (filename, hash, content, references, expected_type, expected_state, duplicate_role) in
            test_cases
        {
            let path = TestFileBuilder::new()
                .with_content(content)
                .create(&temp_dir, filename);

            let references: Vec<PathBuf> = references.into_iter().map(PathBuf::from).collect();

            let image_hash = ImageHash::from(hash);

            let image_file =
                ImageFile::new(path.clone(), image_hash.clone(), references, duplicate_role)
                    .unwrap();

            assert_eq!(image_file.path, path);
            assert_eq!(image_file.hash, image_hash);
            assert_eq!(image_file.size, fs::metadata(&path).unwrap().len());
            assert_eq!(image_file.kind, expected_type);
            assert_eq!(image_file.state, expected_state);
        }
    }

    #[test]
    fn test_incompatible_states() {
        let temp_dir = TempDir::new().unwrap();

        let tiff_path = TestFileBuilder::new()
            .with_content(vec![0x4D, 0x4D, 0x00, 0x2A])
            .create(&temp_dir, "test.tiff");
        let tiff_image = ImageFile::new(
            tiff_path,
            ImageHash::from("hash1"),
            vec![],
            DuplicateRole::NotDuplicate,
        )
        .unwrap();
        assert!(matches!(
            tiff_image.state,
            ImageFileState::Incompatible {
                reason: IncompatibilityReason::TiffFormat,
            }
        ));

        let zero_byte_path = TestFileBuilder::new()
            .with_content(vec![])
            .create(&temp_dir, "test.jpg");
        let zero_byte_image = ImageFile::new(
            zero_byte_path,
            ImageHash::from("hash2"),
            vec![PathBuf::from("note.md")],
            DuplicateRole::NotDuplicate,
        )
        .unwrap();
        assert!(matches!(
            zero_byte_image.state,
            ImageFileState::Incompatible {
                reason: IncompatibilityReason::ZeroByte,
            }
        ));
    }

    #[test]
    fn test_reference_state_determination() {
        let temp_dir = TempDir::new().unwrap();
        let path = TestFileBuilder::new()
            .with_content(vec![0xFF, 0xD8, 0xFF, 0xE0])
            .create(&temp_dir, "test.jpg");

        let unreferenced = ImageFile::new(
            path.clone(),
            ImageHash::from("hash1"),
            vec![],
            DuplicateRole::NotDuplicate,
        )
        .unwrap();
        assert_eq!(unreferenced.state, ImageFileState::Unreferenced);

        let referenced = ImageFile::new(
            path,
            ImageHash::from("hash2"),
            vec![PathBuf::from("note.md")],
            DuplicateRole::NotDuplicate,
        )
        .unwrap();
        assert_eq!(referenced.state, ImageFileState::Valid);
    }

    #[test]
    fn test_equality_and_cloning() {
        let temp_dir = TempDir::new().unwrap();

        let references = vec![PathBuf::from("test_note.md")];

        let original_path = TestFileBuilder::new()
            .with_content(vec![0xFF, 0xD8, 0xFF, 0xE0])
            .create(&temp_dir, "test.jpg");

        let original = ImageFile::new(
            original_path,
            ImageHash::from("testhash"),
            references.clone(),
            DuplicateRole::NotDuplicate,
        )
        .unwrap();

        let cloned = original.clone();
        assert_eq!(original, cloned, "Cloned ImageFile should equal original");

        let different_path = TestFileBuilder::new()
            .with_content(vec![0x89, 0x50, 0x4E, 0x47])
            .create(&temp_dir, "different.jpg");

        let different = ImageFile::new(
            different_path,
            ImageHash::from("differenthash"),
            references,
            DuplicateRole::NotDuplicate,
        )
        .unwrap();
        assert_ne!(
            original, different,
            "Different ImageFile instances should not be equal"
        );
    }

    #[test]
    fn test_image_file_debug() {
        let temp_dir = TempDir::new().unwrap();

        let path = TestFileBuilder::new()
            .with_content(vec![0xFF, 0xD8, 0xFF, 0xE0])
            .create(&temp_dir, "test.jpg");

        let references = vec![PathBuf::from("test_note.md")];

        let image_file = ImageFile::new(
            path.clone(),
            ImageHash::from("testhash"),
            references,
            DuplicateRole::NotDuplicate,
        )
        .unwrap();

        let debug_string = format!("{image_file:?}");

        assert!(
            debug_string.contains("test.jpg"),
            "Debug output should contain filename"
        );
        assert!(
            debug_string.contains("testhash"),
            "Debug output should contain hash"
        );
        assert!(
            debug_string.contains(&fs::metadata(&path).unwrap().len().to_string()),
            "Debug output should contain size"
        );
    }
}

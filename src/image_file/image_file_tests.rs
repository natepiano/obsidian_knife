use std::ffi::OsStr;
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

        let file_type = ImageFileType::from_extension(extension);
        assert_eq!(
            file_type, expected_type,
            "Failed for extension: {extension}"
        );
    }
}

#[test]
fn test_create_image_file() {
    let temp_dir = TempDir::new().unwrap();

    let test_cases = vec![
        // Regular JPEG with references
        (
            "image1.jpg",
            "hash1",
            vec![0xFF, 0xD8, 0xFF, 0xE0],
            vec!["note1.md", "note2.md"],
            ImageFileType::Jpeg,
            ImageFileState::Valid,
            DuplicateRole::NotDuplicate,
        ),
        // PNG with no references
        (
            "image2.png",
            "hash2",
            vec![0x89, 0x50, 0x4E, 0x47],
            vec![],
            ImageFileType::Png,
            ImageFileState::Unreferenced,
            DuplicateRole::NotDuplicate,
        ),
        // TIFF file (should be incompatible regardless of references)
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
        // Zero-byte file (should be incompatible regardless of references)
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
        // Unknown type with references
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
            ImageFile::new(path.clone(), image_hash.clone(), references, duplicate_role);

        assert_eq!(image_file.path, path);
        assert_eq!(image_file.hash, image_hash);
        assert_eq!(image_file.size, fs::metadata(&path).unwrap().len());
        assert_eq!(image_file.file_type, expected_type);
        assert_eq!(image_file.state, expected_state);
    }
}

#[test]
fn test_incompatible_states() {
    let temp_dir = TempDir::new().unwrap();

    // Test TIFF incompatibility
    let tiff_path = TestFileBuilder::new()
        .with_content(vec![0x4D, 0x4D, 0x00, 0x2A])
        .create(&temp_dir, "test.tiff");
    let tiff_image = ImageFile::new(
        tiff_path,
        ImageHash::from("hash1"),
        vec![], // No references
        DuplicateRole::NotDuplicate,
    );
    assert!(matches!(
        tiff_image.state,
        ImageFileState::Incompatible {
            reason: IncompatibilityReason::TiffFormat,
        }
    ));

    // Test zero-byte incompatibility
    let zero_byte_path = TestFileBuilder::new()
        .with_content(vec![]) // Zero-byte content
        .create(&temp_dir, "test.jpg");
    let zero_byte_image = ImageFile::new(
        zero_byte_path,
        ImageHash::from("hash2"),
        vec![PathBuf::from("note.md")], // Single reference
        DuplicateRole::NotDuplicate,
    );
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

    // Test with no references
    let unreferenced = ImageFile::new(
        path.clone(),
        ImageHash::from("hash1"),
        vec![],
        DuplicateRole::NotDuplicate,
    );
    assert_eq!(unreferenced.state, ImageFileState::Unreferenced);

    // Test with references
    let referenced = ImageFile::new(
        path,
        ImageHash::from("hash2"),
        vec![PathBuf::from("note.md")], // Use Vec<PathBuf>
        DuplicateRole::NotDuplicate,
    );
    assert_eq!(referenced.state, ImageFileState::Valid);
}

#[test]
fn test_equality_and_cloning() {
    let temp_dir = TempDir::new().unwrap();

    let references = vec![PathBuf::from("test_note.md")];

    let original_path = TestFileBuilder::new()
        .with_content(vec![0xFF, 0xD8, 0xFF, 0xE0]) // JPEG header
        .create(&temp_dir, "test.jpg");

    let original = ImageFile::new(
        original_path,
        ImageHash::from("testhash"),
        references.clone(), // Use references directly
        DuplicateRole::NotDuplicate,
    );

    let cloned = original.clone();
    assert_eq!(original, cloned, "Cloned ImageFile should equal original");

    // Test inequality
    let different_path = TestFileBuilder::new()
        .with_content(vec![0x89, 0x50, 0x4E, 0x47]) // PNG header
        .create(&temp_dir, "different.jpg");

    let different = ImageFile::new(
        different_path,
        ImageHash::from("differenthash"),
        references, // Same references
        DuplicateRole::NotDuplicate,
    );
    assert_ne!(
        original, different,
        "Different ImageFile instances should not be equal"
    );
}

#[test]
fn test_image_file_debug() {
    let temp_dir = TempDir::new().unwrap();

    let path = TestFileBuilder::new()
        .with_content(vec![0xFF, 0xD8, 0xFF, 0xE0]) // JPEG header
        .create(&temp_dir, "test.jpg");

    let references = vec![PathBuf::from("test_note.md")];

    let info = ImageFile::new(
        path.clone(),
        ImageHash::from("testhash"),
        references, // Directly pass references
        DuplicateRole::NotDuplicate,
    );

    let debug_str = format!("{info:?}");

    assert!(
        debug_str.contains("test.jpg"),
        "Debug output should contain filename"
    );
    assert!(
        debug_str.contains("testhash"),
        "Debug output should contain hash"
    );
    assert!(
        debug_str.contains(&fs::metadata(&path).unwrap().len().to_string()),
        "Debug output should contain size"
    );
}

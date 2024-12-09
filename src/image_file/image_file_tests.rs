use super::*;
use crate::test_utils::TestFileBuilder;
use std::path::PathBuf;
use tempfile::TempDir;

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
            .and_then(|ext| ext.to_str())
            .unwrap_or("unknown");

        let file_type = ImageFileType::from_extension(extension);
        assert_eq!(
            file_type, expected_type,
            "Failed for extension: {}",
            extension
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
            ImageState::DuplicateCandidate,
        ),
        // PNG with no references
        (
            "image2.png",
            "hash2",
            vec![0x89, 0x50, 0x4E, 0x47],
            vec![],
            ImageFileType::Png,
            ImageState::Unreferenced,
        ),
        // TIFF file (should be incompatible regardless of references)
        (
            "image3.tiff",
            "hash3",
            vec![0x4D, 0x4D, 0x00, 0x2A],
            vec!["note3.md"],
            ImageFileType::Tiff,
            ImageState::Incompatible {
                reason: IncompatibilityReason::TiffFormat,
            },
        ),
        // Zero-byte file (should be incompatible regardless of references)
        (
            "image4.jpg",
            "hash4",
            vec![],
            vec!["note4.md"],
            ImageFileType::Jpeg,
            ImageState::Incompatible {
                reason: IncompatibilityReason::ZeroByte,
            },
        ),
        // Unknown type with references
        (
            "image5",
            "hash5",
            vec![0x00, 0x01, 0x02, 0x03],
            vec!["note5.md"],
            ImageFileType::Other("unknown".to_string()),
            ImageState::DuplicateCandidate,
        ),
    ];

    for (filename, hash, content, references, expected_type, expected_state) in test_cases {
        let path = TestFileBuilder::new()
            .with_content(content)
            .create(&temp_dir, filename);

        let mut image_refs = ImageReferences::default();
        image_refs.markdown_file_references = references.into_iter().map(String::from).collect();

        let info = ImageFile::new(path.clone(), hash.to_string(), &image_refs);

        assert_eq!(info.path, path);
        assert_eq!(info.hash, hash);
        assert_eq!(info.size, fs::metadata(&path).unwrap().len());
        assert_eq!(info.file_type, expected_type);
        assert_eq!(info.image_state, expected_state);
        assert_eq!(
            info.references.len(),
            image_refs.markdown_file_references.len()
        );
    }
}

#[test]
fn test_incompatible_states() {
    let temp_dir = TempDir::new().unwrap();

    // Test TIFF incompatibility
    let tiff_path = TestFileBuilder::new()
        .with_content(vec![0x4D, 0x4D, 0x00, 0x2A])
        .create(&temp_dir, "test.tiff");
    let tiff_refs = ImageReferences::default();
    let tiff_image = ImageFile::new(tiff_path, "hash1".to_string(), &tiff_refs);
    assert!(matches!(
        tiff_image.image_state,
        ImageState::Incompatible {
            reason: IncompatibilityReason::TiffFormat
        }
    ));

    // Test zero-byte incompatibility
    let zero_byte_path = TestFileBuilder::new()
        .with_content(vec![])
        .create(&temp_dir, "test.jpg");
    let zero_byte_refs = ImageReferences {
        markdown_file_references: vec!["note.md".to_string()],
        hash: "hash2".to_string(),
    };
    let zero_byte_image = ImageFile::new(zero_byte_path, "hash2".to_string(), &zero_byte_refs);
    assert!(matches!(
        zero_byte_image.image_state,
        ImageState::Incompatible {
            reason: IncompatibilityReason::ZeroByte
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
    let empty_refs = ImageReferences::default();
    let unreferenced = ImageFile::new(path.clone(), "hash1".to_string(), &empty_refs);
    assert_eq!(unreferenced.image_state, ImageState::Unreferenced);

    // Test with references
    let mut refs_with_content = ImageReferences::default();
    refs_with_content
        .markdown_file_references
        .push("note.md".to_string());
    let referenced = ImageFile::new(path, "hash2".to_string(), &refs_with_content);
    assert_eq!(referenced.image_state, ImageState::DuplicateCandidate);
}

#[test]
fn test_equality_and_cloning() {
    let temp_dir = TempDir::new().unwrap();

    let mut image_refs = ImageReferences::default();
    image_refs
        .markdown_file_references
        .push("test_note.md".to_string());

    let original_path = TestFileBuilder::new()
        .with_content(vec![0xFF, 0xD8, 0xFF, 0xE0])
        .create(&temp_dir, "test.jpg");

    let original = ImageFile::new(original_path.clone(), "testhash".to_string(), &image_refs);

    let cloned = original.clone();
    assert_eq!(original, cloned, "Cloned ImageFile should equal original");

    // Test inequality
    let different_path = TestFileBuilder::new()
        .with_content(vec![0x89, 0x50, 0x4E, 0x47])
        .create(&temp_dir, "different.jpg");

    let different = ImageFile::new(different_path, "differenthash".to_string(), &image_refs);
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

    let mut image_refs = ImageReferences::default();
    image_refs
        .markdown_file_references
        .push("test_note.md".to_string());

    let info = ImageFile::new(path.clone(), "testhash".to_string(), &image_refs);

    let debug_str = format!("{:?}", info);
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

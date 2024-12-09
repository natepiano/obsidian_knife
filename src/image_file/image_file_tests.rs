use super::*;
use std::path::PathBuf;
use tempfile::TempDir;
use crate::test_utils::TestFileBuilder;

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
        ("image1.jpg", "hash1", vec![0xFF, 0xD8, 0xFF, 0xE0], ImageFileType::Jpeg, ImageState::DuplicateCandidate),
        ("image2.png", "hash2", vec![0x89, 0x50, 0x4E, 0x47], ImageFileType::Png, ImageState::DuplicateCandidate),
        ("image3.tiff", "hash3", vec![0x4D, 0x4D, 0x00, 0x2A], ImageFileType::Tiff, ImageState::Tiff),
        ("image4.jpg", "hash4", vec![], ImageFileType::Jpeg, ImageState::ZeroByte),
        ("image5", "hash5", vec![0x00, 0x01, 0x02, 0x03], ImageFileType::Other("unknown".to_string()), ImageState::DuplicateCandidate),
    ];

    for (filename, hash, content, expected_type, expected_state) in test_cases {
        let path = TestFileBuilder::new()
            .with_content(content)
            .create(&temp_dir, filename);

        let image_refs = ImageReferences::default();
        let info = ImageFile::new(path.clone(), hash.to_string(), &image_refs);

        assert_eq!(info.path, path);
        assert_eq!(info.hash, hash);
        assert_eq!(info.size, fs::metadata(&path).unwrap().len());
        assert_eq!(info.file_type, expected_type);
        assert_eq!(info.image_state, expected_state);
        assert!(info.references.is_empty());
    }
}

#[test]
fn test_equality_and_cloning() {
    let temp_dir = TempDir::new().unwrap();

    let original_path = TestFileBuilder::new()
        .with_content(vec![0xFF, 0xD8, 0xFF, 0xE0])
        .create(&temp_dir, "test.jpg");

    let original = ImageFile::new(
        original_path.clone(),
        "testhash".to_string(),
        &ImageReferences::default(),
    );

    let cloned = original.clone();
    assert_eq!(original, cloned, "Cloned ImageFile should equal original");

    // Test inequality
    let different_path = TestFileBuilder::new()
        .with_content(vec![0x89, 0x50, 0x4E, 0x47])
        .create(&temp_dir, "different.jpg");

    let different = ImageFile::new(
        different_path,
        "differenthash".to_string(),
        &ImageReferences::default(),
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
        .with_content(vec![0xFF, 0xD8, 0xFF, 0xE0])
        .create(&temp_dir, "test.jpg");

    let info = ImageFile::new(
        path.clone(),
        "testhash".to_string(),
        &ImageReferences::default(),
    );

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

#[test]
fn test_image_state_transitions() {
    let temp_dir = TempDir::new().unwrap();

    // Test initial states
    let tiff_path = TestFileBuilder::new()
        .with_content(vec![0x4D, 0x4D, 0x00, 0x2A])
        .create(&temp_dir, "test.tiff");
    let tiff_image = ImageFile::new(
        tiff_path,
        "hash1".to_string(),
        &ImageReferences::default(),
    );
    assert_eq!(tiff_image.image_state, ImageState::Tiff);

    let zero_byte_path = TestFileBuilder::new()
        .with_content(vec![])
        .create(&temp_dir, "test.jpg");
    let zero_byte_image = ImageFile::new(
        zero_byte_path,
        "hash2".to_string(),
        &ImageReferences::default(),
    );
    assert_eq!(zero_byte_image.image_state, ImageState::ZeroByte);

    let normal_path = TestFileBuilder::new()
        .with_content(vec![0x89, 0x50, 0x4E, 0x47])
        .create(&temp_dir, "test.png");
    let normal_image = ImageFile::new(
        normal_path.clone(),
        "hash3".to_string(),
        &ImageReferences::default(),
    );
    assert_eq!(normal_image.image_state, ImageState::DuplicateCandidate);

    // Test transition to Unreferenced
    let mut info = ImageFile::new(
        normal_path,
        "hash".to_string(),
        &ImageReferences::default(),
    );
    info.mark_as_unreferenced();
    assert_eq!(info.image_state, ImageState::Unreferenced);
}

use super::*;
use std::path::PathBuf;

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
fn test_create_image_file_info() {
    let test_cases = vec![
        (
            "image1.jpg",
            "hash1",
            100,
            ImageFileType::Jpeg,
            ImageState::DuplicateCandidate,
        ),
        (
            "image2.png",
            "hash2",
            200,
            ImageFileType::Png,
            ImageState::DuplicateCandidate,
        ),
        (
            "image3.tiff",
            "hash3",
            300,
            ImageFileType::Tiff,
            ImageState::Tiff,
        ),
        (
            "image4.jpg",
            "hash4",
            0,
            ImageFileType::Jpeg,
            ImageState::ZeroByte,
        ),
        (
            "image5",
            "hash5",
            400,
            ImageFileType::Other("unknown".to_string()),
            ImageState::DuplicateCandidate,
        ),
    ];

    for (filename, hash, size, expected_type, expected_state) in test_cases {
        let path = PathBuf::from(filename);
        let info = ImageFile::new(path.clone(), hash.to_string(), size);

        assert_eq!(info.path, path);
        assert_eq!(info.hash, hash);
        assert_eq!(info.size, size);
        assert_eq!(info.file_type, expected_type);
        assert_eq!(info.image_state, expected_state);
        assert!(info.references.is_empty());
    }
}

#[test]
fn test_equality_and_cloning() {
    let original = ImageFile::new(PathBuf::from("test.jpg"), "testhash".to_string(), 100);

    let cloned = original.clone();
    assert_eq!(original, cloned, "Cloned ImageFile should equal original");

    // Test inequality
    let different = ImageFile::new(
        PathBuf::from("different.jpg"),
        "differenthash".to_string(),
        200,
    );
    assert_ne!(
        original, different,
        "Different ImageFile instances should not be equal"
    );
}

#[test]
fn test_image_file_info_debug() {
    let info = ImageFile::new(PathBuf::from("test.jpg"), "testhash".to_string(), 100);

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
        debug_str.contains("100"),
        "Debug output should contain size"
    );
}

#[test]
fn test_image_state_transitions() {
    // Test initial states
    let tiff_image = ImageFile::new(PathBuf::from("test.tiff"), "hash1".to_string(), 100);
    assert_eq!(tiff_image.image_state, ImageState::Tiff);

    let zero_byte_image = ImageFile::new(PathBuf::from("test.jpg"), "hash2".to_string(), 0);
    assert_eq!(zero_byte_image.image_state, ImageState::ZeroByte);

    let normal_image = ImageFile::new(PathBuf::from("test.png"), "hash3".to_string(), 100);
    assert_eq!(normal_image.image_state, ImageState::DuplicateCandidate);

    // Test transition to Unreferenced
    let mut info = ImageFile::new(PathBuf::from("test.png"), "hash".to_string(), 100);
    info.mark_as_unreferenced();
    assert_eq!(info.image_state, ImageState::Unreferenced);
}

use super::*;
use crate::cleanup_images::{handle_file_operation, FileOperation};
use crate::test_utils::TestFileBuilder;

use chrono::Utc;
use std::fs::File;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_remove_reference() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content("# Test\n![Image](test.jpg)\nSome text\n![[test.jpg]]\nMore text".to_string())
        .create(&temp_dir, "test_file.md");

    let image_path = temp_dir.path().join("test.jpg");

    handle_file_operation(
        &file_path,
        FileOperation::RemoveReference(image_path.clone()),
    )
    .unwrap();

    let result = fs::read_to_string(&file_path).unwrap();

    let today = Utc::now().format("[[%Y-%m-%d]]").to_string();
    let expected_content = format!(
        "---\ndate_modified: \"{}\"\n---\n# Test\nSome text\nMore text",
        today
    );

    assert_eq!(result, expected_content);
}

#[test]
fn test_single_invocation() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content("# Test\n![Image](test.jpg)\nSome text".to_string())
        .create(&temp_dir, "test_file.md");
    let image_path = temp_dir.path().join("test.jpg");

    // first invocation
    handle_file_operation(
        &file_path,
        FileOperation::RemoveReference(image_path.clone()),
    )
    .unwrap();

    // second invocation
    handle_file_operation(
        &file_path,
        FileOperation::RemoveReference(image_path.clone()),
    )
    .unwrap();

    let result = fs::read_to_string(&file_path).unwrap();

    let today = Utc::now().format("[[%Y-%m-%d]]").to_string();
    let expected_content = format!("---\ndate_modified: \"{}\"\n---\n# Test\nSome text", today);

    assert_eq!(result, expected_content);
}

#[test]
fn test_delete() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("file_to_delete.jpg");
    File::create(&file_path).unwrap();

    assert!(file_path.exists(), "Test file should exist before deletion");

    handle_file_operation(&file_path, FileOperation::Delete).unwrap();

    assert!(
        !file_path.exists(),
        "Test file should not exist after deletion"
    );
}

#[test]
fn test_handle_file_operation_wikilink_error() {
    let wikilink_path = PathBuf::from("[[Some File]]");

    // Test with Delete operation
    let result = handle_file_operation(&wikilink_path, FileOperation::Delete);
    assert!(
        result.is_err(),
        "Delete operation should fail with wikilink path"
    );

    // Test with RemoveReference operation
    let result = handle_file_operation(
        &wikilink_path,
        FileOperation::RemoveReference(PathBuf::from("old.jpg")),
    );
    assert!(
        result.is_err(),
        "RemoveReference operation should fail with wikilink path"
    );

    // Test with UpdateReference operation
    let result = handle_file_operation(
        &wikilink_path,
        FileOperation::UpdateReference(PathBuf::from("old.jpg"), PathBuf::from("new.jpg")),
    );
    assert!(
        result.is_err(),
        "UpdateReference operation should fail with wikilink path"
    );
}

#[test]
fn test_remove_reference_with_path() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(
            "# Test\n![[conf/media/test.jpg]]\nSome text\n![Image](conf/media/test.jpg)\nMore text"
                .to_string(),
        )
        .create(&temp_dir, "test_file.md");

    let image_path = temp_dir.path().join("conf").join("media").join("test.jpg");

    handle_file_operation(
        &file_path,
        FileOperation::RemoveReference(image_path.clone()),
    )
    .unwrap();

    let result = fs::read_to_string(&file_path).unwrap();
    let today = Utc::now().format("[[%Y-%m-%d]]").to_string();
    let expected_content = format!(
        "---\ndate_modified: \"{}\"\n---\n# Test\nSome text\nMore text",
        today
    );

    assert_eq!(result, expected_content);
}

#[test]
fn test_update_reference_with_path() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(
            "# Test\n![[conf/media/old.jpg]]\nSome text\n![Image](conf/media/old.jpg)\nMore text"
                .to_string(),
        )
        .create(&temp_dir, "test_file.md");

    let old_path = temp_dir.path().join("conf").join("media").join("old.jpg");
    let new_path = temp_dir.path().join("conf").join("media").join("new.jpg");

    handle_file_operation(
        &file_path,
        FileOperation::UpdateReference(old_path.clone(), new_path.clone()),
    )
    .unwrap();

    let result = fs::read_to_string(&file_path).unwrap();
    let today = Utc::now().format("[[%Y-%m-%d]]").to_string();
    let expected_content = format!(
        "---\ndate_modified: \"{}\"\n---\n# Test\n![[conf/media/new.jpg]]\nSome text\n![Image](conf/media/new.jpg)\nMore text",
        today
    );

    assert_eq!(result, expected_content);
}

#[test]
fn test_update_reference_path_variants() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(
            r#"# Test
Normal link: ![Alt](test.jpg)
Wiki link: ![[test.jpg]]
Path link: ![Alt](path/to/test.jpg)
Path wiki: ![[path/to/test.jpg]]
More text"#
                .to_string(),
        )
        .create(&temp_dir, "test_file.md");
    let image_path = temp_dir.path().join("test.jpg"); // Note: just using test.jpg

    handle_file_operation(
        &file_path,
        FileOperation::RemoveReference(image_path.clone()),
    )
    .unwrap();

    let result = fs::read_to_string(&file_path).unwrap();
    let today = Utc::now().format("[[%Y-%m-%d]]").to_string();
    let expected_content = format!("---\ndate_modified: \"{}\"\n---\n# Test\nMore text", today);

    assert_eq!(result, expected_content);
}

#[test]
fn test_mixed_reference_styles() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(
            r#"# Test
![Simple](test.jpg)
![[test.jpg]]
![Full Path](conf/media/test.jpg)
![[conf/media/test.jpg]]
More text"#
                .to_string(),
        )
        .create(&temp_dir, "test_file.md");
    let image_path = temp_dir.path().join("conf").join("media").join("test.jpg");

    handle_file_operation(
        &file_path,
        FileOperation::RemoveReference(image_path.clone()),
    )
    .unwrap();

    let result = fs::read_to_string(&file_path).unwrap();
    let today = Utc::now().format("[[%Y-%m-%d]]").to_string();
    let expected_content = format!("---\ndate_modified: \"{}\"\n---\n# Test\nMore text", today);

    assert_eq!(result, expected_content);
}

#[test]
fn test_reference_with_spaces() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(
            r#"# Test
![Alt text](my test.jpg)
![[my test.jpg]]
More text"#
                .to_string(),
        )
        .create(&temp_dir, "test_file.md");
    let image_path = temp_dir.path().join("my test.jpg");

    handle_file_operation(
        &file_path,
        FileOperation::RemoveReference(image_path.clone()),
    )
    .unwrap();

    let result = fs::read_to_string(&file_path).unwrap();
    let today = Utc::now().format("[[%Y-%m-%d]]").to_string();
    let expected_content = format!("---\ndate_modified: \"{}\"\n---\n# Test\nMore text", today);

    assert_eq!(result, expected_content);
}

#[test]
fn test_cleanup_with_labels() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(
            r#"# Test
Label 1: ![Alt](test.jpg) text
Label 2: ![[test.jpg]] more text
Just label: ![[test.jpg]]
Mixed: ![Alt](test.jpg) ![[test.jpg]]
More text"#
                .to_string(),
        )
        .create(&temp_dir, "test_file.md");
    let image_path = temp_dir.path().join("test.jpg");

    handle_file_operation(
        &file_path,
        FileOperation::RemoveReference(image_path.clone()),
    )
    .unwrap();

    let result = fs::read_to_string(&file_path).unwrap();
    let today = Utc::now().format("[[%Y-%m-%d]]").to_string();
    let expected_content = format!(
        "---\ndate_modified: \"{}\"\n---\n# Test\nLabel 1: text\nLabel 2: more text\nMore text",
        today
    );

    assert_eq!(result, expected_content);
}

#[test]
fn test_reference_with_inline_text() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(
            r#"# Test
Before ![Alt](test.jpg) after
Text before ![[test.jpg]] and after
More text"#
                .to_string(),
        )
        .create(&temp_dir, "test_file.md");
    let image_path = temp_dir.path().join("test.jpg");

    handle_file_operation(
        &file_path,
        FileOperation::RemoveReference(image_path.clone()),
    )
    .unwrap();

    let result = fs::read_to_string(&file_path).unwrap();
    let today = Utc::now().format("[[%Y-%m-%d]]").to_string();
    let expected_content = format!(
        "---\ndate_modified: \"{}\"\n---\n# Test\nBefore after\nText before and after\nMore text",
        today
    );

    assert_eq!(result, expected_content);
}

#[test]
fn test_frontmatter_preservation() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(
            r#"# Test
![Image](test.jpg)
Some text"#
                .to_string(),
        )
        .with_title("Test Document".to_string())
        .with_tags(vec!["test".to_string(), "image".to_string()])
        .create(&temp_dir, "test_file.md");
    let image_path = temp_dir.path().join("test.jpg");

    handle_file_operation(
        &file_path,
        FileOperation::RemoveReference(image_path.clone()),
    )
    .unwrap();

    let result = fs::read_to_string(&file_path).unwrap();
    assert!(result.contains("title: Test Document"));
    assert!(result.contains("tags:"));
    assert!(result.contains("- test"));
    assert!(result.contains("- image"));
}

#[test]
fn test_multiple_references_same_image() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(
            r#"# Test
First reference: ![Alt](test.jpg)
Second reference: ![[test.jpg]]
Third reference in path: ![Alt](conf/media/test.jpg)
Fourth reference: ![[conf/media/test.jpg]]
Some content here."#
                .to_string(),
        )
        .create(&temp_dir, "test_file.md");

    let image_path = temp_dir.path().join("test.jpg");

    handle_file_operation(
        &file_path,
        FileOperation::RemoveReference(image_path.clone()),
    )
    .unwrap();

    let result = fs::read_to_string(&file_path).unwrap();
    let today = Utc::now().format("[[%Y-%m-%d]]").to_string();
    let expected_content = format!(
        "---\ndate_modified: \"{}\"\n---\n# Test\nSome content here.",
        today
    );

    assert_eq!(result, expected_content);
    assert!(!result.contains("test.jpg"));
    assert!(!result.contains("reference:")); // Verify labels are removed
}

#[test]
fn test_update_reference_with_special_characters() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(
            r#"# Test
![Alt](test-with-dashes.jpg)
![[test with spaces.jpg]]
![Alt](test_with_underscores.jpg)
![[test.with.dots.jpg]]"#
                .to_string(),
        )
        .create(&temp_dir, "test_file.md");

    let old_files = vec![
        "test-with-dashes.jpg",
        "test with spaces.jpg",
        "test_with_underscores.jpg",
        "test.with.dots.jpg",
    ];

    for old_file in old_files {
        let old_path = temp_dir.path().join(old_file);
        handle_file_operation(&file_path, FileOperation::RemoveReference(old_path)).unwrap();
    }

    let result = fs::read_to_string(&file_path).unwrap();
    assert!(!result.contains("test-with-dashes.jpg"));
    assert!(!result.contains("test with spaces.jpg"));
    assert!(!result.contains("test_with_underscores.jpg"));
    assert!(!result.contains("test.with.dots.jpg"));
}

#[test]
fn test_nested_directories() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(
            r#"# Test
![Alt](deeply/nested/path/test.jpg)
![[another/path/test.jpg]]
![Alt](../relative/path/test.jpg)
![[./current/path/test.jpg]]"#
                .to_string(),
        )
        .create(&temp_dir, "test_file.md");

    // Create nested directory structure
    let paths = ["deeply/nested/path", "another/path", "current/path"];

    for path in paths.iter() {
        fs::create_dir_all(temp_dir.path().join(path)).unwrap();
    }

    let test_path = temp_dir.path().join("deeply/nested/path/test.jpg");

    handle_file_operation(&file_path, FileOperation::RemoveReference(test_path)).unwrap();

    let result = fs::read_to_string(&file_path).unwrap();
    assert!(!result.contains("deeply/nested/path/test.jpg"));
}

#[test]
fn test_image_reference_with_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(
            r#"# Test
Standard link: ![Alt|size=200](test.jpg)
Wiki with size: ![[test.jpg|200]]
Wiki with caption: ![[test.jpg|This is a caption]]
Multiple params: ![[test.jpg|200|caption text]]
Some text"#
                .to_string(),
        )
        .create(&temp_dir, "test_file.md");

    let image_path = temp_dir.path().join("test.jpg");

    handle_file_operation(
        &file_path,
        FileOperation::RemoveReference(image_path.clone()),
    )
    .unwrap();

    let result = fs::read_to_string(&file_path).unwrap();
    let today = Utc::now().format("[[%Y-%m-%d]]").to_string();
    // Our cleanup function is designed to remove empty lines and simplify to just the text
    let expected_content = format!("---\ndate_modified: \"{}\"\n---\n# Test\nSome text", today);
    assert_eq!(result, expected_content);
    assert!(!result.contains("test.jpg"));
}

#[test]
fn test_group_images() {
    let temp_dir = TempDir::new().unwrap();
    let mut image_map = HashMap::new();

    // Create test files
    let tiff_path = temp_dir.path().join("test.tiff");
    let zero_byte_path = temp_dir.path().join("empty.jpg");
    let unreferenced_path = temp_dir.path().join("unreferenced.jpg");
    let duplicate_path1 = temp_dir.path().join("duplicate1.jpg");
    let duplicate_path2 = temp_dir.path().join("duplicate2.jpg");

    // Create empty file
    File::create(&zero_byte_path).unwrap();

    // Add test entries to image_map
    image_map.insert(
        tiff_path.clone(),
        ImageInfo {
            hash: "hash1".to_string(),
            references: vec!["ref1".to_string()],
        },
    );

    image_map.insert(
        zero_byte_path.clone(),
        ImageInfo {
            hash: "hash2".to_string(),
            references: vec!["ref2".to_string()],
        },
    );

    image_map.insert(
        unreferenced_path.clone(),
        ImageInfo {
            hash: "hash3".to_string(),
            references: vec![],
        },
    );

    let duplicate_hash = "hash4".to_string();
    image_map.insert(
        duplicate_path1.clone(),
        ImageInfo {
            hash: duplicate_hash.clone(),
            references: vec!["ref3".to_string()],
        },
    );
    image_map.insert(
        duplicate_path2.clone(),
        ImageInfo {
            hash: duplicate_hash.clone(),
            references: vec!["ref4".to_string()],
        },
    );

    // Group the images
    let grouped = group_images(&image_map);

    // Verify TIFF images
    assert!(grouped.get(&ImageGroupType::TiffImage).is_some());

    // Verify zero-byte images
    let zero_byte_group = grouped.get(&ImageGroupType::ZeroByteImage).unwrap();
    assert_eq!(zero_byte_group.len(), 1);
    assert_eq!(zero_byte_group[0].path, zero_byte_path);

    // Verify unreferenced images
    let unreferenced_group = grouped.get(&ImageGroupType::UnreferencedImage).unwrap();
    assert_eq!(unreferenced_group.len(), 1);
    assert_eq!(unreferenced_group[0].path, unreferenced_path);

    // Verify duplicate groups
    let duplicate_groups = grouped.get_duplicate_groups();
    assert_eq!(duplicate_groups.len(), 1);
    let (hash, group) = duplicate_groups[0];
    assert_eq!(hash, &duplicate_hash);
    assert_eq!(group.len(), 2);
    assert!(group.iter().any(|g| g.path == duplicate_path1));
    assert!(group.iter().any(|g| g.path == duplicate_path2));
}

#[test]
fn test_determine_group_type_case_insensitive() {
    let temp_dir = TempDir::new().unwrap();

    // Test different case variations of TIFF extension
    let extensions = ["tiff", "TIFF", "Tiff", "TiFf"];

    for ext in extensions {
        let path = temp_dir.path().join(format!("test.{}", ext));
        File::create(&path).unwrap();

        let info = ImageInfo {
            hash: "hash1".to_string(),
            references: vec!["ref1".to_string()],
        };

        let group_type = determine_group_type(&path, &info);
        assert!(
            matches!(group_type, ImageGroupType::TiffImage),
            "Failed to match TIFF extension: {}",
            ext
        );
    }
}

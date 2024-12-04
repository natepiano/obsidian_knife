use crate::obsidian_repository_info::obsidian_repository_info_types::{
    ImageOperation, MarkdownOperation,
};
use crate::scan::scan_folders;
use crate::test_utils::{eastern_midnight, TestFileBuilder};
use crate::validated_config::get_test_validated_config_builder;
use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_analyze_missing_references() {
    let temp_dir = TempDir::new().unwrap();
    let mut builder = get_test_validated_config_builder(&temp_dir);
    let config = builder.apply_changes(true).build().unwrap();
    fs::create_dir_all(config.output_folder()).unwrap();

    // Create a markdown file that references a non-existent image
    let test_date = eastern_midnight(2024, 1, 15);
    let md_file = TestFileBuilder::new()
        .with_content(
            "# Test\n![[missing.jpg]]\nSome content\n![Another](also_missing.jpg)".to_string(),
        )
        .with_matching_dates(test_date)
        .with_fs_dates(test_date, test_date)
        .create(&temp_dir, "test.md");

    let mut repo_info = scan_folders(&config).unwrap();
    if let Some(markdown_file) = repo_info.markdown_files.get_mut(&md_file) {
        markdown_file.mark_image_reference_as_updated();
    }

    // Run analyze
    let (_, _, image_operations) = repo_info.analyze_images(&config).unwrap();
    repo_info.process_image_reference_updates(&image_operations);
    repo_info.persist(&config, image_operations).unwrap();

    // Verify the markdown file was updated
    let updated_content = fs::read_to_string(&md_file).unwrap();

    let today_formatted = Utc::now().format("[[%Y-%m-%d]]").to_string();

    let expected_content = format!(
        "---\ndate_created: '[[2024-01-15]]'\ndate_modified: '{}'\n---\n# Test\nSome content",
        today_formatted
    );
    assert_eq!(updated_content, expected_content);

    // Second analyze pass to verify idempotency
    let mut repo_info = scan_folders(&config).unwrap();

    let (_, _, image_operations) = repo_info.analyze_images(&config).unwrap();
    repo_info.process_image_reference_updates(&image_operations);
    repo_info.persist(&config, image_operations).unwrap();

    // Verify content remains the same after second pass
    let final_content = fs::read_to_string(&md_file).unwrap();
    assert_eq!(
        final_content, expected_content,
        "Content should not change on second analyze/persist pass"
    );
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_analyze_duplicates() {
    let temp_dir = TempDir::new().unwrap();
    let mut builder = get_test_validated_config_builder(&temp_dir);
    let config = builder.apply_changes(true).build().unwrap();

    fs::create_dir_all(config.output_folder()).unwrap();

    // Create duplicate images with same content
    let img_content = vec![0xFF, 0xD8, 0xFF, 0xE0]; // Simple JPEG header
    let img_path1 = TestFileBuilder::new()
        .with_content(img_content.clone())
        .create(&temp_dir, "image1.jpg");
    let img_path2 = TestFileBuilder::new()
        .with_content(img_content)
        .create(&temp_dir, "image2.jpg");

    // Create markdown files referencing both images
    let test_date = eastern_midnight(2024, 1, 15);
    let md_file1 = TestFileBuilder::new()
        .with_content("# Doc1\n![[image1.jpg]]".to_string())
        .with_matching_dates(test_date)
        .with_fs_dates(test_date, test_date)
        .create(&temp_dir, "doc1.md");
    let md_file2 = TestFileBuilder::new()
        .with_content("# Doc2\n![[image2.jpg]]".to_string())
        .with_matching_dates(test_date)
        .with_fs_dates(test_date, test_date)
        .create(&temp_dir, "doc2.md");

    let mut repo_info = scan_folders(&config).unwrap();

    if let Some(markdown_file) = repo_info.markdown_files.get_mut(&md_file1) {
        markdown_file.mark_image_reference_as_updated();
    }
    if let Some(markdown_file) = repo_info.markdown_files.get_mut(&md_file2) {
        markdown_file.mark_image_reference_as_updated();
    }

    // Run analyze images
    let (_, _, image_operations) = repo_info.analyze_images(&config).unwrap();

    repo_info.process_image_reference_updates(&image_operations);
    repo_info.persist(&config, image_operations).unwrap();

    // Verify one image was kept and one was deleted
    assert_ne!(
        img_path1.exists(),
        img_path2.exists(),
        "One image should be deleted"
    );

    // Verify markdown files were updated to reference the same image
    let keeper_name = if img_path1.exists() {
        "image1.jpg"
    } else {
        "image2.jpg"
    };
    let updated_content1 = fs::read_to_string(&md_file1).unwrap();
    let updated_content2 = fs::read_to_string(&md_file2).unwrap();

    assert!(updated_content1.contains(keeper_name));
    assert!(updated_content2.contains(keeper_name));
}

struct ImageTestCase {
    name: &'static str,
    setup: fn(&TempDir) -> Vec<PathBuf>, // Returns paths created
    expected_ops: fn(&[PathBuf]) -> (Vec<ImageOperation>, Vec<MarkdownOperation>),
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_image_operation_generation() {
    let test_cases = vec![
        ImageTestCase {
            name: "missing_references",
            setup: |temp_dir| {
                let test_date = eastern_midnight(2024, 1, 15);
                let md_file = TestFileBuilder::new()
                    .with_content("# Test\n![[missing.jpg]]\nSome content".to_string())
                    .with_matching_dates(test_date)
                    .with_fs_dates(test_date, test_date)
                    .create(temp_dir, "test1.md");
                vec![md_file]
            },
            expected_ops: |paths| {
                (
                    vec![],
                    vec![MarkdownOperation::RemoveReference {
                        markdown_path: paths[0].clone(),
                        image_path: PathBuf::from("missing.jpg"),
                    }],
                )
            },
        },
        ImageTestCase {
            name: "duplicate_images",
            setup: |temp_dir| {
                let test_date = eastern_midnight(2024, 1, 15);
                // Create duplicate images
                let img_content = vec![0xFF, 0xD8, 0xFF, 0xE0];
                let img_path1 = TestFileBuilder::new()
                    .with_content(img_content.clone())
                    .create(temp_dir, "image1.jpg");
                let img_path2 = TestFileBuilder::new()
                    .with_content(img_content)
                    .create(temp_dir, "image2.jpg");

                // Create markdown files referencing the images
                let md_file1 = TestFileBuilder::new()
                    .with_content("# Doc1\n![[image1.jpg]]".to_string())
                    .with_matching_dates(test_date)
                    .with_fs_dates(test_date, test_date)
                    .create(temp_dir, "test1.md");
                let md_file2 = TestFileBuilder::new()
                    .with_content("# Doc2\n![[image2.jpg]]".to_string())
                    .with_matching_dates(test_date)
                    .with_fs_dates(test_date, test_date)
                    .create(temp_dir, "test2.md");

                vec![img_path1, img_path2, md_file1, md_file2]
            },
            expected_ops: |paths| {
                (
                    vec![ImageOperation::Delete(paths[1].clone())], // Delete image2.jpg
                    vec![MarkdownOperation::UpdateReference {
                        markdown_path: paths[3].clone(),  // test2.md
                        old_image_path: paths[1].clone(), // image2.jpg
                        new_image_path: paths[0].clone(), // image1.jpg
                    }],
                )
            },
        },
        ImageTestCase {
            name: "zero_byte_images",
            setup: |temp_dir| {
                let test_date = eastern_midnight(2024, 1, 15);
                // Create empty image
                let img_path = TestFileBuilder::new()
                    .with_content(vec![])
                    .create(temp_dir, "empty.jpg");

                // Create markdown file referencing the image
                let md_file = TestFileBuilder::new()
                    .with_content("# Doc\n![[empty.jpg]]".to_string())
                    .with_matching_dates(test_date)
                    .with_fs_dates(test_date, test_date)
                    .create(temp_dir, "test.md");

                vec![img_path, md_file]
            },
            expected_ops: expect_delete_remove_reference(),
        },
        ImageTestCase {
            name: "tiff_images",
            setup: |temp_dir| {
                let test_date = eastern_midnight(2024, 1, 15);
                // Create TIFF image with minimal valid header
                let img_path = TestFileBuilder::new()
                    .with_content(vec![0x4D, 0x4D, 0x00, 0x2A]) // TIFF header
                    .create(temp_dir, "image.tiff");

                // Create markdown file referencing the image
                let md_file = TestFileBuilder::new()
                    .with_content("# Doc\n![[image.tiff]]".to_string())
                    .with_matching_dates(test_date)
                    .with_fs_dates(test_date, test_date)
                    .create(temp_dir, "test.md");

                vec![img_path, md_file]
            },
            expected_ops: expect_delete_remove_reference(),
        },
        ImageTestCase {
            name: "unreferenced_images",
            setup: |temp_dir| {
                // Create image with no references
                let img_path = TestFileBuilder::new()
                    .with_content(vec![0xFF, 0xD8, 0xFF, 0xE0])
                    .create(temp_dir, "unused.jpg");

                vec![img_path]
            },
            expected_ops: |paths| {
                (
                    vec![ImageOperation::Delete(paths[0].clone())],
                    vec![], // No markdown changes needed
                )
            },
        },
    ];

    for test_case in test_cases {
        let temp_dir = TempDir::new().unwrap();
        let mut builder = get_test_validated_config_builder(&temp_dir);
        let config = builder.apply_changes(true).build().unwrap();
        fs::create_dir_all(config.output_folder()).unwrap();

        let created_paths = (test_case.setup)(&temp_dir);
        let repo_info = scan_folders(&config).unwrap();

        let (_, _, operations) = repo_info.analyze_images(&config).unwrap();

        let (expected_image_ops, expected_markdown_ops) = (test_case.expected_ops)(&created_paths);

        for (actual, expected) in operations.image_ops.iter().zip(expected_image_ops.iter()) {
            match (actual, expected) {
                (ImageOperation::Delete(actual_path), ImageOperation::Delete(expected_path)) => {
                    assert_eq!(
                        actual_path.as_path(),
                        expected_path.as_path(),
                        "Test case '{}': Delete operation paths don't match",
                        test_case.name
                    );
                }
            }
        }

        for (i, (actual, expected)) in operations
            .markdown_ops
            .iter()
            .zip(expected_markdown_ops.iter())
            .enumerate()
        {
            assert!(
                matches!(actual, MarkdownOperation::RemoveReference { .. })
                    == matches!(expected, MarkdownOperation::RemoveReference { .. })
                    && matches!(actual, MarkdownOperation::UpdateReference { .. })
                        == matches!(expected, MarkdownOperation::UpdateReference { .. }),
                "Test case '{}': Operation type mismatch at position {}",
                test_case.name,
                i
            );

            match (actual, expected) {
                (
                    MarkdownOperation::RemoveReference { markdown_path: actual_md, image_path: actual_img },
                    MarkdownOperation::RemoveReference { markdown_path: expected_md, image_path: expected_img }
                ) => {
                    assert_eq!(actual_md.as_path(), expected_md.as_path(),
                               "Test case '{}': RemoveReference markdown paths don't match", test_case.name);
                    assert_eq!(actual_img.as_path(), expected_img.as_path(),
                               "Test case '{}': RemoveReference image paths don't match", test_case.name);
                },
                (
                    MarkdownOperation::UpdateReference { markdown_path: actual_md, old_image_path: actual_old, new_image_path: actual_new },
                    MarkdownOperation::UpdateReference { markdown_path: expected_md, old_image_path: expected_old, new_image_path: expected_new }
                ) => {
                    assert_eq!(actual_md.as_path(), expected_md.as_path(),
                               "Test case '{}': UpdateReference markdown paths don't match", test_case.name);
                    assert_eq!(actual_old.as_path(), expected_old.as_path(),
                               "Test case '{}': UpdateReference old image paths don't match", test_case.name);
                    assert_eq!(actual_new.as_path(), expected_new.as_path(),
                               "Test case '{}': UpdateReference new image paths don't match", test_case.name);
                },
                _ => panic!("Test case '{}': Mismatched operation types - this should have been caught by the type check", test_case.name)
            }
        }
    }
}

fn expect_delete_remove_reference(
) -> fn(&[PathBuf]) -> (Vec<ImageOperation>, Vec<MarkdownOperation>) {
    |paths| {
        (
            vec![ImageOperation::Delete(paths[0].clone())],
            vec![MarkdownOperation::RemoveReference {
                markdown_path: paths[1].clone(),
                image_path: paths[0].clone(),
            }],
        )
    }
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_image_reference_detection() {
    let temp_dir = TempDir::new().unwrap();
    let mut builder = get_test_validated_config_builder(&temp_dir);
    let config = builder.apply_changes(true).build().unwrap();
    fs::create_dir_all(config.output_folder()).unwrap();

    // Test date for consistent file timestamps
    let test_date = eastern_midnight(2024, 1, 15);

    // Create nested directory structure
    let nested_paths = [
        "deeply/nested/path",
        "another/path",
        "current/path",
        "conf/media",
    ];

    for path in nested_paths.iter() {
        fs::create_dir_all(temp_dir.path().join(path)).unwrap();
    }

    // Create test images with some content
    let img_content = vec![0xFF, 0xD8, 0xFF, 0xE0]; // Simple JPEG header
    let nested_img_path = temp_dir
        .path()
        .join("deeply")
        .join("nested")
        .join("path")
        .join("image2.JPG");
    let another_img_path = temp_dir
        .path()
        .join("another")
        .join("path")
        .join("image3.jpg");

    let img_path1 = TestFileBuilder::new()
        .with_content(img_content.clone())
        .create(&temp_dir, "Image1.jpg"); // Mixed case filename
    let img_path2 = TestFileBuilder::new()
        .with_content(img_content.clone())
        .create(&temp_dir, &nested_img_path.to_string_lossy()); // Different case extension in nested dir
    let img_path3 = TestFileBuilder::new()
        .with_content(img_content.clone())
        .create(&temp_dir, &another_img_path.to_string_lossy()); // Unreferenced in another path

    // Create markdown files with various reference formats including nested paths
    let md_content1 = r#"# Doc1
![[Image1.jpg|300]]
![[deeply/nested/path/image2.JPG]]
[[image2.JPG]]"#;
    let md_file1 = TestFileBuilder::new()
        .with_content(md_content1.to_string())
        .with_matching_dates(test_date)
        .with_fs_dates(test_date, test_date)
        .create(&temp_dir, "doc1.md");

    let md_content2 = r#"# Doc2
![](Image1.jpg)
![Alt](deeply/nested/path/image2.JPG)
![[./current/path/other.jpg]]
![[../relative/path/another.jpg]]"#;
    let md_file2 = TestFileBuilder::new()
        .with_content(md_content2.to_string())
        .with_matching_dates(test_date)
        .with_fs_dates(test_date, test_date)
        .create(&temp_dir, "doc2.md");

    // Scan the repository
    let mut repo_info = scan_folders(&config).unwrap();

    if let Some(markdown_file) = repo_info.markdown_files.get_mut(&md_file1) {
        markdown_file.mark_image_reference_as_updated();
    }
    if let Some(markdown_file) = repo_info.markdown_files.get_mut(&md_file2) {
        markdown_file.mark_image_reference_as_updated();
    }

    // Run analyze to generate the image info map
    let (_, _, operations) = repo_info.analyze_images(&config).unwrap();

    // Verify image reference detection
    let deletion_operations: Vec<_> = operations
        .image_ops
        .iter()
        .filter(|op| matches!(op, ImageOperation::Delete(_)))
        .collect();

    assert_eq!(
        deletion_operations.len(),
        2,
        "Expected two images to be deleted - one duplicate and one unreferenced"
    );

    match &deletion_operations[0] {
        ImageOperation::Delete(path) => {
            assert_eq!(
                path.file_name().unwrap(),
                img_path3.file_name().unwrap(),
                "Wrong image marked as unreferenced"
            );
        }
    }

    // Verify the image references map
    let image_refs = &repo_info.image_path_to_references_map;

    // Check Image1.jpg references (root directory)
    let image1_refs = image_refs.get(&img_path1).unwrap();
    assert_eq!(
        image1_refs.markdown_file_references.len(),
        2,
        "Image1.jpg should be referenced by both markdown files"
    );

    // Check nested image2.JPG references
    let image2_refs = image_refs.get(&img_path2).unwrap();
    assert_eq!(
        image2_refs.markdown_file_references.len(),
        2,
        "image2.JPG should be referenced by both markdown files despite being in nested directory"
    );

    // Check unreferenced image3.jpg
    let image3_refs = image_refs.get(&img_path3).unwrap();
    assert!(
        image3_refs.markdown_file_references.is_empty(),
        "image3.jpg in nested directory should have no references"
    );
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_analyze_wikilink_errors() {
    let temp_dir = TempDir::new().unwrap();
    let mut builder = get_test_validated_config_builder(&temp_dir);
    let config = builder.apply_changes(true).build().unwrap();
    fs::create_dir_all(config.output_folder()).unwrap();

    // Create a markdown file with a wikilink as a path (invalid)
    let test_date = eastern_midnight(2024, 1, 15);
    let md_file = TestFileBuilder::new()
        .with_content("# Test\n![[[[Some File]]]]".to_string())
        .with_matching_dates(test_date)
        .with_fs_dates(test_date, test_date)
        .create(&temp_dir, "test_file.md");

    let repo_info = scan_folders(&config).unwrap();

    // Run analyze and verify it handles wikilink paths appropriately
    let (_, _, operations) = repo_info.analyze_images(&config).unwrap();

    // Verify no operations were generated for invalid wikilink paths
    assert!(
        operations.image_ops.is_empty(),
        "No image operations should be created for wikilink paths"
    );
    assert!(
        operations.markdown_ops.is_empty(),
        "No markdown operations should be created for wikilink paths"
    );

    // Verify the content wasn't modified
    let final_content = fs::read_to_string(&md_file).unwrap();
    assert!(
        final_content.contains("![[[[Some File]]]]"),
        "Content with invalid wikilinks should not be modified"
    );
}

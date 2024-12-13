use crate::obsidian_repository::obsidian_repository_types::ImageOperation;
use crate::obsidian_repository::ObsidianRepository;
use crate::test_utils::TestFileBuilder;
use crate::validated_config::validated_config_tests;
use crate::{test_utils, MARKDOWN_EXTENSION};
use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_analyze_missing_references() {
    let temp_dir = TempDir::new().unwrap();
    let mut builder = validated_config_tests::get_test_validated_config_builder(&temp_dir);
    let config = builder.apply_changes(true).build().unwrap();
    fs::create_dir_all(config.output_folder()).unwrap();

    // Create a markdown file that references a non-existent image
    let test_date = test_utils::eastern_midnight(2024, 1, 15);
    let md_file = TestFileBuilder::new()
        .with_content(
            "# Test\n![[missing.jpg]]\nSome content\n![Another](also_missing.jpg)".to_string(),
        )
        .with_matching_dates(test_date)
        .with_fs_dates(test_date, test_date)
        .create(&temp_dir, "test.md");

    let mut repository = ObsidianRepository::new(&config).unwrap();
    if let Some(markdown_file) = repository.markdown_files.get_mut(&md_file) {
        markdown_file.mark_image_reference_as_updated();
    }

    // Run analyze
    let (_, image_operations) = repository.analyze_repository(&config).unwrap();

    repository.persist(image_operations).unwrap();

    // Verify the markdown file was updated
    let updated_content = fs::read_to_string(&md_file).unwrap();

    let today_formatted = Utc::now().format("[[%Y-%m-%d]]").to_string();

    let expected_content = format!(
        "---\ndate_created: '[[2024-01-15]]'\ndate_modified: '{}'\n---\n# Test\n\nSome content",
        today_formatted
    );
    assert_eq!(updated_content, expected_content);

    // Second analyze pass to verify idempotency
    let mut repository = ObsidianRepository::new(&config).unwrap();

    let (_, image_operations) = repository.analyze_images().unwrap();
    repository.process_image_reference_updates(&image_operations);
    repository.persist(image_operations).unwrap();

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
    let mut builder = validated_config_tests::get_test_validated_config_builder(&temp_dir);
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
    let test_date = test_utils::eastern_midnight(2024, 1, 15);
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

    let mut repository = ObsidianRepository::new(&config).unwrap();

    if let Some(markdown_file) = repository.markdown_files.get_mut(&md_file1) {
        markdown_file.mark_image_reference_as_updated();
    }
    if let Some(markdown_file) = repository.markdown_files.get_mut(&md_file2) {
        markdown_file.mark_image_reference_as_updated();
    }

    // Run analyze
    let (_, image_operations) = repository.analyze_repository(&config).unwrap();

    repository.persist(image_operations).unwrap();

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

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_image_replacement_outcomes() {
    struct ImageTestCase {
        name: &'static str,
        setup: TestSetup,
        verify: VerifyOutcome,
    }

    struct TestSetup {
        images: Vec<TestImage>,
        markdown_files: Vec<TestMarkdown>,
    }

    struct TestImage {
        name: String,
        content: Vec<u8>,
    }

    struct TestMarkdown {
        name: String,
        content: String,
    }

    type VerifyOutcome = fn(&[PathBuf], &ObsidianRepository);

    fn create_test_files(temp_dir: &TempDir, setup: &TestSetup) -> Vec<PathBuf> {
        let test_date = test_utils::eastern_midnight(2024, 1, 15);
        let mut paths = Vec::new();

        // Create images
        for image in &setup.images {
            let path = TestFileBuilder::new()
                .with_content(image.content.clone())
                .create(temp_dir, &image.name);
            paths.push(path);
        }

        // Create markdown files
        for md in &setup.markdown_files {
            let path = TestFileBuilder::new()
                .with_content(md.content.clone())
                .with_matching_dates(test_date)
                .create(temp_dir, &md.name);
            paths.push(path);
        }

        paths
    }

    let jpeg_header = vec![0xFF, 0xD8, 0xFF, 0xE0];
    let tiff_header = vec![0x4D, 0x4D, 0x00, 0x2A];
    let empty_content = vec![];

    let test_cases = vec![
        ImageTestCase {
            name: "duplicate_images",
            setup: TestSetup {
                images: vec![
                    TestImage { name: "image1.jpg".into(), content: jpeg_header.clone() },
                    TestImage { name: "image2.jpg".into(), content: jpeg_header.clone() },
                ],
                markdown_files: vec![
                    TestMarkdown { name: "test1.md".into(), content: "# Doc1\n![[image1.jpg]]".into() },
                    TestMarkdown { name: "test2.md".into(), content: "# Doc2\n![[image2.jpg]]".into() },
                ],
            },
            verify: |paths, _| {
                  assert!(paths[0].exists() != paths[1].exists(),
                        "One image should exist and one should be deleted");

                let keeper_name = if paths[0].exists() { "image1.jpg" } else { "image2.jpg" };

                for (i, md_path) in paths[2..].iter().enumerate() {
                    let content = fs::read_to_string(md_path).unwrap();

                    let possible_refs = vec![
                        format!("![[{}]]", keeper_name),
                        format!("![[conf/media/{}]]", keeper_name)
                    ];

                    assert!(
                        possible_refs.iter().any(|ref_str| content.contains(ref_str)),
                        "Markdown file {} should reference keeper image '{}' either directly or in conf/media/\nActual content:\n{}",
                        i + 1, keeper_name, content
                    );
                }
            },
        },
        ImageTestCase {
            name: "zero_byte_images",
            setup: TestSetup {
                images: vec![
                    TestImage { name: "empty.jpg".into(), content: empty_content.clone() },
                ],
                markdown_files: vec![
                    TestMarkdown {
                        name: "test.md".into(),
                        content: "# Doc\n![[empty.jpg]]\nSome content".into()
                    },
                ],
            },
            verify: |paths, _| {
                assert!(!paths[0].exists(), "Zero byte image should be deleted");
                let content = fs::read_to_string(&paths[1]).unwrap();
                assert!(!content.contains("![[empty.jpg]]"));
                assert!(content.contains("# Doc\nSome content"));
            },
        },
        ImageTestCase {
            name: "tiff_images",
            setup: TestSetup {
                images: vec![
                    TestImage { name: "image.tiff".into(), content: tiff_header },
                ],
                markdown_files: vec![
                    TestMarkdown {
                        name: "test.md".into(),
                        content: "# Doc\n![[image.tiff]]\nOther content".into()
                    },
                ],
            },
            verify: |paths, _| {
                assert!(!paths[0].exists(), "TIFF image should be deleted");
                let content = fs::read_to_string(&paths[1]).unwrap();
                assert!(!content.contains("![[image.tiff]]"));
                assert!(content.contains("# Doc\nOther content"));
            },
        },
        ImageTestCase {
            name: "unreferenced_images",
            setup: TestSetup {
                images: vec![
                    TestImage { name: "unused.jpg".into(), content: jpeg_header.clone() },
                ],
                markdown_files: vec![],
            },
            verify: |paths, _| {
                assert!(!paths[0].exists(), "Unreferenced image should be deleted");
            },
        },
    ];

    for test_case in test_cases {
        let temp_dir = TempDir::new().unwrap();
        let mut builder = validated_config_tests::get_test_validated_config_builder(&temp_dir);
        let config = builder.apply_changes(true).build().unwrap();
        fs::create_dir_all(config.output_folder()).unwrap();

        let created_paths = create_test_files(&temp_dir, &test_case.setup);
        let mut repository = ObsidianRepository::new(&config).unwrap();

        // Mark markdown files for persistence
        for path in &created_paths {
            if path.extension().map_or(false, |ext| ext == MARKDOWN_EXTENSION) {
                if let Some(markdown_file) = repository.markdown_files.get_mut(path) {
                    markdown_file.mark_image_reference_as_updated();
                }
            }
        }

        let (_, operations) = repository.analyze_repository(&config).unwrap();
        repository.persist(operations).unwrap();

        (test_case.verify)(&created_paths, &repository);
    }
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_image_reference_detection() {
    let temp_dir = TempDir::new().unwrap();
    let mut builder = validated_config_tests::get_test_validated_config_builder(&temp_dir);
    let config = builder.apply_changes(true).build().unwrap();
    fs::create_dir_all(config.output_folder()).unwrap();

    // Test date for consistent file timestamps
    let test_date = test_utils::eastern_midnight(2024, 1, 15);

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
    let mut repository = ObsidianRepository::new(&config).unwrap();

    if let Some(markdown_file) = repository.markdown_files.get_mut(&md_file1) {
        markdown_file.mark_image_reference_as_updated();
    }
    if let Some(markdown_file) = repository.markdown_files.get_mut(&md_file2) {
        markdown_file.mark_image_reference_as_updated();
    }

    // Run analyze to generate the image info map
    let (_, operations) = repository.analyze_repository(&config).unwrap();

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
    let image_refs = &repository.image_path_to_references_map;

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
    let mut builder = validated_config_tests::get_test_validated_config_builder(&temp_dir);
    let config = builder.apply_changes(true).build().unwrap();
    fs::create_dir_all(config.output_folder()).unwrap();

    // Create a markdown file with a wikilink as a path (invalid)
    let test_date = test_utils::eastern_midnight(2024, 1, 15);
    let md_file = TestFileBuilder::new()
        .with_content("# Test\n![[[[Some File]]]]".to_string())
        .with_matching_dates(test_date)
        .with_fs_dates(test_date, test_date)
        .create(&temp_dir, "test_file.md");

    let repository = ObsidianRepository::new(&config).unwrap();

    // Run analyze and verify it handles wikilink paths appropriately
    let (_, operations) = repository.analyze_images().unwrap();

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

#[test]
fn test_handle_missing_references() {
    let temp_dir = TempDir::new().unwrap();
    let mut builder = validated_config_tests::get_test_validated_config_builder(&temp_dir);
    let config = builder.apply_changes(true).build().unwrap();
    fs::create_dir_all(config.output_folder()).unwrap();

    let test_date = test_utils::eastern_midnight(2024, 1, 15);

    // Create markdown files with references to non-existent images
    let md_content = r#"# Test Document
![[missing_image1.jpg]]
![[missing_image2.jpg]]
"#;
    let md_file = TestFileBuilder::new()
        .with_content(md_content.to_string())
        .with_matching_dates(test_date)
        .create(&temp_dir, "test_doc.md");

    // Initialize the repository info
    let mut repository = ObsidianRepository::new(&config).unwrap();

    // Run the analysis
    let (_, operations) = repository.analyze_repository(&config).unwrap();

    // Verify that the missing references are handled correctly
    let markdown_file = &repository.markdown_files.get_mut(&md_file).unwrap();
    let missing_references = &markdown_file.image_links.missing();
    assert_eq!(
        missing_references.len(),
        2,
        "Expected two missing image references"
    );

    // Verify that no image operations were created for the missing references
    assert!(
        operations.image_ops.is_empty(),
        "No image operations should be created for missing references"
    );

    // Verify that the MarkdownFile.content does not have the references anymore
    assert!(
        !&markdown_file.content.contains("![[missing_image1.jpg]]")
            && !&markdown_file.content.contains("![[missing_image2.jpg]]"),
        "MarkdownFile content should not contain missing references"
    );

    // verify needs persist has been activated
    assert!(
        &markdown_file.frontmatter.as_ref().unwrap().needs_persist(),
        "needs persist should better well be true, boyo"
    )
}

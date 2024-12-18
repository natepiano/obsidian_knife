use crate::image_file::ImageFileState;
use crate::markdown_file::{ImageLinkState, MarkdownFile, PersistReason};
use crate::markdown_files::MarkdownFiles;
use crate::obsidian_repository::ObsidianRepository;
use crate::test_utils::TestFileBuilder;
use crate::utils::VecEnumFilter;
use crate::validated_config::validated_config_tests;
use crate::{test_utils, MARKDOWN_EXTENSION};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

impl MarkdownFiles {
    fn get_mut(&mut self, path: &Path) -> Option<&mut MarkdownFile> {
        self.iter_mut().find(|file| file.path == path)
    }
}

struct ImageTestCase {
    _name: &'static str,
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
        // Instead of using mark_image_reference_as_updated which uses current date,
        // directly set the date we want
        if let Some(fm) = &mut markdown_file.frontmatter {
            fm.set_date_modified(test_date, config.operational_timezone());
        }
        markdown_file
            .persist_reasons
            .push(PersistReason::ImageReferencesModified);
    }

    repository.persist().unwrap();

    // Verify the markdown file was updated
    let updated_content = fs::read_to_string(&md_file).unwrap();
    let expected_content = "---\ndate_created: '[[2024-01-15]]'\ndate_modified: '[[2024-01-15]]'\n---\n# Test\nSome content".to_string();
    assert_eq!(updated_content, expected_content);

    // Second analyze pass to verify idempotency
    let mut repository = ObsidianRepository::new(&config).unwrap();
    repository.mark_image_files_for_deletion();
    repository.persist().unwrap();

    // Verify content remains the same after second pass
    let final_content = fs::read_to_string(&md_file).unwrap();
    assert_eq!(
        final_content, expected_content,
        "Content should not change on second analyze/persist pass"
    );
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_image_replacement_outcomes() {
    let jpeg_header = vec![0xFF, 0xD8, 0xFF, 0xE0];
    let tiff_header = vec![0x4D, 0x4D, 0x00, 0x2A];
    let empty_content = vec![];

    let test_cases = vec![
        ImageTestCase {
            _name: "zero_byte_images",
            setup: TestSetup {
                images: vec![TestImage {
                    name: "empty.jpg".into(),
                    content: empty_content.clone(),
                }],
                markdown_files: vec![TestMarkdown {
                    name: "test.md".into(),
                    content: "# Doc\n![[empty.jpg]]\nSome content".into(),
                }],
            },
            verify: |paths, _| {
                assert!(!paths[0].exists(), "Zero byte image should be deleted");
                let content = fs::read_to_string(&paths[1]).unwrap();
                assert!(!content.contains("![[empty.jpg]]"));
                assert!(content.contains("# Doc\nSome content"));
            },
        },
        ImageTestCase {
            _name: "duplicate_images",
            setup: TestSetup {
                images: vec![
                    TestImage {
                        name: "image1.jpg".into(),
                        content: jpeg_header.clone(),
                    },
                    TestImage {
                        name: "image2.jpg".into(),
                        content: jpeg_header.clone(),
                    },
                ],
                markdown_files: vec![
                    TestMarkdown {
                        name: "test1.md".into(),
                        content: "# Doc1\n![[image1.jpg]]".into(),
                    },
                    TestMarkdown {
                        name: "test2.md".into(),
                        content: "# Doc2\n![[image2.jpg]]".into(),
                    },
                ],
            },
            verify: |paths, _| {
                assert_ne!(
                    paths[0].exists(),
                    paths[1].exists(),
                    "One image should exist and one should be deleted"
                );

                let keeper_name = if paths[0].exists() {
                    "image1.jpg"
                } else {
                    "image2.jpg"
                };

                for (i, md_path) in paths[2..].iter().enumerate() {
                    let content = fs::read_to_string(md_path).unwrap();

                    let possible_refs = [
                        format!("![[{}]]", keeper_name),
                        format!("![[conf/media/{}]]", keeper_name),
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
            _name: "tiff_images",
            setup: TestSetup {
                images: vec![TestImage {
                    name: "image.tiff".into(),
                    content: tiff_header,
                }],
                markdown_files: vec![TestMarkdown {
                    name: "test.md".into(),
                    content: "# Doc\n![[image.tiff]]\nOther content".into(),
                }],
            },
            verify: |paths, _| {
                assert!(!paths[0].exists(), "TIFF image should be deleted");
                let content = fs::read_to_string(&paths[1]).unwrap();
                assert!(!content.contains("![[image.tiff]]"));
                assert!(content.contains("# Doc\nOther content"));
            },
        },
        ImageTestCase {
            _name: "unreferenced_images",
            setup: TestSetup {
                images: vec![TestImage {
                    name: "unused.jpg".into(),
                    content: jpeg_header.clone(),
                }],
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
            if path
                .extension()
                .map_or(false, |ext| ext == MARKDOWN_EXTENSION)
            {
                if let Some(markdown_file) = repository.markdown_files.get_mut(path) {
                    markdown_file.mark_image_reference_as_updated(config.operational_timezone());
                }
            }
        }

        repository.persist().unwrap();

        (test_case.verify)(&created_paths, &repository);
    }
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

    let mut repository = ObsidianRepository::new(&config).unwrap();

    // Run analyze and verify it handles wikilink paths appropriately
    repository.mark_image_files_for_deletion();

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

    // Verify that the missing references are handled correctly
    let markdown_file = &repository.markdown_files.get_mut(&md_file).unwrap();
    let missing_references = &markdown_file
        .image_links
        .filter_by_variant(ImageLinkState::Missing);
    assert_eq!(
        missing_references.len(),
        2,
        "Expected two missing image references"
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

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_duplicate_grouping() {
    let temp_dir = TempDir::new().unwrap();
    let mut builder = validated_config_tests::get_test_validated_config_builder(&temp_dir);
    let config = builder.apply_changes(true).build().unwrap();
    fs::create_dir_all(config.output_folder()).unwrap();

    let test_date = test_utils::eastern_midnight(2024, 1, 15);

    // Create 4 identical files (same content = same hash)
    let content = vec![0xFF, 0xD8, 0xFF, 0xE0]; // Basic JPEG header

    // Create files that will have the same hash
    let files = [
        ("output1.png", content.clone(), vec![]),
        ("output2.png", content.clone(), vec![]),
        ("output3.png", content.clone(), vec!["test1.md"]),
        ("output4.png", content.clone(), vec!["test2.md"]),
    ];

    let mut created_paths = Vec::new();

    // Create the image files
    for (name, image_content, _) in &files {
        let path = TestFileBuilder::new()
            .with_content(image_content.clone())
            .create(&temp_dir, name);
        created_paths.push(path);
    }

    // Create markdown files referencing some of the images
    for (name, _, references) in &files {
        if !references.is_empty() {
            let md_content = references
                .iter()
                .map(|_| format!("![[{}]]", name))
                .collect::<Vec<_>>()
                .join("\n");

            let path = TestFileBuilder::new()
                .with_content(md_content)
                .with_matching_dates(test_date)
                .create(&temp_dir, references[0]);
            created_paths.push(path);
        }
    }

    let repository = ObsidianRepository::new(&config).unwrap();

    // Verify all files are in the same duplicate group
    let duplicates = repository
        .image_files
        .filter_by_predicate(|state| matches!(state, ImageFileState::Duplicate { .. }));

    let keepers = repository
        .image_files
        .filter_by_predicate(|state| matches!(state, ImageFileState::DuplicateKeeper { .. }));

    // Should have exactly one keeper
    assert_eq!(keepers.len(), 1, "Should have exactly one keeper");

    // Should have three duplicates
    assert_eq!(duplicates.len(), 3, "Should have exactly three duplicates");

    // Verify no files were marked as unreferenced
    let unreferenced = repository
        .image_files
        .filter_by_predicate(|state| matches!(state, ImageFileState::Unreferenced));
    assert_eq!(unreferenced.len(), 0, "Should have no unreferenced files");

    // Verify all duplicates share the same hash as the keeper
    if let ImageFileState::DuplicateKeeper { hash: keeper_hash } = &keepers.files[0].image_state {
        for duplicate in duplicates.files {
            if let ImageFileState::Duplicate { hash } = &duplicate.image_state {
                assert_eq!(hash, keeper_hash, "Duplicate hash should match keeper hash");
            }
        }
    }
}

#[test]
fn test_multiple_file_deletion() {
    let temp_dir = TempDir::new().unwrap();
    let mut builder = validated_config_tests::get_test_validated_config_builder(&temp_dir);
    let config = builder.apply_changes(true).build().unwrap();

    // Create multiple files marked for deletion
    let jpeg_header = vec![0xFF, 0xD8, 0xFF, 0xE0];
    let test_setup = TestSetup {
        images: vec![
            TestImage {
                name: "unused1.jpg".into(),
                content: jpeg_header.clone(),
            },
            TestImage {
                name: "unused2.jpg".into(),
                content: jpeg_header.clone(),
            },
            TestImage {
                name: "empty.jpg".into(),
                content: vec![],
            },
        ],
        markdown_files: vec![],
    };

    let created_paths = create_test_files(&temp_dir, &test_setup);
    let mut repository = ObsidianRepository::new(&config).unwrap();

    // Verify all files are marked for deletion
    assert_eq!(
        repository
            .image_files
            .files
            .iter()
            .filter(|f| f.delete)
            .count(),
        3,
        "Expected all files to be marked for deletion"
    );

    repository.persist().unwrap();

    // Verify all files were deleted
    for path in created_paths {
        assert!(!path.exists(), "File should have been deleted: {:?}", path);
    }
}

#[test]
fn test_referenced_and_unreferenced_duplicates() {
    let temp_dir = TempDir::new().unwrap();
    let mut builder = validated_config_tests::get_test_validated_config_builder(&temp_dir);
    let config = builder.apply_changes(true).build().unwrap();

    // Create two sets of duplicate files with different content
    let test_setup = TestSetup {
        images: vec![
            // First set - both unreferenced
            TestImage {
                name: "unreferenced1.jpg".into(),
                content: vec![0xFF, 0xD8, 0xFF, 0xE0, 0x01],
            },
            TestImage {
                name: "unreferenced2.jpg".into(),
                content: vec![0xFF, 0xD8, 0xFF, 0xE0, 0x01],
            },
            // Second set - one will be referenced
            TestImage {
                name: "referenced1.jpg".into(),
                content: vec![0xFF, 0xD8, 0xFF, 0xE0, 0x02],
            },
            TestImage {
                name: "referenced2.jpg".into(),
                content: vec![0xFF, 0xD8, 0xFF, 0xE0, 0x02],
            },
        ],
        markdown_files: vec![TestMarkdown {
            name: "test.md".into(),
            content: "# Test\n![[referenced1.jpg]]".into(),
        }],
    };

    let created_paths = create_test_files(&temp_dir, &test_setup);
    let mut repository = ObsidianRepository::new(&config).unwrap();

    // Mark markdown file for persistence so files can be deleted
    if let Some(markdown_file) = repository.markdown_files.get_mut(&created_paths[4]) {
        markdown_file.mark_image_reference_as_updated(config.operational_timezone());
    }

    repository.persist().unwrap();

    // Verify unreferenced duplicates - both should be deleted
    assert!(
        !created_paths[0].exists(),
        "unreferenced1.jpg should be deleted"
    );
    assert!(
        !created_paths[1].exists(),
        "unreferenced2.jpg should be deleted"
    );

    // Verify referenced duplicates
    assert!(
        created_paths[2].exists(),
        "referenced1.jpg should be kept as it's referenced in markdown"
    );
    assert!(
        !created_paths[3].exists(),
        "referenced2.jpg should be deleted as it's a duplicate"
    );
}

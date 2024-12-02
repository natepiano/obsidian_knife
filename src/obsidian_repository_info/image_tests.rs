use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::scan::scan_folders;
use crate::test_utils::{eastern_midnight, TestFileBuilder};
use crate::utils::ThreadSafeWriter;
use crate::validated_config::get_test_validated_config_builder;
use crate::obsidian_repository_info::obsidian_repository_info_types::{ImageOperation, MarkdownOperation};
use crate::OUTPUT_MARKDOWN_FILE;

// todo: right now these tests validate the old path that doesn't use our new persist
//       but they test the full input/output which is what we want to make sure we haven't
//       missed something while we refactor separate writing tables from changing the code

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_cleanup_images_missing_references() {
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

    let writer = ThreadSafeWriter::new(config.output_folder()).unwrap();

    // Run cleanup images
    repo_info.cleanup_images(&config, &writer).unwrap();
    // repo_info.persist(&config).unwrap();

    // Verify the markdown file was updated
    let updated_content = fs::read_to_string(&md_file).unwrap();

    let today_formatted = Utc::now().format("[[%Y-%m-%d]]").to_string();

    let expected_content = format!(
        "---\ndate_created: \"[[2024-01-15]]\"\ndate_modified: \"{}\"\n---\n# Test\nSome content",
        today_formatted
    );
    assert_eq!(updated_content, expected_content);

    // Verify the missing references were reported
    let output_content =
        fs::read_to_string(config.output_folder().join(OUTPUT_MARKDOWN_FILE)).unwrap();
    assert!(output_content.contains("missing image references"));
    assert!(output_content.contains("missing.jpg"));
    assert!(output_content.contains("also_missing.jpg"));
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_cleanup_images_duplicates() {
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
    let writer = ThreadSafeWriter::new(config.output_folder()).unwrap();

    // Run cleanup images
    repo_info.cleanup_images(&config, &writer).unwrap();

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

    // Verify the duplication was reported
    let output_content =
        fs::read_to_string(config.output_folder().join(OUTPUT_MARKDOWN_FILE)).unwrap();
    assert!(output_content.contains("duplicate images"));
    assert!(output_content.contains("image1.jpg"));
    assert!(output_content.contains("image2.jpg"));
}

struct CleanupTestCase {
    name: &'static str,
    setup: fn(&TempDir) -> Vec<PathBuf>,  // Returns paths created
    expected_ops: fn(&[PathBuf]) -> (Vec<ImageOperation>, Vec<MarkdownOperation>),
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_image_operations_match_cleanup() {
    let test_cases = vec![
        CleanupTestCase {
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
                    }]
                )
            },
        },
        CleanupTestCase {
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
                    vec![ImageOperation::Delete(paths[1].clone())],  // Delete image2.jpg
                    vec![MarkdownOperation::UpdateReference {
                        markdown_path: paths[3].clone(),  // test2.md
                        old_image_path: paths[1].clone(), // image2.jpg
                        new_image_path: paths[0].clone(), // image1.jpg
                    }]
                )
            }
        },
        CleanupTestCase {
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
            expected_ops: expect_delete_remove_reference()
        },
        CleanupTestCase {
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
            expected_ops: expect_delete_remove_reference()
        },
        CleanupTestCase {
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
                    vec![] // No markdown changes needed
                )
            }
        }
    ];

    for test_case in test_cases {
        let temp_dir = TempDir::new().unwrap();
        let mut builder = get_test_validated_config_builder(&temp_dir);
        let config = builder.apply_changes(true).build().unwrap();
        fs::create_dir_all(config.output_folder()).unwrap();

        let created_paths = (test_case.setup)(&temp_dir);
        let mut repo_info = scan_folders(&config).unwrap();
        let writer = ThreadSafeWriter::new(config.output_folder()).unwrap();

        let operations = repo_info.cleanup_images(&config, &writer).unwrap();

        let (expected_image_ops, expected_markdown_ops) = (test_case.expected_ops)(&created_paths);

        for (actual, expected) in operations.image_ops.iter().zip(expected_image_ops.iter()) {
            match (actual, expected) {
                (ImageOperation::Delete(actual_path), ImageOperation::Delete(expected_path)) => {
                    assert_eq!(actual_path.as_path(), expected_path.as_path(),
                               "Test case '{}': Delete operation paths don't match", test_case.name);
                }
            }
        }

        for (i, (actual, expected)) in operations.markdown_ops.iter().zip(expected_markdown_ops.iter()).enumerate() {
            assert!(
                matches!(actual, MarkdownOperation::RemoveReference { .. }) == matches!(expected, MarkdownOperation::RemoveReference { .. }) &&
                    matches!(actual, MarkdownOperation::UpdateReference { .. }) == matches!(expected, MarkdownOperation::UpdateReference { .. }),
                "Test case '{}': Operation type mismatch at position {}", test_case.name, i
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

fn expect_delete_remove_reference() -> fn(&[PathBuf]) -> (Vec<ImageOperation>, Vec<MarkdownOperation>) {
    |paths| {
        (
            vec![ImageOperation::Delete(paths[0].clone())],
            vec![MarkdownOperation::RemoveReference {
                markdown_path: paths[1].clone(),
                image_path: paths[0].clone(),
            }]
        )
    }
}

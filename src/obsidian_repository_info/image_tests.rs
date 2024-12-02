use crate::scan::scan_folders;
use crate::test_utils::{eastern_midnight, TestFileBuilder};
use crate::utils::ThreadSafeWriter;
use crate::validated_config::get_test_validated_config_builder;
use chrono::Utc;
use std::fs;
use tempfile::TempDir;
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

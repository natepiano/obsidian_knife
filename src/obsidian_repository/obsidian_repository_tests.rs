use crate::image_file::{ImageFile, ImageFiles, ImageFileState, IncompatibilityReason};
use crate::obsidian_repository::ObsidianRepository;
use crate::test_utils::TestFileBuilder;
use crate::validated_config::{ValidatedConfig, ValidatedConfigBuilder};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn setup_test_repo() -> (TempDir, ValidatedConfig) {
    let temp_dir = TempDir::new().unwrap();

    // First create the validated config so we know the correct media path
    let config = get_validated_config(&temp_dir);

    // Now create our test files using the config's media path
    let media_path = temp_dir.path().join("media");
    fs::create_dir_all(&media_path).unwrap();

    // Create test cases with TestFileBuilder, putting them in the media folder
    let md_content = r#"---
date_created: 2024-01-01
date_modified: 2024-01-01
---
# Test Special Images
![[zero_byte.png]]
![[test.tiff]]"#;

    TestFileBuilder::new()
        .with_content(md_content.as_bytes().to_vec())
        .create(&temp_dir, "special_images.md");

    TestFileBuilder::new()
        .with_content(vec![]) // Empty content for zero byte file
        .create(&temp_dir, "media/zero_byte.png");

    TestFileBuilder::new()
        .with_content(vec![0x4D, 0x4D, 0x00, 0x2A]) // TIFF header
        .create(&temp_dir, "media/test.tiff");

    (temp_dir, config)
}

fn get_validated_config(temp_dir: &TempDir) -> ValidatedConfig {
    ValidatedConfigBuilder::default()
        .obsidian_path(temp_dir.path().to_path_buf())
        .output_folder(PathBuf::from("output")) // Just the subfolder name
        .operational_timezone("UTC".to_string())
        .build()
        .unwrap()
}

pub fn find_image_file<'a>(files: &'a ImageFiles, path: &'a PathBuf) -> Option<&'a ImageFile> {
    files.files.iter().find(|f| f.path == *path)
}

#[test]
fn test_new_matches_old_structure() -> Result<(), Box<dyn Error + Send + Sync>> {
    let (_temp_dir, config) = setup_test_repo();

    // Create repository info using new method
    let repository = ObsidianRepository::new(&config)?;

    // Verify both structures contain same information
    for (path, image_refs) in &repository.image_path_to_references_map {

        let image_file = find_image_file(&repository.image_files, path)
            .expect("Image in map should exist in image_files");

        // Verify hash matches
        assert_eq!(
            image_file.hash,
            image_refs.hash,
            "Hash mismatch for {}",
            path.display()
        );

        // Verify references
        let refs_count = image_refs.markdown_file_references.len();
        let file_refs_count = image_file.references.len();
        assert_eq!(
            refs_count,
            file_refs_count,
            "Reference count mismatch for {}: map has {}, ImageFile has {}",
            path.display(),
            refs_count,
            file_refs_count
        );
    }

    // Verify all images are accounted for
    assert_eq!(
        repository.image_path_to_references_map.len(),
        repository.image_files.len(),
        "Number of images should match between old and new structures"
    );

    Ok(())
}

#[test]
fn test_new_handles_empty_repo() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new().unwrap();

    let config = get_validated_config(&temp_dir);

    let repository = ObsidianRepository::new(&config)?;

    assert!(repository.image_files.is_empty());
    assert!(repository.image_path_to_references_map.is_empty());

    Ok(())
}

#[test]
fn test_new_handles_special_cases() -> Result<(), Box<dyn Error + Send + Sync>> {
    let (temp_dir, config) = setup_test_repo();

    // Create test cases with TestFileBuilder
    let zero_byte_path = TestFileBuilder::new()
        .with_content(vec![])
        .create(&temp_dir, "media/zero_byte.png");
    let tiff_path = TestFileBuilder::new()
        .with_content(vec![0x4D, 0x4D, 0x00, 0x2A])
        .create(&temp_dir, "media/test.tiff");

    let md_content = r#"---
date_created: 2024-01-01
date_modified: 2024-01-01
---
# Test Special Images
![[zero_byte.png]]
![[test.tiff]]"#;

    let _ = TestFileBuilder::new()
        .with_content(md_content.as_bytes().to_vec())
        .create(&temp_dir, "special_images.md");

    let repository = ObsidianRepository::new(&config)?;

    fn assert_incompatible_image_state(
        files: &ImageFiles,
        path: &PathBuf,
        expected_reason: IncompatibilityReason,
        message: &str,
    ) {
        if let Some(image) = find_image_file(files, path) {
            assert_eq!(
                image.image_state,
                ImageFileState::Incompatible {
                    reason: expected_reason
                },
                "{}", message
            );
        } else {
            panic!("Expected to find file at {:?}", path);
        }
    }

    assert_incompatible_image_state(
        &repository.image_files,
        &zero_byte_path,
        IncompatibilityReason::ZeroByte,
        "Zero-byte file should have ZeroByte state"
    );

    assert_incompatible_image_state(
        &repository.image_files,
        &tiff_path,
        IncompatibilityReason::TiffFormat,
        "TIFF file should have Tiff state"
    );

    Ok(())
}

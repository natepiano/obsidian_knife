use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;

use super::ObsidianRepository;
use super::constants::MIN_DUPLICATE_GROUP_SIZE;
use crate::constants::CACHE_FILE;
use crate::constants::CACHE_FOLDER;
use crate::image_file::DeletionStatus;
use crate::image_file::ImageFile;
use crate::image_file::ImageFileState;
use crate::image_file::ImageFiles;
use crate::image_file::ImageHash;
use crate::image_file::ImageRole;
use crate::markdown_file::ImageLinkState;
use crate::sha256_cache::Sha256Cache;
use crate::support::VecEnumFilter;
use crate::validated_config::ValidatedConfig;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeeperSelection {
    None,
    FirstSortedImage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DuplicateGroupRole {
    Unique,
    Duplicate { keeper_selection: KeeperSelection },
}

impl From<&[(PathBuf, Vec<String>)]> for DuplicateGroupRole {
    fn from(group: &[(PathBuf, Vec<String>)]) -> Self {
        if group.len() < MIN_DUPLICATE_GROUP_SIZE {
            Self::Unique
        } else {
            Self::Duplicate {
                keeper_selection: group.into(),
            }
        }
    }
}

impl DuplicateGroupRole {
    const fn image_role(self, index: usize) -> ImageRole {
        match self {
            Self::Unique => ImageRole::Unique,
            Self::Duplicate {
                keeper_selection: KeeperSelection::FirstSortedImage,
            } if index == 0 => ImageRole::Original,
            Self::Duplicate { .. } => ImageRole::Duplicate,
        }
    }
}

impl From<&[(PathBuf, Vec<String>)]> for KeeperSelection {
    fn from(group: &[(PathBuf, Vec<String>)]) -> Self {
        if group.iter().any(|(_, references)| !references.is_empty()) {
            Self::FirstSortedImage
        } else {
            Self::None
        }
    }
}

impl KeeperSelection {
    const fn should_sort(self) -> bool { matches!(self, Self::FirstSortedImage) }
}

impl ObsidianRepository {
    pub(super) fn initialize_image_files(
        &self,
        image_files: &[PathBuf],
        validated_config: &ValidatedConfig,
    ) -> Result<ImageFiles, Box<dyn Error + Send + Sync>> {
        let mut sha256_cache = Self::initialize_image_cache(validated_config, image_files);

        // `markdown_references` maps each `MarkdownFile.path` to referenced image filenames.
        let markdown_references = self.get_markdown_file_image_reference_map();

        // `hash_groups` groups `image_files` by `ImageHash` and markdown references.
        let hash_groups = Self::get_image_hash_to_markdown_references_map(
            &mut sha256_cache,
            image_files,
            &markdown_references,
        );

        // `images` stores `ImageFile` states chosen from `DuplicateGroupRole`.
        let images = Self::generate_image_files(hash_groups)?;

        // `Sha256Cache::save` persists entries when `Sha256Cache::has_changes` is true.
        if sha256_cache.has_changes() {
            sha256_cache.save()?;
        }

        Ok(ImageFiles { images })
    }

    // `DuplicateGroupRole` selects `ImageFileState::DuplicateKeeper` for the
    // first referenced path and `ImageFileState::Duplicate` for the remaining paths.
    fn generate_image_files(
        hash_groups: HashMap<ImageHash, Vec<(PathBuf, Vec<String>)>>,
    ) -> Result<Vec<ImageFile>, Box<dyn Error + Send + Sync>> {
        let mut images = Vec::new();

        for (image_hash, mut group) in hash_groups {
            let duplicate_group_role = DuplicateGroupRole::from(group.as_slice());

            if matches!(
                duplicate_group_role,
                DuplicateGroupRole::Duplicate { keeper_selection } if keeper_selection.should_sort()
            ) {
                group.sort_by(|a, b| a.0.cmp(&b.0));
            }

            for (idx, (path, references)) in group.into_iter().enumerate() {
                let path_references: Vec<PathBuf> =
                    references.into_iter().map(PathBuf::from).collect();
                let image_role = duplicate_group_role.image_role(idx);

                images.push(ImageFile::new(
                    path,
                    image_hash.clone(),
                    path_references,
                    image_role,
                )?);
            }
        }

        Ok(images)
    }

    // `HashMap<ImageHash, Vec<(PathBuf, Vec<String>)>>` is keyed by `ImageHash`.
    fn get_image_hash_to_markdown_references_map(
        sha256_cache: &mut Sha256Cache,
        image_files: &[PathBuf],
        markdown_references: &HashMap<String, HashSet<String>>,
    ) -> HashMap<ImageHash, Vec<(PathBuf, Vec<String>)>> {
        image_files
            .iter()
            .filter_map(|image_path| {
                // `ok()?` converts `Sha256Cache::get_or_update` into an optional `ImageHash`.
                let (image_hash, _) = sha256_cache.get_or_update(image_path).ok()?; // `ImageHash`
                let image_name = image_path.file_name()?.to_str()?.to_lowercase();

                let references = markdown_references
                    .iter()
                    .filter_map(|(path, image_names)| {
                        if image_names.contains(&image_name) {
                            Some(path.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                Some((image_hash, (image_path.clone(), references))) // Keyed by `ImageHash`
            })
            .fold(HashMap::new(), |mut accumulator, (image_hash, entry)| {
                accumulator.entry(image_hash).or_default().push(entry); // Use `ImageHash` as the key
                accumulator
            })
    }

    // Map of `markdown_file` paths to the image file names referenced on that `markdown_file`.
    fn get_markdown_file_image_reference_map(&self) -> HashMap<String, HashSet<String>> {
        self.markdown_files
            .iter()
            .filter(|file| !file.image_links.is_empty())
            .map(|file| {
                let markdown_file_path = file.path.to_string_lossy().to_string();
                let image_file_names: HashSet<_> = file
                    .image_links
                    .iter()
                    .map(|link| link.filename.to_lowercase())
                    .collect();
                (markdown_file_path, image_file_names)
            })
            .collect::<HashMap<_, _>>()
    }

    fn initialize_image_cache(
        validated_config: &ValidatedConfig,
        image_files: &[PathBuf],
    ) -> Sha256Cache {
        let file_path = validated_config
            .obsidian_path()
            .join(CACHE_FOLDER)
            .join(CACHE_FILE);
        let valid_paths: HashSet<_> = image_files.iter().map(PathBuf::as_path).collect();

        let mut sha256_cache = Sha256Cache::load_or_create(file_path).0;
        sha256_cache.mark_deletions(&valid_paths);
        sha256_cache
    }

    pub(super) fn identify_image_reference_replacements(&mut self) {
        // Missing filenames assign `ImageLinkState::Missing`.
        let image_filenames: HashSet<String> = self
            .image_files
            .iter()
            .filter_map(|image_file| image_file.path.file_name())
            .map(|name| name.to_string_lossy().to_lowercase())
            .collect();

        for markdown_file in &mut self.markdown_files {
            for link in markdown_file.image_links.iter_mut() {
                if !image_filenames.contains(&link.filename.to_lowercase()) {
                    link.state = ImageLinkState::Missing;
                }
            }
        }

        // `ImageFileState::Incompatible` entries assign `ImageLinkState::Incompatible`.
        let incompatible = self.image_files.filter_by_predicate(|image_file_state| {
            matches!(image_file_state, ImageFileState::Incompatible { .. })
        });

        // `ImageFileState::Incompatible` matches set each `ImageLinkState::Incompatible`
        // `reason`; later `ReplaceableContent` collection reads that state.
        for image_file in incompatible.images {
            if let ImageFileState::Incompatible { reason } = &image_file.state {
                let image_file_name = image_file
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap_or_default();
                for markdown_file in &mut self.markdown_files {
                    if let Some(image_link) = markdown_file
                        .image_links
                        .iter_mut()
                        .find(|link| link.filename == image_file_name)
                    {
                        image_link.state = ImageLinkState::Incompatible {
                            reason: reason.clone(),
                        };
                    }
                }
            }
        }
        // `ImageFileState::Duplicate` and `ImageFileState::DuplicateKeeper` entries
        // assign `ImageLinkState::Duplicate`.
        let duplicates = self
            .image_files
            .filter_by_predicate(|state| matches!(state, ImageFileState::Duplicate { .. }));

        let keepers = self
            .image_files
            .filter_by_predicate(|state| matches!(state, ImageFileState::DuplicateKeeper { .. }));

        for duplicate in duplicates.images {
            let duplicate_file_name = duplicate
                .path
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default();
            if let ImageFileState::Duplicate { image_hash } = &duplicate.state
                && let Some(keeper) = keepers.iter().find(|k| {
                    matches!(&k.state, ImageFileState::DuplicateKeeper { image_hash: keeper_image_hash } if keeper_image_hash == image_hash)
                })
            {
                // Duplicate `ImageLink` entries store `ImageLinkState::Duplicate`.
                for markdown_file in &mut self.markdown_files {
                    if let Some(image_link) = markdown_file
                        .image_links
                        .iter_mut()
                        .find(|link| link.filename == duplicate_file_name)
                    {
                        image_link.state = ImageLinkState::Duplicate {
                            keeper_path: keeper.path.clone(),
                        };
                    }
                }
            }
        }
    }

    pub(super) fn mark_image_files_for_deletion(&mut self) {
        // `can_delete` requires every image reference to belong to `files_to_persist`.
        fn can_delete(files_to_persist: &HashSet<&PathBuf>, image_file: &ImageFile) -> bool {
            image_file
                .references
                .iter()
                .all(|path| files_to_persist.contains(&path))
        }

        let files_to_persist = self.markdown_files.files_to_persist();

        let files_to_persist: HashSet<_> = files_to_persist.iter().map(|f| &f.path).collect();

        for image_file in &mut self.image_files.images {
            match &image_file.state {
                ImageFileState::Unreferenced => {
                    image_file.deletion_status = DeletionStatus::Delete;
                },
                ImageFileState::Incompatible { .. } => {
                    if image_file.references.is_empty() || can_delete(&files_to_persist, image_file)
                    {
                        image_file.deletion_status = DeletionStatus::Delete;
                    }
                },
                ImageFileState::Duplicate { .. } => {
                    if can_delete(&files_to_persist, image_file) {
                        image_file.deletion_status = DeletionStatus::Delete;
                    }
                },
                ImageFileState::DuplicateKeeper { .. } | ImageFileState::Valid => (),
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use crate::constants::MARKDOWN_EXTENSION;
    use crate::constants::YAML_CLOSING_DELIMITER_NEWLINE;
    use crate::constants::YAML_OPENING_DELIMITER;
    use crate::frontmatter::FrontMatter;
    use crate::image_file::DeletionStatus;
    use crate::image_file::ImageFileState;
    use crate::markdown_file::ImageLinkState;
    use crate::markdown_file::MarkdownFile;
    use crate::markdown_file::PersistReason;
    use crate::markdown_files::MarkdownFiles;
    use crate::obsidian_repository::ObsidianRepository;
    use crate::support::VecEnumFilter;
    use crate::test_support as test_utils;
    use crate::test_support::TestFileBuilder;
    use crate::validated_config::ChangeMode;
    use crate::yaml_frontmatter::YamlFrontMatter;

    impl MarkdownFiles {
        fn get_mut(&mut self, path: &Path) -> Option<&mut MarkdownFile> {
            self.iter_mut().find(|file| file.path == path)
        }
    }

    struct ImageTestCase {
        setup:  TestSetup,
        verify: VerifyOutcome,
    }

    struct TestSetup {
        images:         Vec<TestImage>,
        markdown_files: Vec<TestMarkdown>,
    }

    struct TestImage {
        name:    String,
        content: Vec<u8>,
    }

    struct TestMarkdown {
        name:    String,
        content: String,
    }

    type VerifyOutcome = fn(&[PathBuf], &ObsidianRepository);

    fn create_test_files(temp_dir: &TempDir, setup: &TestSetup) -> Vec<PathBuf> {
        let test_date = test_utils::eastern_midnight(2024, 1, 15);
        let mut paths = Vec::new();

        for image in &setup.images {
            let path = TestFileBuilder::new()
                .with_content(image.content.clone())
                .create(temp_dir, &image.name);
            paths.push(path);
        }

        for markdown in &setup.markdown_files {
            let path = TestFileBuilder::new()
                .with_content(markdown.content.clone())
                .with_matching_dates(test_date)
                .create(temp_dir, &markdown.name);
            paths.push(path);
        }

        paths
    }

    #[test]
    #[cfg_attr(
        target_os = "linux",
        ignore = "requires filesystem access unavailable on Linux CI"
    )]
    fn test_analyze_missing_references() {
        let temp_dir = TempDir::new().unwrap();
        let mut builder = test_utils::get_test_validated_config_builder(&temp_dir);
        let validated_config = builder.change_mode(ChangeMode::Apply).build().unwrap();
        fs::create_dir_all(validated_config.output_folder()).unwrap();

        let test_date = test_utils::eastern_midnight(2024, 1, 15);
        let markdown_file_path = TestFileBuilder::new()
            .with_content(
                "# Test\n![[missing.jpg]]\nSome content\n![Another](also_missing.jpg)".to_string(),
            )
            .with_matching_dates(test_date)
            .with_file_system_dates(test_date, test_date)
            .create(&temp_dir, "test.md");

        let mut obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();
        if let Some(markdown_file) = obsidian_repository
            .markdown_files
            .get_mut(&markdown_file_path)
        {
            // Instead of using `mark_image_reference_as_updated`, which uses the current date,
            // directly set the date we want
            if let Some(frontmatter) = &mut markdown_file.frontmatter {
                frontmatter.set_date_modified(test_date, validated_config.operational_timezone());
            }
            markdown_file
                .persist_reasons
                .push(PersistReason::ImageReferencesModified);
        }

        obsidian_repository.persist().unwrap();

        let updated_content = fs::read_to_string(&markdown_file_path).unwrap();
        let mut expected_frontmatter = FrontMatter::default();
        expected_frontmatter.set_date_created(test_date, validated_config.operational_timezone());
        expected_frontmatter.set_date_modified(test_date, validated_config.operational_timezone());
        let yaml = expected_frontmatter.to_yaml_str().unwrap();
        let expected_content = format!(
            "{YAML_OPENING_DELIMITER}{}{YAML_CLOSING_DELIMITER_NEWLINE}# Test\nSome content",
            yaml.trim()
        );
        assert_eq!(updated_content, expected_content);

        let mut obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();
        obsidian_repository.mark_image_files_for_deletion();
        obsidian_repository.persist().unwrap();

        let final_content = fs::read_to_string(&markdown_file_path).unwrap();
        assert_eq!(
            final_content, expected_content,
            "Content should not change on second analyze/persist pass"
        );
    }

    #[test]
    #[cfg_attr(
        target_os = "linux",
        ignore = "requires filesystem access unavailable on Linux CI"
    )]
    #[allow(
        clippy::too_many_lines,
        reason = "test case table + assertion loop — not worth splitting"
    )]
    fn test_image_replacement_outcomes() {
        let jpeg_header = vec![0xFF, 0xD8, 0xFF, 0xE0];
        let tiff_header = vec![0x4D, 0x4D, 0x00, 0x2A];
        let empty_content = vec![];

        let test_cases = vec![
            ImageTestCase {
                setup:  TestSetup {
                    images:         vec![TestImage {
                        name:    "empty.jpg".into(),
                        content: empty_content,
                    }],
                    markdown_files: vec![TestMarkdown {
                        name:    "test.md".into(),
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
                setup:  TestSetup {
                    images:         vec![
                        TestImage {
                            name:    "image1.jpg".into(),
                            content: jpeg_header.clone(),
                        },
                        TestImage {
                            name:    "image2.jpg".into(),
                            content: jpeg_header.clone(),
                        },
                    ],
                    markdown_files: vec![
                        TestMarkdown {
                            name:    "test1.md".into(),
                            content: "# Doc1\n![[image1.jpg]]".into(),
                        },
                        TestMarkdown {
                            name:    "test2.md".into(),
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

                    for (i, markdown_path) in paths[2..].iter().enumerate() {
                        let content = fs::read_to_string(markdown_path).unwrap();

                        let possible_refs = [
                            format!("![[{keeper_name}]]"),
                            format!("![[conf/media/{keeper_name}]]"),
                        ];

                        assert!(
                            possible_refs
                                .iter()
                                .any(|ref_str| content.contains(ref_str)),
                            "Markdown file {} should reference keeper image '{}' either directly or in conf/media/\nActual content:\n{}",
                            i + 1,
                            keeper_name,
                            content
                        );
                    }
                },
            },
            ImageTestCase {
                setup:  TestSetup {
                    images:         vec![TestImage {
                        name:    "image.tiff".into(),
                        content: tiff_header,
                    }],
                    markdown_files: vec![TestMarkdown {
                        name:    "test.md".into(),
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
                setup:  TestSetup {
                    images:         vec![TestImage {
                        name:    "unused.jpg".into(),
                        content: jpeg_header,
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
            let mut builder = test_utils::get_test_validated_config_builder(&temp_dir);
            let validated_config = builder.change_mode(ChangeMode::Apply).build().unwrap();
            fs::create_dir_all(validated_config.output_folder()).unwrap();

            let created_paths = create_test_files(&temp_dir, &test_case.setup);
            let mut obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

            // Mark markdown files for persistence
            for path in &created_paths {
                if path
                    .extension()
                    .is_some_and(|ext| ext == MARKDOWN_EXTENSION)
                    && let Some(markdown_file) = obsidian_repository.markdown_files.get_mut(path)
                {
                    markdown_file
                        .mark_image_reference_as_updated(validated_config.operational_timezone())
                        .unwrap();
                }
            }

            obsidian_repository.persist().unwrap();

            (test_case.verify)(&created_paths, &obsidian_repository);
        }
    }

    #[test]
    #[cfg_attr(
        target_os = "linux",
        ignore = "requires filesystem access unavailable on Linux CI"
    )]
    fn test_analyze_wikilink_errors() {
        let temp_dir = TempDir::new().unwrap();
        let mut builder = test_utils::get_test_validated_config_builder(&temp_dir);
        let validated_config = builder.change_mode(ChangeMode::Apply).build().unwrap();
        fs::create_dir_all(validated_config.output_folder()).unwrap();

        let test_date = test_utils::eastern_midnight(2024, 1, 15);
        let markdown_file_path = TestFileBuilder::new()
            .with_content("# Test\n![[[[Some File]]]]".to_string())
            .with_matching_dates(test_date)
            .with_file_system_dates(test_date, test_date)
            .create(&temp_dir, "test_file.md");

        let mut obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        obsidian_repository.mark_image_files_for_deletion();

        let final_content = fs::read_to_string(&markdown_file_path).unwrap();
        assert!(
            final_content.contains("![[[[Some File]]]]"),
            "Content with invalid wikilinks should not be modified"
        );
    }

    #[test]
    fn test_handle_missing_references() {
        let temp_dir = TempDir::new().unwrap();
        let mut builder = test_utils::get_test_validated_config_builder(&temp_dir);
        let validated_config = builder.change_mode(ChangeMode::Apply).build().unwrap();
        fs::create_dir_all(validated_config.output_folder()).unwrap();

        let test_date = test_utils::eastern_midnight(2024, 1, 15);

        let markdown_content = r"# Test Document
![[missing_image1.jpg]]
![[missing_image2.jpg]]
";
        let markdown_file_path = TestFileBuilder::new()
            .with_content(markdown_content.to_string())
            .with_matching_dates(test_date)
            .create(&temp_dir, "test_doc.md");

        let mut obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        let markdown_file = &obsidian_repository
            .markdown_files
            .get_mut(&markdown_file_path)
            .unwrap();
        let missing_references = &markdown_file
            .image_links
            .filter_by_variant(ImageLinkState::Missing);
        assert_eq!(
            missing_references.len(),
            2,
            "Expected two missing image references"
        );

        assert!(
            !&markdown_file.content.contains("![[missing_image1.jpg]]")
                && !&markdown_file.content.contains("![[missing_image2.jpg]]"),
            "`MarkdownFile` content should not contain missing references"
        );

        assert!(
            &markdown_file.frontmatter.as_ref().unwrap().needs_persist(),
            "needs persist should better well be true, boyo"
        );
    }

    #[test]
    #[cfg_attr(
        target_os = "linux",
        ignore = "requires filesystem access unavailable on Linux CI"
    )]
    fn test_duplicate_grouping() {
        let temp_dir = TempDir::new().unwrap();
        let mut builder = test_utils::get_test_validated_config_builder(&temp_dir);
        let validated_config = builder.change_mode(ChangeMode::Apply).build().unwrap();
        fs::create_dir_all(validated_config.output_folder()).unwrap();

        let test_date = test_utils::eastern_midnight(2024, 1, 15);

        let content = vec![0xFF, 0xD8, 0xFF, 0xE0]; // Basic JPEG header

        let files = [
            ("output1.png", content.clone(), vec![]),
            ("output2.png", content.clone(), vec![]),
            ("output3.png", content.clone(), vec!["test1.md"]),
            ("output4.png", content, vec!["test2.md"]),
        ];

        for (name, image_content, _) in &files {
            TestFileBuilder::new()
                .with_content(image_content.clone())
                .create(&temp_dir, name);
        }

        for (name, _, references) in &files {
            if !references.is_empty() {
                let markdown_content = references
                    .iter()
                    .map(|_| format!("![[{name}]]"))
                    .collect::<Vec<_>>()
                    .join("\n");

                TestFileBuilder::new()
                    .with_content(markdown_content)
                    .with_matching_dates(test_date)
                    .create(&temp_dir, references[0]);
            }
        }

        let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        let duplicates = obsidian_repository
            .image_files
            .filter_by_predicate(|state| matches!(state, ImageFileState::Duplicate { .. }));

        let keepers = obsidian_repository
            .image_files
            .filter_by_predicate(|state| matches!(state, ImageFileState::DuplicateKeeper { .. }));

        assert_eq!(keepers.len(), 1, "Should have exactly one keeper");

        assert_eq!(duplicates.len(), 3, "Should have exactly three duplicates");

        let unreferenced = obsidian_repository
            .image_files
            .filter_by_predicate(|state| matches!(state, ImageFileState::Unreferenced));
        assert_eq!(unreferenced.len(), 0, "Should have no unreferenced files");

        if let ImageFileState::DuplicateKeeper {
            image_hash: keeper_image_hash,
        } = &keepers.images[0].state
        {
            for duplicate in duplicates.images {
                if let ImageFileState::Duplicate { image_hash } = &duplicate.state {
                    assert_eq!(
                        image_hash, keeper_image_hash,
                        "Duplicate hash should match keeper hash"
                    );
                }
            }
        }
    }

    #[test]
    fn test_multiple_file_deletion() {
        let temp_dir = TempDir::new().unwrap();
        let mut builder = test_utils::get_test_validated_config_builder(&temp_dir);
        let validated_config = builder.change_mode(ChangeMode::Apply).build().unwrap();

        let jpeg_header = vec![0xFF, 0xD8, 0xFF, 0xE0];
        let test_setup = TestSetup {
            images:         vec![
                TestImage {
                    name:    "unused1.jpg".into(),
                    content: jpeg_header.clone(),
                },
                TestImage {
                    name:    "unused2.jpg".into(),
                    content: jpeg_header,
                },
                TestImage {
                    name:    "empty.jpg".into(),
                    content: vec![],
                },
            ],
            markdown_files: vec![],
        };

        let created_paths = create_test_files(&temp_dir, &test_setup);
        let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        assert_eq!(
            obsidian_repository
                .image_files
                .iter()
                .filter(|f| f.deletion_status == DeletionStatus::Delete)
                .count(),
            3,
            "Expected all files to be marked for deletion"
        );

        obsidian_repository.persist().unwrap();

        for path in created_paths {
            assert!(!path.exists(), "File should have been deleted: {path:?}");
        }
    }

    #[test]
    fn test_referenced_and_unreferenced_duplicates() {
        let temp_dir = TempDir::new().unwrap();
        let mut builder = test_utils::get_test_validated_config_builder(&temp_dir);
        let validated_config = builder.change_mode(ChangeMode::Apply).build().unwrap();

        let test_setup = TestSetup {
            images:         vec![
                TestImage {
                    name:    "unreferenced1.jpg".into(),
                    content: vec![0xFF, 0xD8, 0xFF, 0xE0, 0x01],
                },
                TestImage {
                    name:    "unreferenced2.jpg".into(),
                    content: vec![0xFF, 0xD8, 0xFF, 0xE0, 0x01],
                },
                TestImage {
                    name:    "referenced1.jpg".into(),
                    content: vec![0xFF, 0xD8, 0xFF, 0xE0, 0x02],
                },
                TestImage {
                    name:    "referenced2.jpg".into(),
                    content: vec![0xFF, 0xD8, 0xFF, 0xE0, 0x02],
                },
            ],
            markdown_files: vec![TestMarkdown {
                name:    "test.md".into(),
                content: "# Test\n![[referenced1.jpg]]".into(),
            }],
        };

        let created_paths = create_test_files(&temp_dir, &test_setup);
        let mut obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        // Mark markdown file for persistence so files can be deleted
        if let Some(markdown_file) = obsidian_repository
            .markdown_files
            .get_mut(&created_paths[4])
        {
            markdown_file
                .mark_image_reference_as_updated(validated_config.operational_timezone())
                .unwrap();
        }

        obsidian_repository.persist().unwrap();

        assert!(
            !created_paths[0].exists(),
            "unreferenced1.jpg should be deleted"
        );
        assert!(
            !created_paths[1].exists(),
            "unreferenced2.jpg should be deleted"
        );

        assert!(
            created_paths[2].exists(),
            "referenced1.jpg should be kept as it's referenced in markdown"
        );
        assert!(
            !created_paths[3].exists(),
            "referenced2.jpg should be deleted as it's a duplicate"
        );
    }
}

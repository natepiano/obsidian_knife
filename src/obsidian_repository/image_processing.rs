use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;

use super::ObsidianRepository;
use crate::constants::CACHE_FILE;
use crate::constants::CACHE_FOLDER;
use crate::image_file::DeletionStatus;
use crate::image_file::DuplicateRole;
use crate::image_file::ImageFile;
use crate::image_file::ImageFileState;
use crate::image_file::ImageFiles;
use crate::image_file::ImageHash;
use crate::markdown_file::ImageLinkState;
use crate::utils::Sha256Cache;
use crate::utils::VecEnumFilter;
use crate::validated_config::ValidatedConfig;

impl ObsidianRepository {
    pub fn initialize_image_files(
        &self,
        image_files: &[PathBuf],
        validated_config: &ValidatedConfig,
    ) -> Result<ImageFiles, Box<dyn Error + Send + Sync>> {
        let mut cache = Self::initialize_image_cache(validated_config, image_files);

        // Step 1: Create a map of markdown_file_path to their referenced image_file_names
        let markdown_references = self.get_markdown_file_image_reference_map();

        // Step 2: Build an image hash-based grouping for duplicate handling
        let hash_groups = Self::get_image_hash_to_markdown_references_map(
            &mut cache,
            image_files,
            &markdown_references,
        );

        // Step 3: Generate `ImageFiles` with duplicate and keeper logic
        let files = Self::generate_image_files(hash_groups);

        // Step 4: Save cache if needed
        if cache.has_changes() {
            cache.save()?;
        }

        Ok(ImageFiles { files })
    }

    // if a group has multiple references, check if any are referenced
    // the first referenced file is marked as a `DuplicateKeeper`
    // remaining files are marked as `Duplicate`
    fn generate_image_files(
        hash_groups: HashMap<ImageHash, Vec<(PathBuf, Vec<String>)>>,
    ) -> Vec<ImageFile> {
        hash_groups
            .into_iter()
            .flat_map(|(hash, mut group)| {
                let is_duplicate_group = group.len() > 1;
                let mut should_have_keeper = false;

                if is_duplicate_group {
                    let any_referenced = group.iter().any(|(_, refs)| !refs.is_empty());
                    if any_referenced {
                        should_have_keeper = true;
                        group.sort_by(|a, b| a.0.cmp(&b.0));
                    }
                }

                group
                    .into_iter()
                    .enumerate()
                    .map(move |(idx, (path, references))| {
                        let path_references: Vec<PathBuf> =
                            references.into_iter().map(PathBuf::from).collect();
                        let duplicate_role = if !is_duplicate_group {
                            DuplicateRole::NotDuplicate
                        } else if should_have_keeper && idx == 0 {
                            DuplicateRole::Original
                        } else {
                            DuplicateRole::Duplicate
                        };
                        ImageFile::new(path, hash.clone(), path_references, duplicate_role)
                    })
            })
            .collect()
    }

    // this map is keyed on image hash
    fn get_image_hash_to_markdown_references_map(
        cache: &mut Sha256Cache,
        image_files: &[PathBuf],
        markdown_references: &HashMap<String, HashSet<String>>,
    ) -> HashMap<ImageHash, Vec<(PathBuf, Vec<String>)>> {
        image_files
            .iter()
            .filter_map(|image_path| {
                // Use `ok()?` to convert Result to Option and get ImageHash
                let (hash, _) = cache.get_or_update(image_path).ok()?; // hash is `ImageHash`
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

                Some((hash, (image_path.clone(), references))) // Keyed by `ImageHash`
            })
            .fold(HashMap::new(), |mut acc, (hash, entry)| {
                acc.entry(hash).or_default().push(entry); // Use `ImageHash` as the key
                acc
            })
    }

    // map of markdown file paths to the image file names that are referenced on that markdown_file
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
        let cache_file_path = validated_config
            .obsidian_path()
            .join(CACHE_FOLDER)
            .join(CACHE_FILE);
        let valid_paths: HashSet<_> = image_files
            .iter()
            .map(std::path::PathBuf::as_path)
            .collect();

        let mut cache = Sha256Cache::load_or_create(cache_file_path).0;
        cache.mark_deletions(&valid_paths);
        cache
    }

    pub(super) fn identify_image_reference_replacements(&mut self) {
        // first handle missing references
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

        // next handle incompatible image references
        let incompatible = self.image_files.filter_by_predicate(|image_file_state| {
            matches!(image_file_state, ImageFileState::Incompatible { .. })
        });

        // match tiff/zero_byte image files to `image_links` that refer to them so we can mark the
        // `image_link` as incompatible the `image_link` will then be collected as a
        // `ReplaceableContent` match which happens in the next step
        for image_file in incompatible.files {
            if let ImageFileState::Incompatible { reason } = &image_file.image_state {
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
        // last handle duplicates
        let duplicates = self
            .image_files
            .filter_by_predicate(|state| matches!(state, ImageFileState::Duplicate { .. }));

        let keepers = self
            .image_files
            .filter_by_predicate(|state| matches!(state, ImageFileState::DuplicateKeeper { .. }));

        for duplicate in duplicates.files {
            let duplicate_file_name = duplicate
                .path
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default();
            if let ImageFileState::Duplicate { hash } = &duplicate.image_state {
                // Find the keeper with matching hash
                if let Some(keeper) = keepers.iter().find(|k| {
                    matches!(&k.image_state, ImageFileState::DuplicateKeeper { hash: keeper_hash } if keeper_hash == hash)
                }) {
                    // Update `ImageLink` states in markdown files
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
    }

    pub(super) fn mark_image_files_for_deletion(&mut self) {
        // Check if all references are in files being persisted
        fn can_delete(files_to_persist: &HashSet<&PathBuf>, image_file: &ImageFile) -> bool {
            image_file
                .markdown_file_references
                .iter()
                .all(|path| files_to_persist.contains(&path))
        }

        let files_to_persist = self.markdown_files.files_to_persist();

        let files_to_persist: HashSet<_> = files_to_persist.iter().map(|f| &f.path).collect();

        for image_file in &mut self.image_files.files {
            match &image_file.image_state {
                ImageFileState::Unreferenced => {
                    image_file.deletion = DeletionStatus::Delete;
                },
                ImageFileState::Incompatible { .. } => {
                    if image_file.markdown_file_references.is_empty()
                        || can_delete(&files_to_persist, image_file)
                    {
                        image_file.deletion = DeletionStatus::Delete;
                    }
                },
                ImageFileState::Duplicate { .. } => {
                    if can_delete(&files_to_persist, image_file) {
                        image_file.deletion = DeletionStatus::Delete;
                    }
                },
                ImageFileState::DuplicateKeeper { .. } | ImageFileState::Valid => (),
            }
        }
    }
}

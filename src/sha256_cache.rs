use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::fmt::Write;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;

use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

use crate::constants::SHA256_BUFFER_SIZE;
use crate::image_file::ImageHash;

#[derive(Debug, Clone, Copy)]
pub(crate) enum CacheFileStatus {
    ReadFromCache,
    CreatedNewCache,
    CacheCorrupted,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CacheEntryStatus {
    Read,
    Added,
    Modified,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CachedImageInfo {
    pub hash:       ImageHash,
    pub time_stamp: SystemTime,
}

#[derive(Debug)]
pub(crate) struct Sha256Cache {
    cache:               HashMap<PathBuf, CachedImageInfo>,
    cache_file_path:     PathBuf,
    reads:               usize,
    pub(super) added:    usize,
    pub(super) modified: usize,
    deleted:             usize,
}

impl Sha256Cache {
    pub(crate) fn load_or_create(cache_file_path: PathBuf) -> (Self, CacheFileStatus) {
        let (cache, status) = if cache_file_path.exists() {
            File::open(&cache_file_path).map_or_else(
                |_| (HashMap::new(), CacheFileStatus::CreatedNewCache),
                |file| {
                    let reader = BufReader::new(file);
                    serde_json::from_reader(reader).map_or_else(
                        |_| (HashMap::new(), CacheFileStatus::CacheCorrupted),
                        |parsed_cache| (parsed_cache, CacheFileStatus::ReadFromCache),
                    )
                },
            )
        } else {
            (HashMap::new(), CacheFileStatus::CreatedNewCache)
        };

        (
            Self {
                cache,
                cache_file_path,
                reads: 0,
                added: 0,
                modified: 0,
                deleted: 0,
            },
            status,
        )
    }

    pub(crate) fn get_or_update(
        &mut self,
        path: &Path,
    ) -> Result<(ImageHash, CacheEntryStatus), Box<dyn Error + Send + Sync>> {
        let metadata = fs::metadata(path)?;
        let time_stamp = metadata.modified()?;

        if let Some(cached_info) = self.cache.get(path)
            && cached_info.time_stamp == time_stamp
        {
            self.reads += 1;
            return Ok((cached_info.hash.clone(), CacheEntryStatus::Read));
        }

        let new_hash = ImageHash::from(Self::hash_file(path)?);
        let status = if self.cache.contains_key(path) {
            self.modified += 1;
            CacheEntryStatus::Modified
        } else {
            self.added += 1;
            CacheEntryStatus::Added
        };

        self.cache.insert(
            path.to_path_buf(),
            CachedImageInfo {
                hash: new_hash.clone(),
                time_stamp,
            },
        );

        Ok((new_hash, status))
    }

    pub(crate) fn mark_deletions(&mut self, valid_paths: &HashSet<&Path>) {
        // Create vector of paths that exist in cache but not in `valid_paths`
        let to_remove: Vec<_> = self
            .cache
            .keys() // Iterate over all paths in cache
            .filter(|path| !valid_paths.contains(path.as_path())) // Keep only paths NOT in valid_paths
            .cloned() // Clone the `PathBuf`s we want to remove
            .collect(); // Collect into Vec

        self.deleted = to_remove.len();
        for path in to_remove {
            self.cache.remove(&path);
        }
    }

    pub(crate) const fn has_changes(&self) -> bool {
        self.added > 0 || self.modified > 0 || self.deleted > 0
    }

    pub(crate) fn save(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(parent) = self.cache_file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(&self.cache_file_path)?;
        serde_json::to_writer(file, &self.cache)?;
        Ok(())
    }

    fn hash_file(path: &Path) -> Result<String, Box<dyn Error + Send + Sync>> {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0; SHA256_BUFFER_SIZE];

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let hash = hasher.finalize();
        let mut hex = String::with_capacity(hash.len() * 2);
        for byte in hash {
            let _ = write!(hex, "{byte:02x}");
        }
        Ok(hex)
    }
}

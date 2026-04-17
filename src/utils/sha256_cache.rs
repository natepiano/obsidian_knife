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

use crate::image_file::ImageHash;

#[derive(Debug, Clone, Copy)]
pub enum CacheFileStatus {
    ReadFromCache,
    CreatedNewCache,
    CacheCorrupted,
}

#[derive(Debug, Clone, Copy)]
pub enum CacheEntryStatus {
    Read,
    Added,
    Modified,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedImageInfo {
    pub hash:       ImageHash,
    pub time_stamp: SystemTime,
}

#[derive(Debug)]
pub struct Sha256Cache {
    cache:                     HashMap<PathBuf, CachedImageInfo>,
    cache_file_path:           PathBuf,
    read_count:                usize,
    pub(super) added_count:    usize,
    pub(super) modified_count: usize,
    deleted_count:             usize,
}

impl Sha256Cache {
    pub fn load_or_create(cache_file_path: PathBuf) -> (Self, CacheFileStatus) {
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
                read_count: 0,
                added_count: 0,
                modified_count: 0,
                deleted_count: 0,
            },
            status,
        )
    }

    pub fn get_or_update(
        &mut self,
        path: &Path,
    ) -> Result<(ImageHash, CacheEntryStatus), Box<dyn Error + Send + Sync>> {
        let metadata = fs::metadata(path)?;
        let time_stamp = metadata.modified()?;

        if let Some(cached_info) = self.cache.get(path)
            && cached_info.time_stamp == time_stamp
        {
            self.read_count += 1;
            return Ok((cached_info.hash.clone(), CacheEntryStatus::Read));
        }

        let new_hash = ImageHash::from(Self::hash_file(path)?);
        let status = if self.cache.contains_key(path) {
            self.modified_count += 1;
            CacheEntryStatus::Modified
        } else {
            self.added_count += 1;
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

    pub fn mark_deletions(&mut self, valid_paths: &HashSet<&Path>) {
        // Create vector of paths that exist in cache but not in `valid_paths`
        let to_remove: Vec<_> = self
            .cache
            .keys() // Iterate over all paths in cache
            .filter(|path| !valid_paths.contains(path.as_path())) // Keep only paths NOT in valid_paths
            .cloned() // Clone the `PathBuf`s we want to remove
            .collect(); // Collect into Vec

        self.deleted_count = to_remove.len();
        for path in to_remove {
            self.cache.remove(&path);
        }
    }

    pub const fn has_changes(&self) -> bool {
        self.added_count > 0 || self.modified_count > 0 || self.deleted_count > 0
    }

    pub fn save(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
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
        let mut buffer = [0; 1024];

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

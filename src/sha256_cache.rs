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
use serde_json::from_reader;
use serde_json::to_writer;
use sha2::Digest;
use sha2::Sha256;

use crate::constants::HEX_DIGITS_PER_BYTE;
use crate::constants::SHA256_BUFFER_SIZE;
use crate::image_file::ImageHash;

#[derive(Debug, Clone, Copy)]
pub(crate) enum CacheFileStatus {
    Read,
    Created,
    Corrupted,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CacheEntryStatus {
    Read,
    Added,
    Modified,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CachedImageInfo {
    #[serde(rename = "hash")]
    pub image_hash: ImageHash,
    pub time_stamp: SystemTime,
}

#[derive(Debug)]
pub(crate) struct Sha256Cache {
    entries:             HashMap<PathBuf, CachedImageInfo>,
    file_path:           PathBuf,
    reads:               usize,
    pub(super) added:    usize,
    pub(super) modified: usize,
    deleted:             usize,
}

impl Sha256Cache {
    pub(crate) fn load_or_create(file_path: PathBuf) -> (Self, CacheFileStatus) {
        let (entries, status) = if file_path.exists() {
            File::open(&file_path).map_or_else(
                |_| (HashMap::new(), CacheFileStatus::Created),
                |file| {
                    let buf_reader = BufReader::new(file);
                    from_reader(buf_reader).map_or_else(
                        |_| (HashMap::new(), CacheFileStatus::Corrupted),
                        |parsed_cache| (parsed_cache, CacheFileStatus::Read),
                    )
                },
            )
        } else {
            (HashMap::new(), CacheFileStatus::Created)
        };

        (
            Self {
                entries,
                file_path,
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

        if let Some(cached_info) = self.entries.get(path)
            && cached_info.time_stamp == time_stamp
        {
            self.reads += 1;
            return Ok((cached_info.image_hash.clone(), CacheEntryStatus::Read));
        }

        let new_image_hash = ImageHash::from(Self::hash_file(path)?);
        let status = if self.entries.contains_key(path) {
            self.modified += 1;
            CacheEntryStatus::Modified
        } else {
            self.added += 1;
            CacheEntryStatus::Added
        };

        self.entries.insert(
            path.to_path_buf(),
            CachedImageInfo {
                image_hash: new_image_hash.clone(),
                time_stamp,
            },
        );

        Ok((new_image_hash, status))
    }

    pub(crate) fn mark_deletions(&mut self, valid_paths: &HashSet<&Path>) {
        // to_remove lists Sha256Cache entries absent from valid_paths.
        let to_remove: Vec<_> = self
            .entries
            .keys()
            .filter(|path| !valid_paths.contains(path.as_path()))
            .cloned()
            .collect();

        self.deleted = to_remove.len();
        for path in to_remove {
            self.entries.remove(&path);
        }
    }

    pub(crate) const fn has_changes(&self) -> bool {
        self.added > 0 || self.modified > 0 || self.deleted > 0
    }

    pub(crate) fn save(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(&self.file_path)?;
        to_writer(file, &self.entries)?;
        Ok(())
    }

    fn hash_file(path: &Path) -> Result<String, Box<dyn Error + Send + Sync>> {
        let mut file = File::open(path)?;
        let mut sha256_hasher = Sha256::new();
        let mut buffer = [0; SHA256_BUFFER_SIZE];

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            sha256_hasher.update(&buffer[..bytes_read]);
        }

        let hash = sha256_hasher.finalize();
        let mut hex = String::with_capacity(hash.len() * HEX_DIGITS_PER_BYTE);
        for byte in hash {
            let _ = write!(hex, "{byte:02x}");
        }
        Ok(hex)
    }
}

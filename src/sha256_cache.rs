use std::collections::HashMap;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};

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
    pub hash: String,
    pub time_stamp: SystemTime,
}

pub struct Sha256Cache {
    cache: HashMap<PathBuf, CachedImageInfo>,
    cache_file_path: PathBuf,
    files_read: usize,
    files_added: usize,
    files_modified: usize,
}

impl Sha256Cache {
    pub fn new(cache_file_path: PathBuf) -> Result<(Self, CacheFileStatus), Box<dyn Error + Send + Sync>> {
        let (cache, status) = if cache_file_path.exists() {
            match File::open(&cache_file_path) {
                Ok(file) => {
                    let reader = BufReader::new(file);
                    match serde_json::from_reader(reader) {
                        Ok(parsed_cache) => (parsed_cache, CacheFileStatus::ReadFromCache),
                        Err(_) => (HashMap::new(), CacheFileStatus::CacheCorrupted)
                    }
                },
                Err(_) => (HashMap::new(), CacheFileStatus::CreatedNewCache)
            }
        } else {
            (HashMap::new(), CacheFileStatus::CreatedNewCache)
        };

        Ok((Sha256Cache {
            cache,
            cache_file_path,
            files_read: 0,
            files_added: 0,
            files_modified: 0,
        }, status))
    }

    pub fn get_or_update(&mut self, path: &Path) -> Result<(String, CacheEntryStatus), Box<dyn Error + Send + Sync>> {
        let metadata = fs::metadata(path)?;
        let time_stamp = metadata.modified()?;

        if let Some(cached_info) = self.cache.get(path) {
            if cached_info.time_stamp == time_stamp {
                self.files_read += 1;
                return Ok((cached_info.hash.clone(), CacheEntryStatus::Read));
            }
        }

        let new_hash = self.hash_file(path)?;
        let status = if self.cache.contains_key(path) {
            self.files_modified += 1;
            CacheEntryStatus::Modified
        } else {
            self.files_added += 1;
            CacheEntryStatus::Added
        };

        self.cache.insert(path.to_path_buf(), CachedImageInfo {
            hash: new_hash.clone(),
            time_stamp,
        });

        Ok((new_hash, status))
    }

    pub fn remove_non_existent_entries(&mut self) -> usize {
        let initial_count = self.cache.len();
        self.cache.retain(|path, _| path.exists());
        initial_count - self.cache.len()
    }

    pub fn save(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(parent) = self.cache_file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(&self.cache_file_path)?;
        serde_json::to_writer(file, &self.cache)?;
        Ok(())
    }

    pub fn get_stats(&self) -> (usize, usize, usize, usize) {
        (self.files_read, self.files_added, self.files_modified, self.cache.len())
    }

    pub fn get_initial_count(&self) -> usize {
        self.cache.len()
    }

    fn hash_file(&self, path: &Path) -> Result<String, Box<dyn Error + Send + Sync>> {
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

        Ok(format!("{:x}", hasher.finalize()))
    }
}

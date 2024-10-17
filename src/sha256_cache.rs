use std::collections::HashMap;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use crate::thread_safe_writer::ThreadSafeWriter;

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedImageInfo {
    pub hash: String,
    pub modified: SystemTime,
}

pub struct Sha256Cache {
    cache: HashMap<PathBuf, CachedImageInfo>,
    cache_file_path: PathBuf,
}

impl Sha256Cache {
    pub fn new(cache_file_path: PathBuf, writer: &ThreadSafeWriter) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let cache = if cache_file_path.exists() {
            match File::open(&cache_file_path) {
                Ok(file) => {
                    let reader = BufReader::new(file);
                    match serde_json::from_reader(reader) {
                        Ok(parsed_cache) => parsed_cache,
                        Err(e) => {
                            writer.writeln_markdown("### Warning", "Failed to parse cache file:")?;
                            writer.writeln_markdown("Cache file: ", &format!("{}", cache_file_path.display()))?;
                            writer.writeln_markdown("", &format!("Error: {}", e))?;
                            writer.writeln_markdown("", "\nStarting with a new, empty cache")?;

                            HashMap::new()
                        }
                    }
                },
                Err(e) => {
                    writer.writeln_markdown("### Error", &format!("Failed to open cache file: {}. Error: {}.", cache_file_path.display(), e))?;
                    return Err(Box::new(e));
                }
            }
        } else {
            HashMap::new()
        };

        Ok(Sha256Cache {
            cache,
            cache_file_path,
        })
    }

    pub fn get_or_update(&mut self, path: &Path) -> Result<String, Box<dyn Error + Send + Sync>> {
        let metadata = fs::metadata(path)?;
        let modified = metadata.modified()?;

        if let Some(cached_info) = self.cache.get(path) {
            if cached_info.modified == modified {
                return Ok(cached_info.hash.clone());
            }
        }

        let new_hash = self.hash_file(path)?;
        self.cache.insert(path.to_path_buf(), CachedImageInfo {
            hash: new_hash.clone(),
            modified,
        });

        Ok(new_hash)
    }

    pub fn remove_non_existent_entries(&mut self) {
        self.cache.retain(|path, _| path.exists());
    }

    pub fn save(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(parent) = self.cache_file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(&self.cache_file_path)?;
        serde_json::to_writer(file, &self.cache)?;
        Ok(())
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

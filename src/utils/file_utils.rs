use crate::{ValidatedConfig, DS_STORE, MARKDOWN_EXTENSION};
use crate::{ERROR_NOT_FOUND, ERROR_READING, IMAGE_EXTENSIONS};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::{fs, io};

pub fn read_contents_from_file(path: &Path) -> Result<String, Box<dyn Error + Send + Sync>> {
    let contents = fs::read_to_string(path).map_err(|e| -> Box<dyn Error + Send + Sync> {
        if e.kind() == io::ErrorKind::NotFound {
            Box::new(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{}{}", ERROR_NOT_FOUND, path.display()),
            ))
        } else {
            Box::new(io::Error::new(
                e.kind(),
                format!("{}'{}': {}", ERROR_READING, path.display(), e),
            ))
        }
    })?;
    Ok(contents)
}

// Expands a path that starts with `~/` to the user's home directory.
pub fn expand_tilde<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = path.as_ref();

    // Handle paths that start with "~/"
    if let Some(path_str) = path.to_str() {
        if let Some(home) = std::env::var_os("HOME") {
            if let Some(stripped) = path_str.strip_prefix("~/") {
                return PathBuf::from(home).join(stripped);
            }
        }
    } else {
        // Handle invalid UTF-8 paths (OsStr -> PathBuf without assuming valid UTF-8)
        let mut components = path.components();
        if let Some(std::path::Component::Normal(first)) = components.next() {
            if first == "~" {
                if let Some(home) = std::env::var_os("HOME") {
                    let mut expanded_path = PathBuf::from(home);
                    expanded_path.extend(components);
                    return expanded_path;
                }
            }
        }
    }

    // Return the original path if no tilde expansion was needed
    path.to_path_buf()
}

pub struct RepositoryFiles {
    pub image_files: Vec<PathBuf>,
    pub markdown_files: Vec<PathBuf>,
    pub other_files: Vec<PathBuf>,
}

// using rayon (.into_par_iter()) and not using walkdir
// takes this from 12ms down to 4ms
pub fn collect_repository_files(
    validated_config: &ValidatedConfig,
    ignore_folders: &[PathBuf],
) -> Result<RepositoryFiles, Box<dyn Error + Send + Sync>> {
    fn is_ignored(path: &Path, ignore_folders: &[PathBuf]) -> bool {
        ignore_folders
            .iter()
            .any(|ignored| path.starts_with(ignored))
    }

    let md_files = Mutex::new(Vec::new());
    let img_files = Mutex::new(Vec::new());
    let other_files = Mutex::new(Vec::new());

    fn visit_dirs(
        dirs: Vec<PathBuf>,
        ignore_folders: &[PathBuf],
        md_files: &Mutex<Vec<PathBuf>>,
        img_files: &Mutex<Vec<PathBuf>>,
        other_files: &Mutex<Vec<PathBuf>>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        dirs.into_par_iter().try_for_each(|dir| {
            if is_ignored(&dir, ignore_folders) {
                return Ok(());
            }

            let subdirs: Vec<PathBuf> = fs::read_dir(&dir)?
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .filter(|path| path.file_name().and_then(|s| s.to_str()) != Some(DS_STORE))
                .inspect(|path| {
                    if let Some(ext) = path
                        .extension()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_lowercase())
                    {
                        let mutex = if ext == MARKDOWN_EXTENSION {
                            md_files
                        } else if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                            img_files
                        } else {
                            other_files
                        };
                        mutex.lock().unwrap().push(path.clone());
                    }
                })
                .filter(|path| path.is_dir())
                .collect();

            if !subdirs.is_empty() {
                visit_dirs(subdirs, ignore_folders, md_files, img_files, other_files)?;
            }
            Ok(())
        })
    }

    visit_dirs(
        vec![validated_config.obsidian_path().to_path_buf()],
        ignore_folders,
        &md_files,
        &img_files,
        &other_files,
    )?;

    Ok(RepositoryFiles {
        markdown_files: md_files.into_inner().unwrap(),
        image_files: img_files.into_inner().unwrap(),
        other_files: other_files.into_inner().unwrap(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde() {
        // Only run this test if HOME is set
        if let Some(home) = std::env::var_os("HOME") {
            let input = "~/Documents/brain";
            let expected = PathBuf::from(home).join("Documents/brain");
            let expanded = expand_tilde(input);
            assert_eq!(expanded, expected);
        }
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        let input = "/usr/local/bin";
        let expected = PathBuf::from("/usr/local/bin");
        let expanded = expand_tilde(input);
        assert_eq!(expanded, expected);
    }

    #[test]
    fn test_expand_tilde_invalid_utf8() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        // Create a path with invalid UTF-8
        let bytes = b"~/invalid-\xFF-path";
        let os_str = OsStr::from_bytes(bytes);
        let path = Path::new(os_str);

        let expanded = expand_tilde(path);

        // Since HOME is unlikely to contain invalid bytes, the tilde should be expanded
        if let Some(home) = std::env::var_os("HOME") {
            let mut expected = PathBuf::from(home);
            expected.push(OsStr::from_bytes(b"invalid-\xFF-path"));
            assert_eq!(expanded, expected);
        } else {
            // If HOME is not set, the path should remain unchanged
            assert_eq!(
                expanded,
                PathBuf::from(OsStr::from_bytes(b"~/invalid-\xFF-path"))
            );
        }
    }
}

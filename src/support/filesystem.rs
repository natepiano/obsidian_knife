use std::env::var_os;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::io::ErrorKind;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;

use crate::constants::ERROR_NOT_FOUND;
use crate::constants::ERROR_READING;
use crate::constants::HIDDEN_ENTRY_PREFIX;
use crate::constants::HOME_ENVIRONMENT_VARIABLE;
use crate::constants::IMAGE_EXTENSIONS;
use crate::constants::IMAGE_FILE_COLLECTION_LOCK_POISONED;
use crate::constants::MARKDOWN_EXTENSION;
use crate::constants::MARKDOWN_FILE_COLLECTION_LOCK_POISONED;
use crate::constants::TILDE;
use crate::constants::TILDE_SLASH;
use crate::validated_config::ValidatedConfig;

pub struct RepositoryFiles {
    pub images:   Vec<PathBuf>,
    pub markdown: Vec<PathBuf>,
}

pub fn read_contents_from_file(path: &Path) -> Result<String, Box<dyn Error + Send + Sync>> {
    let contents = fs::read_to_string(path).map_err(|e| -> Box<dyn Error + Send + Sync> {
        if e.kind() == ErrorKind::NotFound {
            Box::new(io::Error::new(
                ErrorKind::NotFound,
                format!("{ERROR_NOT_FOUND}{}", path.display()),
            ))
        } else {
            Box::new(io::Error::new(
                e.kind(),
                format!("{ERROR_READING}'{}': {e}", path.display()),
            ))
        }
    })?;
    Ok(contents)
}

// `expand_tilde` replaces a leading `~/` with the user's home directory.
pub fn expand_tilde<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = path.as_ref();

    if let Some(path_str) = path.to_str()
        && let Some(home) = var_os(HOME_ENVIRONMENT_VARIABLE)
        && let Some(stripped) = path_str.strip_prefix(TILDE_SLASH)
    {
        return PathBuf::from(home).join(stripped);
    }

    // Component::Normal preserves invalid UTF-8 by avoiding Path::to_str.
    let mut components = path.components();
    if let Some(Component::Normal(first)) = components.next()
        && first == TILDE
        && let Some(home) = var_os(HOME_ENVIRONMENT_VARIABLE)
    {
        let mut expanded_path = PathBuf::from(home);
        expanded_path.extend(components);
        return expanded_path;
    }

    // `path` is returned unchanged when neither tilde expansion branch matches.
    path.to_path_buf()
}

pub(crate) fn format_relative_path(path: &Path, base_path: &Path) -> String {
    path.strip_prefix(base_path)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

// `rayon` via `.into_par_iter()` keeps `collect_repository_files` at about 4ms
// instead of the 12ms measured with `walkdir`.
pub fn collect_repository_files(
    validated_config: &ValidatedConfig,
    ignore_folders: &[PathBuf],
) -> Result<RepositoryFiles, Box<dyn Error + Send + Sync>> {
    fn is_ignored(path: &Path, ignore_folders: &[PathBuf]) -> bool {
        ignore_folders
            .iter()
            .any(|ignored| path.starts_with(ignored))
    }

    // Obsidian never indexes a file or folder whose name starts with a dot
    // (`.obsidian`, `.trash`, `.git`, `.DS_Store`), so a wikilink into one cannot
    // resolve in the app; scanning them would only report on notes the vault
    // cannot see.
    fn is_hidden(path: &Path) -> bool {
        path.file_name().is_some_and(|name| {
            name.as_encoded_bytes()
                .starts_with(HIDDEN_ENTRY_PREFIX.as_bytes())
        })
    }

    fn visit_dirs(
        dirs: Vec<PathBuf>,
        ignore_folders: &[PathBuf],
        markdown_files: &Mutex<Vec<PathBuf>>,
        image_files: &Mutex<Vec<PathBuf>>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        dirs.into_par_iter().try_for_each(|dir| {
            if is_ignored(&dir, ignore_folders) {
                return Ok(());
            }

            let mut subdirs = Vec::new();

            for path in fs::read_dir(&dir)?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
            {
                if is_hidden(&path) {
                    continue;
                }

                if let Some(ext) = path
                    .extension()
                    .and_then(OsStr::to_str)
                    .map(str::to_lowercase)
                {
                    if ext == MARKDOWN_EXTENSION {
                        markdown_files
                            .lock()
                            .map_err(|error| {
                                format!("{MARKDOWN_FILE_COLLECTION_LOCK_POISONED}: {error}")
                            })?
                            .push(path.clone());
                    } else if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                        image_files
                            .lock()
                            .map_err(|error| {
                                format!("{IMAGE_FILE_COLLECTION_LOCK_POISONED}: {error}")
                            })?
                            .push(path.clone());
                    }
                }

                if path.is_dir() {
                    subdirs.push(path);
                }
            }

            if !subdirs.is_empty() {
                visit_dirs(subdirs, ignore_folders, markdown_files, image_files)?;
            }
            Ok(())
        })
    }

    let markdown_files = Mutex::new(Vec::new());
    let image_files = Mutex::new(Vec::new());

    visit_dirs(
        vec![validated_config.obsidian_path().to_path_buf()],
        ignore_folders,
        &markdown_files,
        &image_files,
    )?;

    Ok(RepositoryFiles {
        markdown: markdown_files
            .into_inner()
            .map_err(|error| format!("{MARKDOWN_FILE_COLLECTION_LOCK_POISONED}: {error}"))?,
        images:   image_files
            .into_inner()
            .map_err(|error| format!("{IMAGE_FILE_COLLECTION_LOCK_POISONED}: {error}"))?,
    })
}

#[cfg(test)]
mod expand_tilde_tests {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    use super::*;

    #[test]
    fn test_expand_tilde() {
        // Only run this test if HOME is set
        if let Some(home) = var_os("HOME") {
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
        let bytes = b"~/invalid-\xFF-path";
        let os_str = OsStr::from_bytes(bytes);
        let path = Path::new(os_str);

        let expanded = expand_tilde(path);

        // Since HOME is unlikely to contain invalid bytes, the tilde should be expanded
        if let Some(home) = var_os("HOME") {
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

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod collect_repository_files_tests {
    use tempfile::TempDir;

    use super::*;
    use crate::validated_config::ValidatedConfigBuilder;

    fn create_empty_file(root: &Path, relative: &str) -> PathBuf {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "").unwrap();
        path
    }

    #[test]
    fn test_collect_repository_files_skips_hidden_entries_and_ignored_folders() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let kept_markdown = create_empty_file(root, "notes/kept.md");
        let kept_image = create_empty_file(root, "notes/kept.png");
        create_empty_file(root, ".trash/Untitled.md");
        create_empty_file(root, ".obsidian/plugins/plugin.md");
        create_empty_file(root, "notes/.hidden.md");
        create_empty_file(root, "notes/.DS_Store");
        create_empty_file(root, "templates/skipped.md");
        create_empty_file(root, "output/report.md");

        let mut builder = ValidatedConfigBuilder::default();
        builder.obsidian_path(root.to_path_buf());
        builder.ignore_folders(Some(vec![PathBuf::from("templates")]));
        builder.output_folder(root.join("output"));
        let validated_config = builder.build().unwrap();

        let repository_files =
            collect_repository_files(&validated_config, validated_config.ignore_folders().unwrap())
                .unwrap();

        assert_eq!(repository_files.markdown, vec![kept_markdown]);
        assert_eq!(repository_files.images, vec![kept_image]);
    }
}

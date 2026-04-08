use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::DateTime;
use chrono::Utc;
use filetime::FileTime;
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;

use crate::constants::DS_STORE;
use crate::constants::ERROR_NOT_FOUND;
use crate::constants::ERROR_READING;
use crate::constants::IMAGE_EXTENSIONS;
use crate::constants::MARKDOWN_EXTENSION;
use crate::validated_config::ValidatedConfig;

pub fn read_contents_from_file(path: &Path) -> Result<String, Box<dyn Error + Send + Sync>> {
    let contents = fs::read_to_string(path).map_err(|e| -> Box<dyn Error + Send + Sync> {
        if e.kind() == io::ErrorKind::NotFound {
            Box::new(io::Error::new(
                io::ErrorKind::NotFound,
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

// Expands a path that starts with `~/` to the user's home directory.
pub fn expand_tilde<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = path.as_ref();

    // Handle paths that start with "~/"
    if let Some(path_str) = path.to_str() {
        if let Some(home) = std::env::var_os("HOME")
            && let Some(stripped) = path_str.strip_prefix("~/")
        {
            return PathBuf::from(home).join(stripped);
        }
    } else {
        // Handle invalid UTF-8 paths (`OsStr` -> `PathBuf` without assuming valid UTF-8)
        let mut components = path.components();
        if let Some(std::path::Component::Normal(first)) = components.next()
            && first == "~"
            && let Some(home) = std::env::var_os("HOME")
        {
            let mut expanded_path = PathBuf::from(home);
            expanded_path.extend(components);
            return expanded_path;
        }
    }

    // Return the original path if no tilde expansion was needed
    path.to_path_buf()
}

pub struct RepositoryFiles {
    pub image_files:    Vec<PathBuf>,
    pub markdown_files: Vec<PathBuf>,
}

// using rayon (.into_par_iter()) and not using walkdir
// takes this from 12ms down to 4ms
#[allow(
    clippy::unwrap_used,
    reason = "mutex poisoning is unrecoverable — unwrap is the standard pattern"
)]
pub fn collect_repository_files(
    validated_config: &ValidatedConfig,
    ignore_folders: &[PathBuf],
) -> Result<RepositoryFiles, Box<dyn Error + Send + Sync>> {
    fn is_ignored(path: &Path, ignore_folders: &[PathBuf]) -> bool {
        ignore_folders
            .iter()
            .any(|ignored| path.starts_with(ignored))
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

            let subdirs: Vec<PathBuf> = fs::read_dir(&dir)?
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .filter(|path| path.file_name().and_then(|s| s.to_str()) != Some(DS_STORE))
                .inspect(|path| {
                    if let Some(ext) = path
                        .extension()
                        .and_then(|s| s.to_str())
                        .map(str::to_lowercase)
                    {
                        if ext == MARKDOWN_EXTENSION {
                            markdown_files.lock().unwrap().push(path.clone());
                        } else if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                            image_files.lock().unwrap().push(path.clone());
                        }
                    }
                })
                .filter(|path| path.is_dir())
                .collect();

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
        markdown_files: markdown_files.into_inner().unwrap(),
        image_files:    image_files.into_inner().unwrap(),
    })
}

#[cfg(target_os = "macos")]
pub fn set_file_dates(
    path: &Path,
    created: Option<DateTime<Utc>>,
    modified: DateTime<Utc>,
    operational_timezone: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    filetime::set_file_mtime(path, FileTime::from_system_time(modified.into()))?;

    if let Some(created_date) = created {
        let tz: chrono_tz::Tz = operational_timezone.parse().unwrap_or(chrono_tz::UTC);

        // Convert directly to local time without additional adjustment
        let local_time = created_date.with_timezone(&tz);
        let formatted_date = local_time.format("%m/%d/%Y %H:%M:%S").to_string();

        let output = std::process::Command::new("SetFile")
            .arg("-d")
            .arg(&formatted_date)
            .arg(path)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to set creation date with SetFile: {stderr}").into());
        }
    }

    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn set_file_dates(
    path: &Path,
    created: Option<DateTime<Utc>>,
    modified: DateTime<Utc>,
    _operational_timezone: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(created_date) = created {
        filetime::set_file_times(
            path,
            FileTime::from_system_time(created_date.into()),
            FileTime::from_system_time(modified.into()),
        )?;
    } else {
        filetime::set_file_mtime(path, FileTime::from_system_time(modified.into()))?;
    }
    Ok(())
}

#[cfg(test)]
mod expand_tilde_tests {
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

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod set_file_dates_tests {
    use std::fs::File;

    use chrono::TimeZone;
    use chrono::Utc;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_set_file_dates_with_operational_timezone() {
        // Define test parameters
        let operational_timezone = "America/New_York";
        let tz: chrono_tz::Tz = operational_timezone.parse().unwrap();

        // Define creation and modification dates in UTC
        let created_date_utc = Utc.with_ymd_and_hms(2022, 12, 31, 15, 0, 0).unwrap();
        let modified_date_utc = Utc.with_ymd_and_hms(2023, 1, 1, 12, 0, 0).unwrap();

        // Create a temporary file for testing
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let temp_file_path = temp_dir.path().join("test_file.txt");
        File::create(&temp_file_path).expect("Failed to create test file");

        // Apply `set_file_dates` with the operational timezone
        set_file_dates(
            &temp_file_path,
            Some(created_date_utc),
            modified_date_utc,
            operational_timezone,
        )
        .expect("Failed to set file dates");

        // Retrieve metadata for verification
        let metadata = fs::metadata(&temp_file_path).expect("Failed to retrieve metadata");

        // Verify modification date
        let retrieved_modified: DateTime<Utc> = metadata
            .modified()
            .expect("Failed to retrieve modified date")
            .into();
        let retrieved_modified_in_tz = retrieved_modified.with_timezone(&tz);
        let expected_modified_in_tz = modified_date_utc.with_timezone(&tz);

        assert_eq!(
            retrieved_modified, modified_date_utc,
            "Modified dates do not match in UTC"
        );
        assert_eq!(
            retrieved_modified_in_tz, expected_modified_in_tz,
            "Modified dates do not match in the operational timezone"
        );

        // Verify creation date
        #[cfg(target_os = "macos")]
        {
            let retrieved_created: DateTime<Utc> = metadata
                .created()
                .expect("Failed to retrieve created date")
                .into();
            let retrieved_created_in_tz = retrieved_created.with_timezone(&tz);
            let expected_created_in_tz = created_date_utc.with_timezone(&tz);

            println!("\nCreation Time Verification:");
            println!("Created in UTC: {retrieved_created}");
            println!("Retrieved created in {operational_timezone}: {retrieved_created_in_tz}");
            println!("Expected created in {operational_timezone}: {expected_created_in_tz}");

            assert_eq!(
                retrieved_created, created_date_utc,
                "Created dates do not match in UTC"
            );
            assert_eq!(
                retrieved_created_in_tz, expected_created_in_tz,
                "Created dates do not match in the operational timezone"
            );
        }
    }
}

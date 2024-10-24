use chrono::{Local, NaiveDateTime};
use regex::Regex;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, io};

pub fn update_file<P: AsRef<Path>>(
    path: P,
    update_fn: impl FnOnce(&str) -> String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let path = path.as_ref();
    let content = fs::read_to_string(path)?;
    let updated_content = if path.extension().and_then(|s| s.to_str()) == Some("md") {
        update_markdown_content(&content, update_fn)
    } else {
        update_fn(&content)
    };
    fs::write(path, updated_content)?;
    Ok(())
}

fn update_markdown_content(content: &str, update_fn: impl FnOnce(&str) -> String) -> String {
    let frontmatter_regex = Regex::new(r"(?s)^---\n(.*?)\n---").unwrap();
    let date_modified_regex = Regex::new(r"(?m)^date_modified:\s*(.*)$").unwrap();

    let today = Local::now().format("[[%Y-%m-%d]]").to_string();

    let updated_content = if let Some(captures) = frontmatter_regex.captures(content) {
        let frontmatter = captures.get(1).unwrap().as_str();
        let updated_frontmatter = if date_modified_regex.is_match(frontmatter) {
            date_modified_regex
                .replace(frontmatter, |_: &regex::Captures| {
                    format!("date_modified: \"{}\"", today)
                })
                .to_string()
        } else {
            format!("{}\ndate_modified: \"{}\"", frontmatter.trim(), today)
        };

        frontmatter_regex
            .replace(content, |_: &regex::Captures| {
                format!("---\n{}\n---", updated_frontmatter.trim())
            })
            .to_string()
    } else {
        format!(
            "---\ndate_modified: \"{}\"\n---\n{}",
            today,
            content.trim_start()
        )
    };

    update_fn(&updated_content)
}

/// Expands a path that starts with `~/` to the user's home directory.
///
/// # Arguments
///
/// * `path` - A path that may start with `~/`.
///
/// # Returns
///
/// * `PathBuf` with the expanded path.
pub fn expand_tilde<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = path.as_ref();

    // Handle paths that start with "~/"
    if let Some(path_str) = path.to_str() {
        if path_str.starts_with("~/") {
            if let Some(home) = std::env::var_os("HOME") {
                return PathBuf::from(home).join(&path_str[2..]);
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

pub fn set_file_create_date(file_path: &Path, creation_date: NaiveDateTime) -> io::Result<()> {
    // Format the date with hh:mm:ss included
    let formatted_date = creation_date.format("%m/%d/%Y %H:%M:%S").to_string();

    println!(
        "formatted_date to set: {:?} - not enabled yet",
        formatted_date
    );

    Command::new("SetFile")
        .arg("-d")
        .arg(&formatted_date)
        .arg(file_path.to_str().unwrap())
        .status()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    /// Use `GetFileInfo` to verify creation time on macOS
    fn get_creation_time(file_path: &Path) -> io::Result<NaiveDateTime> {
        let output = Command::new("GetFileInfo")
            .arg("-d")
            .arg(file_path.to_str().unwrap())
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        // Expected format: MM/DD/YYYY HH:MM:SS
        let date_time_str = output_str.trim();
        NaiveDateTime::parse_from_str(date_time_str, "%m/%d/%Y %H:%M:%S").map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to parse date: {}", e),
            )
        })
    }

    #[test]
    fn test_update_markdown_content() {
        let today = Local::now().format("[[%Y-%m-%d]]").to_string();

        // Test case 1: Existing frontmatter with date_modified
        let content1 = "---\ntitle: Test\ndate_modified: \"[[2023-01-01]]\"\n---\nContent";
        let expected1 = format!(
            "---\ntitle: Test\ndate_modified: \"{}\"\n---\nContent",
            today
        );
        assert_eq!(
            update_markdown_content(content1, |s| s.to_string()),
            expected1
        );

        // Test case 2: Existing frontmatter without date_modified
        let content2 = "---\ntitle: Test\n---\nContent";
        let expected2 = format!(
            "---\ntitle: Test\ndate_modified: \"{}\"\n---\nContent",
            today
        );
        assert_eq!(
            update_markdown_content(content2, |s| s.to_string()),
            expected2
        );

        // Test case 3: No frontmatter
        let content3 = "Content without frontmatter";
        let expected3 = format!(
            "---\ndate_modified: \"{}\"\n---\nContent without frontmatter",
            today
        );
        assert_eq!(
            update_markdown_content(content3, |s| s.to_string()),
            expected3
        );
    }

    #[test]
    fn test_update_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        let content = "---\ntitle: Test\n---\nContent";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        update_file(file_path, |s| s.replace("Content", "Updated Content")).unwrap();

        let updated_content = fs::read_to_string(temp_dir.path().join("test.md")).unwrap();
        let today = Local::now().format("[[%Y-%m-%d]]").to_string();
        assert!(updated_content.contains(&format!("date_modified: \"{}\"", today)));
        assert!(updated_content.contains("Updated Content"));
    }

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

    #[test]
    fn test_set_file_times_with_full_datetime() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_file.txt");

        // Create a temporary file
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "Temporary content").unwrap();

        // Arbitrary date and time for testing
        let test_date_time =
            NaiveDateTime::parse_from_str("2023-10-24 15:45:30", "%Y-%m-%d %H:%M:%S").unwrap();

        // Set the creation time of the file to the specified date and time
        set_file_create_date(&file_path, test_date_time).unwrap();

        // Verify the creation time using GetFileInfo
        let creation_time = get_creation_time(&file_path).unwrap();

        // Assert the expected and actual creation times
        assert_eq!(
            creation_time, test_date_time,
            "The file's creation time was not set correctly"
        );
    }

    #[test]
    fn test_set_file_times_with_edge_case_times() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edge_case_file.txt");

        // Create the file
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "Edge case test").unwrap();

        // Test with midnight time
        let midnight_time =
            NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        set_file_create_date(&file_path, midnight_time).unwrap();

        // Verify the creation date was set correctly to midnight
        let creation_time = get_creation_time(&file_path).unwrap();
        assert_eq!(
            creation_time, midnight_time,
            "The midnight creation time was not set correctly"
        );

        // Test with a random time of day
        let random_time =
            NaiveDateTime::parse_from_str("2024-06-15 13:22:11", "%Y-%m-%d %H:%M:%S").unwrap();
        set_file_create_date(&file_path, random_time).unwrap();

        let creation_time = get_creation_time(&file_path).unwrap();
        assert_eq!(
            creation_time, random_time,
            "The random time of day was not set correctly"
        );
    }
}

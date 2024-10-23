use std::cmp::Ordering;
use crate::scan::CollectedFiles;
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::validated_config::ValidatedConfig;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use filetime::FileTime;
use regex::Regex;
use std::error::Error;
use std::fs::{self, metadata};
use std::path::PathBuf;

pub struct CreationDateInfo {
    file_path: PathBuf,
    current_date: DateTime<Local>,
    new_date: DateTime<Local>,
}

pub struct InvalidDateInfo {
    file_path: PathBuf,
    invalid_value: String,
}

struct DateUpdateResults {
    valid_dates: Vec<CreationDateInfo>,
    invalid_dates: Vec<InvalidDateInfo>,
}

// Define a trait to generalize access to file_path
trait HasFilePath {
    fn file_path(&self) -> &PathBuf;
}

// Implement the trait for both structs
impl HasFilePath for CreationDateInfo {
    fn file_path(&self) -> &PathBuf {
        &self.file_path
    }
}

impl HasFilePath for InvalidDateInfo {
    fn file_path(&self) -> &PathBuf {
        &self.file_path
    }
}

// Define a sorting function that uses the trait
fn sort_by_file_name<T: HasFilePath>(a: &T, b: &T) -> Ordering {
    a.file_path()
        .file_name()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
        .to_lowercase()
        .cmp(
            &b.file_path()
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default()
                .to_lowercase(),
        )
}

pub fn update_creation_dates(
    config: &ValidatedConfig,
    collected_files: &CollectedFiles,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("#", "creation date update")?;

    let Some(date_property) = config.creation_date_property() else {
        writer.writeln("", "no creation_date_property specified in config")?;
        return Ok(());
    };

    let date_updates = collect_creation_dates(collected_files, date_property)?;

    if date_updates.valid_dates.is_empty() && date_updates.invalid_dates.is_empty() {
        writer.writeln("", "no files found with matching date property")?;
        return Ok(());
    }

    write_date_update_counts(writer, &date_updates)?;

    if !date_updates.invalid_dates.is_empty() {
        write_invalid_dates_table(writer, &date_updates.invalid_dates)?;
    }

    if !date_updates.valid_dates.is_empty() {
        write_creation_dates_table(writer, &date_updates.valid_dates)?;
    }

    Ok(())
}

fn collect_creation_dates(
    collected_files: &CollectedFiles,
    date_property: &str,
) -> Result<DateUpdateResults, Box<dyn Error + Send + Sync>> {
    let mut valid_dates = Vec::new();
    let mut invalid_dates = Vec::new();
    let frontmatter_regex = Regex::new(r"(?s)^---\n(.*?)\n---")?;
    let date_regex = Regex::new(&format!(
        r"(?m)^{}\s*:\s*(.+)$",
        regex::escape(date_property)
    ))?;
    let wikilink_date_regex = Regex::new(r#""?\[\[(\d{4}-\d{2}-\d{2})]]"?"#)?;

    for (file_path, _) in &collected_files.markdown_files {
        let content = fs::read_to_string(file_path)?;

        if let Some(captures) = frontmatter_regex.captures(&content) {
            let frontmatter = captures.get(1).unwrap().as_str();
            if let Some(date_capture) = date_regex.captures(frontmatter) {
                if let Some(date_str) = date_capture.get(1) {
                    let date_str = date_str.as_str().trim();

                    let metadata = metadata(file_path)?;
                    let creation_time = FileTime::from_creation_time(&metadata)
                        .ok_or("Could not get file creation time")?;

                    let utc_date = DateTime::from_timestamp(
                        creation_time.seconds(),
                        creation_time.nanoseconds(),
                    )
                        .ok_or("Invalid timestamp")?;
                    let current_date: DateTime<Local> = utc_date.into();

                    // Try to extract date from wikilink format
                    if let Some(wikilink_capture) = wikilink_date_regex.captures(date_str) {
                        let date_str = wikilink_capture.get(1).unwrap().as_str();
                        if let Ok(naive_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                            // Use the time from current_date
                            let new_date = Local.from_local_datetime(
                                &naive_date.and_time(current_date.time())
                            ).earliest().unwrap_or(current_date);

                            valid_dates.push(CreationDateInfo {
                                file_path: file_path.clone(),
                                current_date,
                                new_date,
                            });
                            continue;
                        }
                    }

                    // If we get here, the date format wasn't valid
                    invalid_dates.push(InvalidDateInfo {
                        file_path: file_path.clone(),
                        invalid_value: date_str.to_string(),
                    });
                }
            }
        }
    }

    valid_dates.sort_by(&sort_by_file_name);
    invalid_dates.sort_by(&sort_by_file_name);

    Ok(DateUpdateResults {
        valid_dates,
        invalid_dates,
    })
}

fn write_date_update_counts(
    writer: &ThreadSafeWriter,
    results: &DateUpdateResults,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("", &format!("found {} files with invalid dates", results.invalid_dates.len()))?;
    writer.writeln("", &format!("found {} files with valid dates", results.valid_dates.len()))?;
    writer.writeln("", "")?; // Empty line before tables
    Ok(())
}

fn write_invalid_dates_table(
    writer: &ThreadSafeWriter,
    invalid_dates: &[InvalidDateInfo],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("##", "invalid date values")?;
    writer.writeln("", &format!("found {} files with invalid date values", invalid_dates.len()))?;
    writer.writeln("", "")?;

    let headers = &["file", "invalid value"];

    let rows: Vec<Vec<String>> = invalid_dates
        .iter()
        .map(|info| {
            vec![
                format!(
                    "[[{}]]",
                    info.file_path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                ),
                info.invalid_value.clone(),
            ]
        })
        .collect();

    writer.write_markdown_table(
        headers,
        &rows,
        Some(&[ColumnAlignment::Left, ColumnAlignment::Left]),
    )?;

    writer.writeln("", "")?;
    Ok(())
}

fn write_creation_dates_table(
    writer: &ThreadSafeWriter,
    date_updates: &[CreationDateInfo],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("##", "valid dates to update")?;
    writer.writeln("", &format!("found {} files with valid dates to update", date_updates.len()))?;
    writer.writeln("", "")?;

    let headers = &["file", "current", "new"];

    let rows: Vec<Vec<String>> = date_updates
        .iter()
        .map(|info| {
            vec![
                format!(
                    "[[{}]]",
                    info.file_path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                ),
                info.current_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                info.new_date.format("%Y-%m-%d %H:%M:%S").to_string(),
            ]
        })
        .collect();

    writer.write_markdown_table(
        headers,
        &rows,
        Some(&[
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]),
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_markdown_file(
        dir: &TempDir,
        filename: &str,
        content: &str,
    ) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
        let file_path = dir.path().join(filename);
        let mut file = File::create(&file_path)?;
        file.write_all(content.as_bytes())?;
        Ok(file_path)
    }

    #[test]
    fn test_collect_creation_dates_valid_wikilink() {
        let temp_dir = TempDir::new().unwrap();

        let content = r#"---
title: Test File
date_onenote: "[[2023-01-01]]"
---
Test content"#;

        let file_path = create_test_markdown_file(&temp_dir, "test.md", content).unwrap();

        let mut markdown_files = HashMap::new();
        markdown_files.insert(file_path, Default::default());
        let collected_files = CollectedFiles {
            markdown_files,
            image_map: HashMap::new(),
            other_files: Vec::new(),
        };

        let date_updates = collect_creation_dates(&collected_files, "date_onenote").unwrap();

        assert_eq!(date_updates.valid_dates.len(), 1);
        assert_eq!(date_updates.invalid_dates.len(), 0);

        // Extract just the date portion for comparison
        let new_date = date_updates.valid_dates[0]
            .new_date
            .format("%Y-%m-%d")
            .to_string();
        assert_eq!(new_date, "2023-01-01");

        // Verify that the time matches the current_date
        assert_eq!(
            date_updates.valid_dates[0].new_date.time(),
            date_updates.valid_dates[0].current_date.time()
        );
    }

    #[test]
    fn test_collect_creation_dates_invalid_format() {
        let temp_dir = TempDir::new().unwrap();

        let content = r#"---
title: Test File
date_onenote: "not a valid date"
---
Test content"#;

        let file_path = create_test_markdown_file(&temp_dir, "test.md", content).unwrap();

        let mut markdown_files = HashMap::new();
        markdown_files.insert(file_path, Default::default());
        let collected_files = CollectedFiles {
            markdown_files,
            image_map: HashMap::new(),
            other_files: Vec::new(),
        };

        let date_updates = collect_creation_dates(&collected_files, "date_onenote").unwrap();

        assert_eq!(date_updates.valid_dates.len(), 0);
        assert_eq!(date_updates.invalid_dates.len(), 1);
        assert_eq!(date_updates.invalid_dates[0].invalid_value, "\"not a valid date\"");
    }

    #[test]
    fn test_collect_creation_dates_mixed_formats() {
        let temp_dir = TempDir::new().unwrap();

        let content1 = r#"---
title: Test File 1
date_onenote: "[[2023-01-01]]"
---
Test content"#;

        let content2 = r#"---
title: Test File 2
date_onenote: invalid_date
---
Test content"#;

        let file_path1 = create_test_markdown_file(&temp_dir, "test1.md", content1).unwrap();
        let file_path2 = create_test_markdown_file(&temp_dir, "test2.md", content2).unwrap();

        let mut markdown_files = HashMap::new();
        markdown_files.insert(file_path1, Default::default());
        markdown_files.insert(file_path2, Default::default());
        let collected_files = CollectedFiles {
            markdown_files,
            image_map: HashMap::new(),
            other_files: Vec::new(),
        };

        let date_updates = collect_creation_dates(&collected_files, "date_onenote").unwrap();

        assert_eq!(date_updates.valid_dates.len(), 1);
        assert_eq!(date_updates.invalid_dates.len(), 1);
    }
}

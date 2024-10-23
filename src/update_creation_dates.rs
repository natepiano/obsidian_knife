use crate::scan::CollectedFiles;
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::validated_config::ValidatedConfig;
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
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

    if date_updates.is_empty() {
        writer.writeln(
            "",
            format!(
                "no files found with matching date property: {:?}",
                date_property
            )
            .as_str(),
        )?;
        return Ok(());
    }

    write_creation_dates_table(writer, &date_updates)?;

    Ok(())
}

fn collect_creation_dates(
    collected_files: &CollectedFiles,
    date_property: &str,
) -> Result<Vec<CreationDateInfo>, Box<dyn Error + Send + Sync>> {
    let mut date_updates = Vec::new();
    let frontmatter_regex = Regex::new(r"(?s)^---\n(.*?)\n---")?;
    let date_regex = Regex::new(&format!(
        r"(?m)^{}\s*:\s*(.+)$",
        regex::escape(date_property)
    ))?;

    for (file_path, _) in &collected_files.markdown_files {
        let content = fs::read_to_string(file_path)?;

        // Extract date from frontmatter if it exists
        if let Some(captures) = frontmatter_regex.captures(&content) {
            let frontmatter = captures.get(1).unwrap().as_str();
            if let Some(date_capture) = date_regex.captures(frontmatter) {
                if let Some(date_str) = date_capture.get(1) {
                    let date_str = date_str.as_str().trim().trim_matches('"');

                    // Get current file creation time
                    let metadata = metadata(file_path)?;
                    let creation_time = FileTime::from_creation_time(&metadata)
                        .ok_or("Could not get file creation time")?;

                    // Convert UTC timestamp to DateTime<Local>
                    let utc_date = DateTime::from_timestamp(
                        creation_time.seconds() as i64,
                        creation_time.nanoseconds() as u32,
                    )
                    .ok_or("Invalid timestamp")?;
                    let current_date: DateTime<Local> = utc_date.into();

                    // Parse the date from frontmatter
                    if let Ok(naive_date) =
                        NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S")
                    {
                        if let Some(new_date) = Local.from_local_datetime(&naive_date).earliest() {
                            date_updates.push(CreationDateInfo {
                                file_path: file_path.clone(),
                                current_date,
                                new_date,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(date_updates)
}

fn write_creation_dates_table(
    writer: &ThreadSafeWriter,
    date_updates: &[CreationDateInfo],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("#", "creation date update")?;

    let headers = &["File", "Current", "New"];

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
    fn test_collect_creation_dates() {
        let temp_dir = TempDir::new().unwrap();

        // Create a test markdown file with date property
        let content = r#"---
title: Test File
date_onenote: 2023-01-01 12:00:00
---
Test content"#;

        let file_path = create_test_markdown_file(&temp_dir, "test.md", content).unwrap();

        // Create test CollectedFiles
        let mut markdown_files = HashMap::new();
        markdown_files.insert(file_path, Default::default());
        let collected_files = CollectedFiles {
            markdown_files,
            image_map: HashMap::new(),
            other_files: Vec::new(),
        };

        // Test the function
        let date_updates = collect_creation_dates(&collected_files, "date_onenote").unwrap();

        assert_eq!(date_updates.len(), 1);
        assert_eq!(
            date_updates[0]
                .new_date
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            "2023-01-01 12:00:00"
        );
    }
}

use crate::scan::MarkdownFileInfo;
use crate::simplify_wikilinks::format_wikilink;
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use chrono::{DateTime, Local, NaiveDate};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

struct DateInfo {
    created_timestamp: DateTime<Local>,
    date_created: Option<String>,
    date_modified: Option<String>,
    date_created_fix: Option<String>,
}

struct DateValidationError {
    date_created: Option<String>,
    date_created_error: Option<String>,
    date_modified: Option<String>,
    date_modified_error: Option<String>,
    date_created_fix: Option<String>,
    date_created_fix_error: Option<String>,
}

// Define a trait for types that can be sorted by filename
trait SortByFilename {
    fn sort_by_filename(&mut self);
}

fn get_filename_lowercase(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase()
}

// Single generic implementation for both types
impl<T> SortByFilename for Vec<(PathBuf, T)> {
    fn sort_by_filename(&mut self) {
        self.sort_by(|(a_path, _), (b_path, _)| {
            get_filename_lowercase(a_path).cmp(&get_filename_lowercase(b_path))
        });
    }
}

pub fn process_dates(
    collected_files: &HashMap<PathBuf, MarkdownFileInfo>,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("#", "update dates")?;

    let (valid_entries, invalid_entries, property_errors) = collect_date_entries(collected_files)?;

    if property_errors.is_empty() && valid_entries.is_empty() && invalid_entries.is_empty() {
        writer.writeln("", "no create dates fixes found")?;
        return Ok(());
    }

    // Output property errors first
    if !property_errors.is_empty() {
        write_property_errors_table(&property_errors, writer)?;
    }

    // Output invalid dates table
    if !invalid_entries.is_empty() {
        write_invalid_dates_table(&invalid_entries, writer)?;
    }

    // Output valid dates table
    if !valid_entries.is_empty() {
        write_valid_dates_table(&valid_entries, writer)?;
    }

    Ok(())
}

fn validate_date_field(
    date: Option<&String>,
    check_wikilink: bool,
    allow_missing: bool,
) -> Option<String> {
    match date {
        Some(d) if d.is_empty() && allow_missing => Some("missing date".to_string()), // Check for missing date
        Some(d) => {
            if check_wikilink {
                let error_msg = build_error_message(d);
                if let Some(msg) = error_msg {
                    return Some(msg);
                }
            } else {
                let (_, is_valid_date) = validate_wikilink_and_date(d);
                if !is_valid_date {
                    return Some("invalid date".to_string());
                }
            }
            None
        }
        None if allow_missing => Some("missing date".to_string()), // Date is None and allow missing check
        None => None, // Date is None and allow missing date is false
    }
}

fn collect_date_entries(
    collected_files: &HashMap<PathBuf, MarkdownFileInfo>,
) -> Result<
    (
        Vec<(PathBuf, DateInfo)>,
        Vec<(PathBuf, DateValidationError)>,
        Vec<(PathBuf, String)>,
    ),
    Box<dyn Error + Send + Sync>,
> {
    let mut valid_entries = Vec::new();
    let mut invalid_entries = Vec::new();
    let mut property_errors = Vec::new();

    for (path, file_info) in collected_files {
        // Collect property_error if exists
        if let Some(error) = &file_info.property_error {
            property_errors.push((path.clone(), error.clone()));
        }

        if let Some(props) = &file_info.properties {
            let mut validation_error = DateValidationError {
                date_created: props.date_created.clone(),
                date_created_error: None,
                date_modified: props.date_modified.clone(),
                date_modified_error: None,
                date_created_fix: props.date_created_fix.clone(),
                date_created_fix_error: None,
            };

            let mut has_error = false;

            // Validate date_created
            if let Some(error) = validate_date_field(props.date_created.as_ref(), true, true) {
                validation_error.date_created_error = Some(error);
                has_error = true;
            }

            // Validate date_modified
            if let Some(error) = validate_date_field(props.date_modified.as_ref(), true, true) {
                validation_error.date_modified_error = Some(error);
                has_error = true;
            }

            // Validate date_created_fix but only output invalid date errors
            if let Some(error) = validate_date_field(props.date_created_fix.as_ref(), false, false)
            {
                validation_error.date_created_fix_error = Some(error);
                has_error = true;
            }

            if has_error {
                invalid_entries.push((path.clone(), validation_error));
            }

            // Files with a valid date_created_fix should be added to valid_entries
            if let Some(date_fix) = &props.date_created_fix {
                if validate_wikilink_and_date(date_fix).1 {
                    // Check if it's a valid date
                    if let Ok(created) = get_file_creation_time(path) {
                        let date_info = DateInfo {
                            created_timestamp: created,
                            date_created: props.date_created.clone(),
                            date_modified: props.date_modified.clone(),
                            date_created_fix: props.date_created_fix.clone(),
                        };
                        valid_entries.push((path.clone(), date_info));
                    }
                }
            }
        }
    }

    // Sort entries
    valid_entries.sort_by_filename();
    invalid_entries.sort_by_filename();
    property_errors.sort_by_filename();

    Ok((valid_entries, invalid_entries, property_errors))
}

fn build_error_message(date: &str) -> Option<String> {
    let (is_wikilink, is_valid_date) = validate_wikilink_and_date(date);

    let mut error_msg = String::new();

    if !is_wikilink {
        error_msg.push_str("invalid wikilink");
    }

    if !is_valid_date {
        if !error_msg.is_empty() {
            error_msg.push_str(", ");
        }
        error_msg.push_str("invalid date");
    }

    if error_msg.is_empty() {
        None
    } else {
        Some(error_msg)
    }
}

fn validate_wikilink_and_date(date: &str) -> (bool, bool) {
    // First, trim any surrounding whitespace from the string
    let date = date.trim();

    // Check if the date starts with exactly `[[` and ends with exactly `]]`
    let is_wikilink = date.starts_with("[[") && date.ends_with("]]");

    // Ensure there are exactly two opening and two closing brackets
    let valid_bracket_count = date.matches('[').count() == 2 && date.matches(']').count() == 2;

    // Combine both checks to ensure it's a proper wikilink
    let is_wikilink = is_wikilink && valid_bracket_count;

    // If it's a wikilink, validate the inner date; otherwise, validate the raw string
    let clean_date = if is_wikilink {
        date.trim_start_matches("[[").trim_end_matches("]]").trim()
    } else {
        date.trim()
    };

    // Validate if the inner content is a valid date in YYYY-MM-DD format
    let is_valid_date = NaiveDate::parse_from_str(clean_date, "%Y-%m-%d").is_ok();

    (is_wikilink, is_valid_date)
}

fn format_date_created_fix(date: &str, timestamp: &DateTime<Local>) -> String {
    let clean_date = date.trim_start_matches("[[").trim_end_matches("]]").trim();

    format!("[[{}]] {}", clean_date, timestamp.format("%H:%M:%S"))
}

fn get_file_creation_time(path: &Path) -> Result<DateTime<Local>, Box<dyn Error + Send + Sync>> {
    let metadata = fs::metadata(path)?;
    let created = metadata.created()?;
    Ok(DateTime::from(created))
}

fn write_invalid_dates_table(
    entries: &[(PathBuf, DateValidationError)],
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("##", "invalid date values")?;
    writer.writeln("", &format!("{} files have invalid dates\n", entries.len()))?;

    let headers = &[
        "file",
        "date_created",
        "error",
        "date_modified",
        "error",
        "date_created_fix",
        "error",
    ];

    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|(path, error)| {
            vec![
                format_wikilink(path),
                error.date_created.clone().unwrap_or_default(),
                error.date_created_error.clone().unwrap_or_default(),
                error.date_modified.clone().unwrap_or_default(),
                error.date_modified_error.clone().unwrap_or_default(),
                error.date_created_fix.clone().unwrap_or_default(),
                error.date_created_fix_error.clone().unwrap_or_default(),
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
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]),
    )?;

    Ok(())
}

fn write_valid_dates_table(
    entries: &[(PathBuf, DateInfo)],
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("##", "update create dates to value in date_created_fix")?;
    writer.writeln("", &format!("{} files will be updated\n", entries.len()))?;

    let headers = &[
        "file",
        "created",
        "date_created",
        "date_modified",
        "date_created_fix",
    ];
    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|(path, info)| {
            vec![
                format_wikilink(path),
                info.created_timestamp
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
                info.date_created
                    .as_ref()
                    .map(|d| d.clone())
                    .unwrap_or_default(),
                info.date_modified
                    .as_ref()
                    .map(|d| d.clone())
                    .unwrap_or_default(),
                format_date_created_fix(
                    &info
                        .date_created_fix
                        .as_ref()
                        .map(|d| d.clone())
                        .unwrap_or_default(),
                    &info.created_timestamp,
                ),
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
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]),
    )?;

    Ok(())
}

fn write_property_errors_table(
    entries: &[(PathBuf, String)],
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("##", "Property Errors")?;
    writer.writeln(
        "",
        &format!("{} files have property errors\n", entries.len()),
    )?;

    let headers = &["file", "error"];
    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|(path, error)| {
            vec![
                format_wikilink(path), // Format the file path as a wiki link
                error.clone(),         // Error message
            ]
        })
        .collect();

    writer.write_markdown_table(
        headers,
        &rows,
        Some(&[ColumnAlignment::Left, ColumnAlignment::Left]),
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::Properties;
    use crate::yaml_utils::deserialize_yaml_frontmatter;
    use chrono::Timelike;

    #[test]
    fn test_yaml_parsing() {
        let yaml = r#"---
date_created: "[[2023-10-23]]"
date_modified: "[[2023-10-23]]"
date_created_fix: "[[2023-10-23]]"
---"#;

        let props: Properties = deserialize_yaml_frontmatter(yaml).unwrap();

        assert_eq!(
            props.date_created.as_ref().map(|d| d.as_str()),
            Some("[[2023-10-23]]")
        );
        assert_eq!(
            props.date_modified.as_ref().map(|d| d.as_str()),
            Some("[[2023-10-23]]")
        );
        assert_eq!(
            props.date_created_fix.as_ref().map(|d| d.as_str()),
            Some("[[2023-10-23]]")
        );
    }

    #[test]
    fn test_format_date_created_fix() {
        // Create a timestamp at a specific local time
        let local_time = Local::now()
            .with_hour(8)
            .unwrap()
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();

        assert_eq!(
            format_date_created_fix("[[2023-01-01]]", &local_time),
            "[[2023-01-01]] 08:00:00"
        );
        assert_eq!(
            format_date_created_fix("2023-01-01", &local_time),
            "[[2023-01-01]] 08:00:00"
        );
    }
}

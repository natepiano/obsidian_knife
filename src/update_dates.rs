use crate::scan::{MarkdownFileInfo, Properties};
use crate::simplify_wikilinks::format_wikilink;
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use chrono::{DateTime, Local, NaiveDate};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use crate::ValidatedConfig;

#[derive(Debug)]
enum DateUpdateAction {
    UpdateDateCreated {
        new_value: String,
        file_creation_time: Option<DateTime<Local>>,
    },
    UpdateDateModified(String),
    NoAction,
}

impl DateUpdateAction {
    fn to_string(&self) -> String {
        match self {
            DateUpdateAction::UpdateDateCreated { file_creation_time, .. } => {
                let mut actions = vec!["date_created updated"];
                if file_creation_time.is_some() {
                    actions.push("file creation date updated");
                }
                actions.join(", ")
            }
            DateUpdateAction::UpdateDateModified(_) => "date_modified updated".to_string(),
            DateUpdateAction::NoAction => "no action".to_string(),
        }
    }
}

// these are all the resulting tables we could write
#[derive(Debug)]
struct DateValidationResults {
    invalid_entries: Vec<(PathBuf, DateValidationError)>,
    date_created_entries: Vec<(PathBuf, DateInfo, SetFileCreationDateWith)>,
    date_modified_entries: Vec<(PathBuf, String, String)>,
    property_errors: Vec<(PathBuf, String)>,
}

impl DateValidationResults {
    pub fn new() -> Self {
        DateValidationResults {
            invalid_entries: Vec::new(),
            date_created_entries: Vec::new(),
            date_modified_entries: Vec::new(),
            property_errors: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.property_errors.is_empty()
            && self.invalid_entries.is_empty()
            && self.date_created_entries.is_empty()
            && self.date_modified_entries.is_empty()
    }

    pub fn write_tables(
        &self,
        writer: &ThreadSafeWriter,
        show_updates: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if self.is_empty() {
            writer.writeln("", "no date issues found.")?;
            return Ok(());
        }

        if !self.property_errors.is_empty() {
            write_property_errors_table(&self.property_errors, writer)?;
        }

        if !self.invalid_entries.is_empty() {
            write_invalid_dates_table(&self.invalid_entries, writer)?;
        }

        if !self.date_modified_entries.is_empty() {
            write_date_modified_table(&self.date_modified_entries, writer, show_updates)?;
        }

        if !self.date_created_entries.is_empty() {
            write_date_created_table(&self.date_created_entries, writer, show_updates)?;
        }

        Ok(())
    }
}

#[derive(Debug)]
struct FileValidationContext {
    path: PathBuf,
    created_time: DateTime<Local>,
    properties: Option<Properties>,
    property_error: Option<String>,
}
#[derive(Clone, Debug)]
struct DateInfo {
    created_timestamp: DateTime<Local>,
    date_created: Option<String>,
    date_modified: Option<String>,
    date_created_fix: Option<String>,
    updated_property: DateCreatedPropertyUpdate, // New field
}

#[derive(Debug)]
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

// Update SortByFilename trait to handle Vecs with 3 elements in tuples generically
impl<T1, T2> SortByFilename for Vec<(PathBuf, T1, T2)> {
    fn sort_by_filename(&mut self) {
        self.sort_by(|(a_path, _, _), (b_path, _, _)| {
            get_filename_lowercase(a_path).cmp(&get_filename_lowercase(b_path))
        });
    }
}

// Define the enum for final set value
#[derive(Debug, PartialEq)]
enum SetFileCreationDateWith {
    FileCreationTime,                 // Set date_created to file creation time if missing
    CreatedFixWithTimestamp,          // Concatenate date_created_fix with file creation timestamp
    NoChange,                         // No final value, used for invalid entries
}


// Implement a function to convert the enum into the corresponding string value
impl SetFileCreationDateWith {
    fn to_string(&self, timestamp: DateTime<Local>, date_created_fix: Option<&String>) -> String {
        match self {
            SetFileCreationDateWith::FileCreationTime => {
                // Use the file creation timestamp without wikilinks
                timestamp.format("%Y-%m-%d %H:%M:%S").to_string()
            }
            SetFileCreationDateWith::CreatedFixWithTimestamp => {
                // Concatenate date_created_fix with the file creation timestamp
                // Strip wikilinks from date_created_fix if present
                if let Some(fix) = date_created_fix {
                    let clean_fix = if fix.starts_with("[[") && fix.ends_with("]]") {
                        &fix[2..fix.len()-2]
                    } else {
                        fix
                    };
                    format!("{} {}", clean_fix, timestamp.format("%H:%M:%S"))
                } else {
                    String::new()
                }
            }
            SetFileCreationDateWith::NoChange => {
                "no change".to_string() // Changed from empty string to "no change"
            }
        }
    }
}

#[derive(Clone, Debug)]
enum DateCreatedPropertyUpdate {
    UseDateCreatedFixProperty(String),      // 1. date_created_fix is not empty
    UseDateCreatedProperty(String),         // 2. date_created is present but missing wikilink
    UseFileCreationDate(String),    // 3. date_created is missing and date_created_fix is empty
    NoChange,                    // 4. No change needed
}


// Update process_dates to use the new implementation
pub fn process_dates(
    config: &ValidatedConfig,
    collected_files: &HashMap<PathBuf, MarkdownFileInfo>,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("#", "update dates")?;

    let results = collect_date_entries(collected_files)?;
    results.write_tables(writer, config.apply_changes())?;

    Ok(())
}

fn collect_date_entries(
    collected_files: &HashMap<PathBuf, MarkdownFileInfo>,
) -> Result<DateValidationResults, Box<dyn Error + Send + Sync>> {
    let mut results = DateValidationResults::new();

    for (path, file_info) in collected_files {
        let context = create_validation_context(path, file_info)?;
        process_file(&mut results, context);
    }

    // Sort all results by filename
    results.invalid_entries.sort_by_filename();
    results.date_created_entries.sort_by_filename();
    results.date_modified_entries.sort_by_filename();
    results.property_errors.sort_by_filename();

    Ok(results)
}

fn create_validation_context(
    path: &PathBuf,
    file_info: &MarkdownFileInfo,
) -> Result<FileValidationContext, Box<dyn Error + Send + Sync>> {
    Ok(FileValidationContext {
        path: path.clone(),
        created_time: get_file_creation_time(path)
            .unwrap_or_else(|_| Local::now()),
        properties: file_info.properties.clone(),
        property_error: file_info.property_error.clone(),
    })
}

fn process_file(results: &mut DateValidationResults, context: FileValidationContext) {
    // Handle property errors first
    if let Some(error) = context.property_error {
        results.property_errors.push((context.path, error));
        return;
    }

    // Early return if no properties
    let props = match &context.properties {
        Some(props) => props,
        None => return,
    };

    let validation = validate_date_fields(props);
    let has_invalid_dates = validation.has_invalid_dates();

    // Process invalid dates separately from invalid wikilinks
    if has_invalid_dates {
        results.invalid_entries.push((
            context.path,
            validation.create_validation_error(props),
        ));
        return;
    }

    // Process valid dates but possibly invalid wikilinks
    process_valid_dates(results, &context, props);
}


struct DateFieldValidation {
    date_created_error: Option<String>,
    date_modified_error: Option<String>,
    date_created_fix_error: Option<String>,
}

// Update DateFieldValidation to distinguish between invalid dates and invalid wikilinks
impl DateFieldValidation {
    fn has_invalid_dates(&self) -> bool {
        self.has_error_containing("invalid date")
    }

    fn has_error_containing(&self, error_text: &str) -> bool {
        let check_error = |error: &Option<String>| {
            error
                .as_ref()
                .map_or(false, |e| e.contains(error_text))
        };

        check_error(&self.date_created_error)
            || check_error(&self.date_modified_error)
            || check_error(&self.date_created_fix_error)
    }

    fn create_validation_error(&self, props: &Properties) -> DateValidationError {
        DateValidationError {
            date_created: props.date_created.clone(),
            date_created_error: self.date_created_error.clone(),
            date_modified: props.date_modified.clone(),
            date_modified_error: self.date_modified_error.clone(),
            date_created_fix: props.date_created_fix.clone(),
            date_created_fix_error: self.date_created_fix_error.clone(),
        }
    }
}

fn validate_date_fields(props: &Properties) -> DateFieldValidation {
    DateFieldValidation {
        date_created_error: check_date_validity(props.date_created.as_ref()),
        date_modified_error: check_date_validity(props.date_modified.as_ref()),
        date_created_fix_error: check_date_validity(props.date_created_fix.as_ref()),
    }
}

fn check_date_validity(date: Option<&String>) -> Option<String> {
    if let Some(d) = date {
        let (is_wikilink, is_valid_date) = validate_wikilink_and_date(d);

        if !is_valid_date || !is_wikilink {
            let mut errors = Vec::new();

            if !is_wikilink {
                errors.push("invalid wikilink");
            }

            if !is_valid_date {
                errors.push("invalid date");
            }

            Some(errors.join(", "))
        } else {
            None
        }
    } else {
        None
    }
}

fn process_valid_dates(
    results: &mut DateValidationResults,
    context: &FileValidationContext,
    props: &Properties,
) {
    // First check if we should add this to date_created_entries
    let should_add =
        props.date_created_fix.is_some() ||
            props.date_created.is_none() ||
            props.date_created.as_ref().map_or(false, |d| !is_wikilink(Some(d)));

    if should_add {
        let final_set_value = calculate_final_set_value(
            props.date_created.as_ref(),
            props.date_created_fix.as_ref(),
        );

        let date_info = DateInfo {
            created_timestamp: context.created_time,
            date_created: props.date_created.clone(),
            date_modified: props.date_modified.clone(),
            date_created_fix: props.date_created_fix.clone(),
            updated_property: determine_updated_property(
                props.date_created.as_ref(),
                props.date_created_fix.as_ref(),
                context.created_time,
            ),
        };

        results.date_created_entries.push((
            context.path.clone(),
            date_info,
            final_set_value,
        ));
    }

    // Always check date_modified, including when it's missing
    let today = Local::now().format("[[%Y-%m-%d]]").to_string();

    match &props.date_modified {
        Some(date_modified) => {
            if !is_wikilink(Some(date_modified)) && validate_wikilink_and_date(date_modified).1 {
                let fix = format!("[[{}]]", date_modified);
                results.date_modified_entries.push((
                    context.path.clone(),
                    date_modified.clone(),
                    fix,
                ));
            }
        },
        None => {
            // Handle missing date_modified
            results.date_modified_entries.push((
                context.path.clone(),
                "missing".to_string(),
                today,
            ));
        }
    }
}

fn determine_updated_property(
    date_created: Option<&String>,
    date_created_fix: Option<&String>,
    created_time: DateTime<Local>,
) -> DateCreatedPropertyUpdate {
    match (date_created, date_created_fix) {
        // date_created_fix takes precedence
        (_, Some(fix)) => DateCreatedPropertyUpdate::UseDateCreatedFixProperty(fix.clone()),

        // date_created exists but needs wikilink
        (Some(d), None) if !is_wikilink(Some(d)) =>
            DateCreatedPropertyUpdate::UseDateCreatedProperty(d.clone()),

        // date_created is missing completely
        (None, None) => DateCreatedPropertyUpdate::UseFileCreationDate(
            created_time.format("%Y-%m-%d").to_string(),
        ),

        // No changes needed
        _ => DateCreatedPropertyUpdate::NoChange,
    }
}

/// Helper function to calculate the final set value for date_created
fn calculate_final_set_value(
    date_created: Option<&String>,
    date_created_fix: Option<&String>,
) -> SetFileCreationDateWith {
    match (date_created, date_created_fix) {
        // If date_created_fix exists, use it with timestamp
        (_, Some(_)) => SetFileCreationDateWith::CreatedFixWithTimestamp,
        // If date_created is missing, use file creation time
        (None, None) => SetFileCreationDateWith::FileCreationTime,
        // If date_created exists, no final value needed
        _ => SetFileCreationDateWith::NoChange
    }
}

fn is_wikilink(date: Option<&String>) -> bool {
    if let Some(d) = date {
        d.starts_with("[[") && d.ends_with("]]")
    } else {
        false
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

fn write_date_created_table(
    entries: &[(PathBuf, DateInfo, SetFileCreationDateWith)],
    writer: &ThreadSafeWriter,
    show_updates: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("##", "date created issues and fixes")?;
    writer.writeln("", &format!("{} files have issues with date_created\n", entries.len()))?;

    let mut headers = vec![
        "file",
        "created (from file)",
        "date_created",
        "date_created_fix",
        "updated property",
        "updated file creation date",
    ];

    if show_updates {
        headers.push("update action");
    }

    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|(path, info, final_set_value)| {
            let mut row = vec![
                format_wikilink(path),
                info.created_timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
                info.date_created.clone().unwrap_or_else(|| "missing".to_string()),
                info.date_created_fix.clone().unwrap_or_default(),
                match &info.updated_property {
                    DateCreatedPropertyUpdate::UseDateCreatedFixProperty(fix) => ensure_wikilink_format(fix),
                    DateCreatedPropertyUpdate::UseDateCreatedProperty(d) => ensure_wikilink_format(d),
                    DateCreatedPropertyUpdate::UseFileCreationDate(date) => format!("[[{}]]", date),
                    DateCreatedPropertyUpdate::NoChange => String::from("no change"),
                },
                final_set_value.to_string(info.created_timestamp, info.date_created_fix.as_ref()),
            ];

            if show_updates {
                let action = match &info.updated_property {
                    DateCreatedPropertyUpdate::NoChange => DateUpdateAction::NoAction,
                    _ => {
                        let new_value = match &info.updated_property {
                            DateCreatedPropertyUpdate::UseDateCreatedFixProperty(fix) => ensure_wikilink_format(fix),
                            DateCreatedPropertyUpdate::UseDateCreatedProperty(d) => ensure_wikilink_format(d),
                            DateCreatedPropertyUpdate::UseFileCreationDate(date) => format!("[[{}]]", date),
                            DateCreatedPropertyUpdate::NoChange => unreachable!(),
                        };
                        let file_creation_time = if *final_set_value != SetFileCreationDateWith::NoChange {
                            Some(info.created_timestamp)
                        } else {
                            None
                        };
                        DateUpdateAction::UpdateDateCreated {
                            new_value,
                            file_creation_time,
                        }
                    }
                };
                row.push(action.to_string());
            }

            row
        })
        .collect();

    let mut alignments = vec![ColumnAlignment::Left; 6];
    if show_updates {
        alignments.push(ColumnAlignment::Left);
    }

    writer.write_markdown_table(&headers, &rows, Some(&alignments))?;

    Ok(())
}

// Helper function to ensure a date string has wikilink format
fn ensure_wikilink_format(date: &str) -> String {
    if date.starts_with("[[") && date.ends_with("]]") {
        date.to_string()
    } else {
        format!("[[{}]]", date)
    }
}

// Helper function to strip wikilinks
fn strip_wikilinks(text: &str) -> String {
    if text.starts_with("[[") && text.ends_with("]]") {
        text[2..text.len()-2].to_string()
    } else {
        text.to_string()
    }
}

fn write_date_modified_table(
    entries: &[(PathBuf, String, String)],
    writer: &ThreadSafeWriter,
    show_updates: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("##", "date modified issues and fixes")?;
    writer.writeln(
        "",
        &format!("{} files have issues with date_modified\n", entries.len()),
    )?;

    let mut headers = vec!["file", "date_modified", "updated property"];
    if show_updates {
        headers.push("update action");
    }

    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|(path, date_modified, fix)| {
            let mut row = vec![
                format_wikilink(path),
                date_modified.clone(),
                fix.clone(),
            ];

            if show_updates {
                let action = DateUpdateAction::UpdateDateModified(fix.clone());
                row.push(action.to_string());
            }

            row
        })
        .collect();

    let mut alignments = vec![ColumnAlignment::Left; 3];
    if show_updates {
        alignments.push(ColumnAlignment::Left);
    }

    writer.write_markdown_table(&headers, &rows, Some(&alignments))?;

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
    use crate::scan::Properties;
    use crate::yaml_utils::deserialize_yaml_frontmatter;

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

}

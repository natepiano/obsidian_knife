use crate::constants::*;
use crate::frontmatter::FrontMatter;
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::wikilink::{format_wikilink, is_wikilink};
use crate::{file_utils, ValidatedConfig};

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime};

use crate::markdown_file_info::MarkdownFileInfo;
use crate::yaml_frontmatter::YamlFrontMatter;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct DateUpdates {
    date_created: Option<String>,       // For frontmatter update
    date_modified: Option<String>,      // For frontmatter update
    file_creation_time: Option<String>, // The actual value to use in set_file_times
    remove_date_created_fix: bool,      // Flag to indicate we should remove date_created_fix
}

#[derive(Debug)]
enum DateUpdateAction {
    UpdateDateCreated {
        file_creation_time: Option<DateTime<Local>>,
    },
    UpdateDateModified,
    NoAction,
}

impl DateUpdateAction {
    fn to_string(&self) -> String {
        match self {
            DateUpdateAction::UpdateDateCreated {
                file_creation_time, ..
            } => {
                let mut actions = vec!["date_created updated".to_string()];
                if file_creation_time.is_some() {
                    actions.push("file creation date updated".to_string());
                }
                actions.join(", ")
            }
            DateUpdateAction::UpdateDateModified => "date_modified updated".to_string(),
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
}

#[derive(Clone, Debug)]
struct DateInfo {
    created_timestamp: DateTime<Local>,
    date_created: Option<String>,
    date_created_fix: Option<String>,
    updated_property: DateCreatedPropertyUpdate,
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

// Update SortByFilename trait to handle a Vec with 3 elements in tuples generically
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
    FileCreationTime,        // Set date_created to file creation time if missing
    CreatedFixWithTimestamp, // Concatenate date_created_fix with file creation timestamp
    NoChange,                // No final value, used for invalid entries
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
                    let clean_fix =
                        if fix.starts_with(OPENING_WIKILINK) && fix.ends_with(CLOSING_WIKILINK) {
                            // let clean_fix = if is_wikilink(Some(fix)) {
                            &fix[2..fix.len() - 2]
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
    UseDateCreatedFixProperty(String), // 1. date_created_fix is not empty
    UseDateCreatedProperty(String),    // 2. date_created is present but missing wikilink
    UseFileCreationDate(String),       // 3. date_created is missing and date_created_fix is empty
    NoChange,                          // 4. No change needed
}

// Update process_dates to use the new implementation
pub fn process_dates(
    config: &ValidatedConfig,
    markdown_files: &mut HashMap<PathBuf, MarkdownFileInfo>,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL1, "dates")?;

    let results = collect_date_entries(markdown_files, config)?;
    results.write_tables(writer, config.apply_changes())?;

    Ok(())
}

fn collect_date_entries(
    collected_files: &mut HashMap<PathBuf, MarkdownFileInfo>,
    config: &ValidatedConfig,
) -> Result<DateValidationResults, Box<dyn Error + Send + Sync>> {
    let mut results = DateValidationResults::new();

    // Create a Vec of paths to iterate over to avoid borrow checker issues
    let paths: Vec<PathBuf> = collected_files.keys().cloned().collect();

    for path in paths {
        let context = FileValidationContext {
            path: path.clone(),
            created_time: get_file_creation_time(&path).unwrap_or_else(|_| Local::now()),
        };

        // Get mutable reference to file_info
        if let Some(file_info) = collected_files.get_mut(&path) {
            process_file(&mut results, context, file_info, config)?;
        }
    }

    // Sort all results by filename
    results.invalid_entries.sort_by_filename();
    results.date_created_entries.sort_by_filename();
    results.date_modified_entries.sort_by_filename();
    results.property_errors.sort_by_filename();

    Ok(results)
}

fn process_file(
    results: &mut DateValidationResults,
    context: FileValidationContext,
    file_info: &mut MarkdownFileInfo,
    config: &ValidatedConfig,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Early return if no frontmatter
    if file_info.frontmatter.is_none() {
        // we output frontmatter issues elsewhere
        // todo - just create the dates here so that we now have a frontmatter
        return Ok(());
    }

    let fm = file_info.frontmatter.as_ref().unwrap();
    let validation = validate_date_fields(fm);
    let has_invalid_dates = validation.has_invalid_dates();

    if has_invalid_dates {
        results
            .invalid_entries
            .push((context.path.clone(), validation.create_validation_error(fm)));
        return Ok(());
    }

    process_valid_dates(results, &context, file_info, config)
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
        let check_error =
            |error: &Option<String>| error.as_ref().map_or(false, |e| e.contains(error_text));

        check_error(&self.date_created_error)
            || check_error(&self.date_modified_error)
            || check_error(&self.date_created_fix_error)
    }

    fn create_validation_error(&self, fm: &FrontMatter) -> DateValidationError {
        DateValidationError {
            date_created: fm.date_created().cloned(),
            date_created_error: self.date_created_error.clone(),
            date_modified: fm.date_modified().cloned(),
            date_modified_error: self.date_modified_error.clone(),
            date_created_fix: fm.date_created_fix().cloned(),
            date_created_fix_error: self.date_created_fix_error.clone(),
        }
    }
}

fn validate_date_fields(fm: &FrontMatter) -> DateFieldValidation {
    DateFieldValidation {
        date_created_error: check_date_validity(fm.date_created()),
        date_modified_error: check_date_validity(fm.date_modified()),
        date_created_fix_error: check_date_validity(fm.date_created_fix()),
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
    file_info: &mut MarkdownFileInfo,
    config: &ValidatedConfig,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let fm = match &file_info.frontmatter {
        Some(fm) => fm,
        None => return Ok(()),
    };

    let mut updates = DateUpdates {
        date_created: None,
        date_modified: None,
        file_creation_time: None,
        remove_date_created_fix: false,
    };

    let should_add = fm.date_created_fix().is_some()
        || fm.date_created().is_none()
        || fm.date_created().map_or(false, |d| !is_wikilink(Some(d)));

    if should_add {
        let file_creation_date_approach =
            calculate_file_creation_date_approach(fm.date_created(), fm.date_created_fix());

        let date_info = DateInfo {
            created_timestamp: context.created_time,
            date_created: fm.date_created().cloned(),
            date_created_fix: fm.date_created_fix().cloned(),
            updated_property: determine_updated_property(
                fm.date_created(),
                fm.date_created_fix(),
                context.created_time,
            ),
        };

        // Prepare date_created update if needed
        match &date_info.updated_property {
            DateCreatedPropertyUpdate::UseDateCreatedFixProperty(fix) => {
                updates.date_created = Some(ensure_wikilink_format(fix));
            }
            DateCreatedPropertyUpdate::UseDateCreatedProperty(date) => {
                updates.date_created = Some(ensure_wikilink_format(date));
            }
            DateCreatedPropertyUpdate::UseFileCreationDate(date) => {
                updates.date_created = Some(format!("[[{}]]", date));
            }
            DateCreatedPropertyUpdate::NoChange => {}
        }

        // Set the file_creation_time based on the final_set_value
        updates.file_creation_time = if file_creation_date_approach
            != SetFileCreationDateWith::NoChange
        {
            Some(file_creation_date_approach.to_string(context.created_time, fm.date_created_fix()))
        } else {
            None
        };

        updates.remove_date_created_fix = match file_creation_date_approach {
            SetFileCreationDateWith::CreatedFixWithTimestamp => true,
            SetFileCreationDateWith::FileCreationTime => false,
            SetFileCreationDateWith::NoChange => false,
        };

        results.date_created_entries.push((
            context.path.clone(),
            date_info,
            file_creation_date_approach,
        ));
    }

    // Handle date_modified updates
    let today = Local::now().format("[[%Y-%m-%d]]").to_string();

    match fm.date_modified() {
        Some(date_modified) => {
            if !is_wikilink(Some(date_modified)) && validate_wikilink_and_date(date_modified).1 {
                let fix = format!("[[{}]]", date_modified);
                updates.date_modified = Some(fix.clone());
                results.date_modified_entries.push((
                    context.path.clone(),
                    date_modified.clone(),
                    fix,
                ));
            }
        }
        None => {
            updates.date_modified = Some(today.clone());
            results.date_modified_entries.push((
                context.path.clone(),
                "missing".to_string(),
                today,
            ));
        }
    }

    apply_date_changes(config, &context.path, file_info, &updates)?;

    Ok(())
}

fn apply_date_changes(
    config: &ValidatedConfig,
    path: &Path,
    file_info: &mut MarkdownFileInfo,
    updates: &DateUpdates,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Early return if:
    // 1. Changes are disabled OR
    // 2. We have no actual changes to make
    if !config.apply_changes()
        || !(updates.date_created.is_some()
            || updates.date_modified.is_some()
            || updates.remove_date_created_fix)
    {
        return Ok(());
    }

    // Update the frontmatter directly in MarkdownFileInfo
    if let Some(fm) = &mut file_info.frontmatter {
        if let Some(date_created) = &updates.date_created {
            fm.update_date_created(Some(date_created.clone()));
        }
        if let Some(date_modified) = &updates.date_modified {
            fm.update_date_modified(Some(date_modified.clone()));
        }
        if updates.remove_date_created_fix {
            fm.update_date_created_fix(None);
        }

        // we know something changed so save it
        fm.persist(path)?;
    }

    // After successful frontmatter update, set the file creation time if we have one
    if let Some(time_str) = &updates.file_creation_time {
        if let Ok(parsed_time) = NaiveDateTime::parse_from_str(time_str, "%Y-%m-%d %H:%M:%S") {
            file_utils::set_file_create_date(path, parsed_time)?;
        }
    }

    Ok(())
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
        (Some(d), None) if !is_wikilink(Some(d)) => {
            DateCreatedPropertyUpdate::UseDateCreatedProperty(d.clone())
        }

        // date_created is missing completely
        (None, None) => DateCreatedPropertyUpdate::UseFileCreationDate(
            created_time.format("%Y-%m-%d").to_string(),
        ),

        // No changes needed
        _ => DateCreatedPropertyUpdate::NoChange,
    }
}

/// Helper function to calculate the file creation date approach
fn calculate_file_creation_date_approach(
    date_created: Option<&String>,
    date_created_fix: Option<&String>,
) -> SetFileCreationDateWith {
    match (date_created, date_created_fix) {
        // If date_created_fix exists, use it with timestamp
        (_, Some(_)) => SetFileCreationDateWith::CreatedFixWithTimestamp,
        // If date_created is missing, use file creation time
        (None, None) => SetFileCreationDateWith::FileCreationTime,
        // If date_created exists, no final value needed
        _ => SetFileCreationDateWith::NoChange,
    }
}

fn validate_wikilink_and_date(date: &str) -> (bool, bool) {
    // First, trim any surrounding whitespace from the string
    let date = date.trim();

    // Check if the date starts with exactly `[[` and ends with exactly `]]`
    let is_wikilink = date.starts_with(OPENING_WIKILINK) && date.ends_with(CLOSING_WIKILINK);

    // Ensure there are exactly two opening and two closing brackets
    let valid_bracket_count =
        date.matches('[').count() == 2 && date.matches(CLOSING_BRACKET).count() == 2;

    // Combine both checks to ensure it's a proper wikilink
    let is_wikilink = is_wikilink && valid_bracket_count;

    // If it's a wikilink, validate the inner date; otherwise, validate the raw string
    let clean_date = if is_wikilink {
        date.trim_start_matches(OPENING_WIKILINK)
            .trim_end_matches(CLOSING_WIKILINK)
            .trim()
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
    writer.writeln(LEVEL2, "invalid date values")?;

    writer.writeln_pluralized(entries.len(), Phrase::InvalidDates)?;

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
    writer.writeln(LEVEL2, "date_created fixes")?;
    writer.writeln_pluralized(entries.len(), Phrase::DateCreated)?;

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
                info.created_timestamp
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
                info.date_created
                    .clone()
                    .unwrap_or_else(|| "missing".to_string()),
                info.date_created_fix.clone().unwrap_or_default(),
                match &info.updated_property {
                    DateCreatedPropertyUpdate::UseDateCreatedFixProperty(fix) => {
                        ensure_wikilink_format(fix)
                    }
                    DateCreatedPropertyUpdate::UseDateCreatedProperty(d) => {
                        ensure_wikilink_format(d)
                    }
                    DateCreatedPropertyUpdate::UseFileCreationDate(date) => format!("[[{}]]", date),
                    DateCreatedPropertyUpdate::NoChange => String::from("no change"),
                },
                final_set_value.to_string(info.created_timestamp, info.date_created_fix.as_ref()),
            ];

            if show_updates {
                let action = match &info.updated_property {
                    DateCreatedPropertyUpdate::NoChange => DateUpdateAction::NoAction,
                    _ => {
                        let file_creation_time =
                            if *final_set_value != SetFileCreationDateWith::NoChange {
                                Some(info.created_timestamp)
                            } else {
                                None
                            };
                        DateUpdateAction::UpdateDateCreated { file_creation_time }
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
    if date.starts_with(OPENING_WIKILINK) && date.ends_with(CLOSING_WIKILINK) {
        date.to_string()
    } else {
        format!("[[{}]]", date)
    }
}

fn write_date_modified_table(
    entries: &[(PathBuf, String, String)],
    writer: &ThreadSafeWriter,
    show_updates: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL2, "date modified issues and fixes")?;
    writer.writeln_pluralized(entries.len(), Phrase::DateModified)?;

    let mut headers = vec!["file", "date_modified", "updated property"];
    if show_updates {
        headers.push("update action");
    }

    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|(path, date_modified, fix)| {
            let mut row = vec![format_wikilink(path), date_modified.clone(), fix.clone()];

            if show_updates {
                let action = DateUpdateAction::UpdateDateModified;
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

fn format_error_for_table(error: &str) -> String {
    // Replace newlines and pipes with spaces to keep content in one cell
    let error = error.replace('\n', " ").replace('|', "\\|");

    // If the error contains YAML content, format it more cleanly
    if error.contains("Content:") {
        let parts: Vec<&str> = error.split("Content:").collect();
        let message = parts[0].trim();
        let content = parts.get(1).map(|c| c.trim()).unwrap_or("");

        // Format YAML content as inline
        let formatted_content = content.split_whitespace().collect::<Vec<_>>().join(" ");

        format!("{} — YAML: {}", message, formatted_content)
    } else {
        error.to_string()
    }
}

// Update the property errors table writing function
fn write_property_errors_table(
    entries: &[(PathBuf, String)],
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL2, "yaml frontmatter property errors")?;
    writer.writeln_pluralized(entries.len(), Phrase::PropertyErrors)?;

    let headers = &["file", "error"];
    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|(path, error)| vec![format_wikilink(path), format_error_for_table(error)])
        .collect();

    writer.write_markdown_table(
        headers,
        &rows,
        Some(&[ColumnAlignment::Left, ColumnAlignment::Left]),
    )?;

    Ok(())
}

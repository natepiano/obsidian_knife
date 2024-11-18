#[cfg(test)]
mod date_fix_tests;
#[cfg(test)]
mod persist_frontmatter_tests;

use crate::file_utils::read_contents_from_file;
use crate::frontmatter::FrontMatter;
use crate::wikilink::is_wikilink;
use crate::wikilink_types::InvalidWikilink;
use crate::yaml_frontmatter::{YamlFrontMatter, YamlFrontMatterError};
use crate::{CLOSING_WIKILINK, LEVEL1, OPENING_WIKILINK};

use crate::utils::{ColumnAlignment, ThreadSafeWriter};
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use itertools::Itertools;
use regex::Regex;
use std::error::Error;
use std::path::PathBuf;
use std::{fs, io};

#[derive(Debug, PartialEq)]
pub enum DateValidationStatus {
    Valid,
    Missing,
    InvalidFormat,
    InvalidWikilink,
    FileSystemMismatch,
}

#[derive(Debug, PartialEq)]
pub struct DateValidation {
    pub frontmatter_date: Option<String>,
    pub file_system_date: DateTime<Local>,
    pub status: DateValidationStatus,
}

impl DateValidation {
    pub fn to_issue_string(&self) -> String {
        match self.status {
            DateValidationStatus::Valid => "valid".to_string(),
            DateValidationStatus::Missing => "missing".to_string(),
            DateValidationStatus::InvalidFormat => {
                format_invalid_date("invalid format", &self.frontmatter_date)
            }
            DateValidationStatus::InvalidWikilink => {
                format_invalid_date("invalid wikilink", &self.frontmatter_date)
            }
            DateValidationStatus::FileSystemMismatch => {
                format_invalid_date("mismatch", &self.frontmatter_date)
            }
        }
    }

    pub fn to_action_string(&self) -> Option<String> {
        match self.status {
            DateValidationStatus::Valid => None,
            DateValidationStatus::Missing
            | DateValidationStatus::FileSystemMismatch
            | DateValidationStatus::InvalidFormat
            | DateValidationStatus::InvalidWikilink => Some(format!(
                "\"\\[[{}]]\"",
                self.file_system_date.format("%Y-%m-%d")
            )),
        }
    }
}

fn format_invalid_date(prefix: &str, date_string: &Option<String>) -> String {
    let escaped_date = date_string
        .as_ref()
        .map(|date_str| date_str.replace("\"", "\\\"").replace("[", "\\["))
        .unwrap_or_default();

    format!(
        "{}<br><span style=\"color:red\">{}</span>",
        prefix, escaped_date
    )
}

// In markdown_file_info.rs
#[derive(Debug)]
pub struct DateCreatedFixValidation {
    pub date_string: Option<String>,
    pub fix_date: Option<DateTime<Local>>,
}

impl DateCreatedFixValidation {
    pub fn to_issue_string(&self) -> String {
        match (&self.date_string, &self.fix_date) {
            (None, _) => "".to_string(),
            (Some(_), None) => format_invalid_date("invalid fix", &self.date_string),
            (Some(_), Some(_)) => "valid fix".to_string(),
        }
    }

    pub fn to_action_string(&self) -> Option<String> {
        match (&self.date_string, &self.fix_date) {
            (None, _) => None,
            (Some(_), None) => Some("won't fix".to_string()),
            (Some(_), Some(date)) => Some(format!("\"\\[[{}]]\"", date.format("%Y-%m-%d"))),
        }
    }
}

impl DateCreatedFixValidation {
    fn from_frontmatter(
        frontmatter: &Option<FrontMatter>,
        file_created_date: DateTime<Local>,
    ) -> Self {
        let date_string = frontmatter
            .as_ref()
            .and_then(|fm| fm.date_created_fix().cloned());

        let parsed_date = date_string.as_ref().and_then(|date_str| {
            let date = if is_wikilink(Some(date_str)) {
                extract_date(date_str)
            } else {
                date_str.trim().trim_matches('"')
            };

            NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
                .ok()
                .map(|naive_date| {
                    let time = file_created_date.time();
                    let naive_datetime = naive_date.and_time(time);
                    Local.from_local_datetime(&naive_datetime).unwrap()
                })
        });

        DateCreatedFixValidation {
            date_string,
            fix_date: parsed_date,
        }
    }
}

#[derive(Debug)]
pub struct MarkdownFileInfo {
    pub content: String,
    pub date_created_fix: DateCreatedFixValidation,
    pub date_validation_created: DateValidation,
    pub date_validation_modified: DateValidation,
    pub do_not_back_populate_regexes: Option<Vec<Regex>>,
    pub frontmatter: Option<FrontMatter>,
    pub frontmatter_error: Option<YamlFrontMatterError>,
    pub image_links: Vec<String>,
    pub invalid_wikilinks: Vec<InvalidWikilink>,
    pub path: PathBuf,
}

impl MarkdownFileInfo {
    pub fn new(path: PathBuf) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let content = read_contents_from_file(&path)?;

        let (mut frontmatter, frontmatter_error) = match FrontMatter::from_markdown_str(&content) {
            Ok(fm) => (Some(fm), None),
            Err(error) => (None, Some(error)),
        };

        let (date_validation_created, date_validation_modified) =
            get_date_validations(&frontmatter, &path)?;

        process_date_validations(
            &mut frontmatter,
            &date_validation_created,
            &date_validation_modified,
        );

        let date_created_fix = DateCreatedFixValidation::from_frontmatter(
            &frontmatter,
            date_validation_created.file_system_date,
        );

        if let Some(ref mut fm) = frontmatter {
            if let Some(fix_date) = date_created_fix.fix_date {
                fm.set_date_created(fix_date);
                fm.remove_date_created_fix();
            }
        }

        let do_not_back_populate_regexes = frontmatter
            .as_ref()
            .map(|fm| fm.get_do_not_back_populate_regexes())
            .flatten();

        Ok(MarkdownFileInfo {
            content,
            date_created_fix,
            do_not_back_populate_regexes,
            date_validation_created,
            date_validation_modified,
            frontmatter,
            frontmatter_error,
            invalid_wikilinks: Vec::new(),
            image_links: Vec::new(),
            path,
        })
    }

    // Helper method to add invalid wikilinks
    pub fn add_invalid_wikilinks(&mut self, wikilinks: Vec<InvalidWikilink>) {
        self.invalid_wikilinks.extend(wikilinks);
    }
}

fn get_date_validation_status(
    date_opt: Option<&String>,
    fs_date: &DateTime<Local>,
) -> DateValidationStatus {
    match date_opt {
        None => DateValidationStatus::Missing,
        Some(date_str) => {
            if !is_wikilink(Some(date_str)) {
                DateValidationStatus::InvalidWikilink
            } else {
                let extracted_date = extract_date(date_str);
                if !is_valid_date(extracted_date) {
                    DateValidationStatus::InvalidFormat
                } else if extracted_date != fs_date.format("%Y-%m-%d").to_string() {
                    DateValidationStatus::FileSystemMismatch
                } else {
                    DateValidationStatus::Valid
                }
            }
        }
    }
}

fn get_date_validations(
    frontmatter: &Option<FrontMatter>,
    path: &PathBuf,
) -> Result<(DateValidation, DateValidation), std::io::Error> {
    let metadata = fs::metadata(path)?;

    let dates = [
        (
            frontmatter
                .as_ref()
                .and_then(|fm| fm.date_created().cloned()),
            metadata
                .created()
                .map(|t| t.into())
                .unwrap_or_else(|_| Local::now()),
        ),
        (
            frontmatter
                .as_ref()
                .and_then(|fm| fm.date_modified().cloned()),
            metadata
                .modified()
                .map(|t| t.into())
                .unwrap_or_else(|_| Local::now()),
        ),
    ];

    Ok(dates
        .into_iter()
        .map(|(frontmatter_date, fs_date)| {
            let status = get_date_validation_status(frontmatter_date.as_ref(), &fs_date);
            DateValidation {
                frontmatter_date,
                file_system_date: fs_date,
                status,
            }
        })
        .collect_tuple()
        .unwrap())
}

// Extracts the date string from a possible wikilink format
fn extract_date(date_str: &str) -> &str {
    let date_str = date_str.trim();
    if is_wikilink(Some(date_str)) {
        date_str
            .trim_start_matches(OPENING_WIKILINK)
            .trim_end_matches(CLOSING_WIKILINK)
            .trim()
    } else {
        date_str
    }
}

// Validates if a string is a valid YYYY-MM-DD date
fn is_valid_date(date_str: &str) -> bool {
    NaiveDate::parse_from_str(date_str.trim(), "%Y-%m-%d").is_ok()
}

fn process_date_validations(
    frontmatter: &mut Option<FrontMatter>,
    date_validation_created: &DateValidation,
    date_validation_modified: &DateValidation,
) {
    if let Some(ref mut frontmatter) = frontmatter {
        // Process created date
        match date_validation_created.status {
            DateValidationStatus::Missing
            | DateValidationStatus::FileSystemMismatch
            | DateValidationStatus::InvalidWikilink
            | DateValidationStatus::InvalidFormat => {
                frontmatter.set_date_created(date_validation_created.file_system_date);
            }
            _ => {} // All other cases: do nothing
        }

        // Process modified date
        match date_validation_modified.status {
            DateValidationStatus::Missing
            | DateValidationStatus::FileSystemMismatch
            | DateValidationStatus::InvalidWikilink
            | DateValidationStatus::InvalidFormat => {
                frontmatter.set_date_modified(date_validation_modified.file_system_date);
            }
            _ => {} // All other cases: do nothing
        }
    }
}

pub fn write_date_validation_table(
    writer: &ThreadSafeWriter,
    files: &[MarkdownFileInfo],
) -> io::Result<()> {
    let mut rows: Vec<Vec<String>> = Vec::new();

    for file in files {
        if file.date_validation_created.status != DateValidationStatus::Valid
            || file.date_validation_modified.status != DateValidationStatus::Valid
            || file.date_created_fix.date_string.is_some()
        {
            let file_name = file
                .path
                .file_name()
                .and_then(|f| f.to_str())
                .map(|s| s.trim_end_matches(".md"))
                .unwrap_or_default();

            let wikilink = format!("[[{}]]", file_name);
            let created_status = file.date_validation_created.to_issue_string();
            let modified_status = file.date_validation_modified.to_issue_string();
            let fix_status = file.date_created_fix.to_issue_string();

            // Simplified persistence status
            let persistence_status = match &file.frontmatter {
                Some(fm) => {
                    if fm.needs_persist() {
                        "yes".to_string()
                    } else {
                        "no".to_string()
                    }
                }
                None => "no frontmatter".to_string(),
            };

            // Collect actions...
            let mut actions = Vec::new();
            if let Some(action) = file.date_validation_created.to_action_string() {
                actions.push(format!("date_created: {}", action));
            }
            if let Some(action) = file.date_validation_modified.to_action_string() {
                actions.push(format!("date_modified: {}", action));
            }
            if let Some(action) = file.date_created_fix.to_action_string() {
                actions.push(format!("date_created_fix: {}", action));
            }

            let action_column = actions.join("<br>");

            rows.push(vec![
                wikilink,
                created_status,
                modified_status,
                fix_status,
                persistence_status,
                action_column,
            ]);
        }
    }

    if !rows.is_empty() {
        rows.sort_by(|a, b| a[0].to_lowercase().cmp(&b[0].to_lowercase()));

        writer.writeln(LEVEL1, "date info from markdown file info")?;
        writer.writeln("", "if date is valid, do nothing")?;
        writer.writeln(
            "",
            "if date is missing, invalid format, or invalid wikilink, pull the date from the file",
        )?;
        writer.writeln("", "")?;

        let headers = &[
            "file",
            "date_created",
            "date_modified",
            "date_created_fix",
            "persist",
            "actions",
        ];

        let alignments = &[
            ColumnAlignment::Left,
            ColumnAlignment::Center,
            ColumnAlignment::Center,
            ColumnAlignment::Center,
            ColumnAlignment::Center,
            ColumnAlignment::Left,
        ];

        for (i, chunk) in rows.chunks(500).enumerate() {
            if i > 0 {
                writer.writeln("", "")?;
            }
            writer.write_markdown_table(headers, chunk, Some(alignments))?;
        }
    }

    Ok(())
}

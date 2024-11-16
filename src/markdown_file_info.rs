#[cfg(test)]
mod date_fix_tests;
#[cfg(test)]
mod persist_frontmatter_tests;

use crate::file_utils::read_contents_from_file;
use crate::frontmatter::FrontMatter;
use crate::wikilink::is_wikilink;
use crate::wikilink_types::InvalidWikilink;
use crate::yaml_frontmatter::{YamlFrontMatter, YamlFrontMatterError};
use crate::{CLOSING_WIKILINK, OPENING_WIKILINK};

use chrono::{DateTime, Local, NaiveDate};
use regex::Regex;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use itertools::Itertools;

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
    pub fn to_report_string(&self) -> String {
        match self.status {
            DateValidationStatus::Valid => "valid".to_string(),
            DateValidationStatus::Missing => "missing".to_string(),
            DateValidationStatus::InvalidFormat => format!(
                "invalid date format: '{}'",
                self.frontmatter_date
                    .as_ref()
                    .unwrap_or(&"none".to_string())
            ),
            DateValidationStatus::InvalidWikilink => format!(
                "missing wikilink: '{}'",
                self.frontmatter_date
                    .as_ref()
                    .unwrap_or(&"none".to_string())
            ),
            DateValidationStatus::FileSystemMismatch => format!(
                "modified date mismatch: frontmatter='{}', filesystem='{}'",
                self.frontmatter_date
                    .as_ref()
                    .unwrap_or(&"none".to_string()),
                self.file_system_date.format("%Y-%m-%d")
            ),
        }
    }
}

#[derive(Debug)]
pub struct MarkdownFileInfo {
    pub content: String,
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

        let (frontmatter, frontmatter_error) = match FrontMatter::from_markdown_str(&content) {
            Ok(fm) => (Some(fm), None),
            Err(error) => (None, Some(error)),
        };

        let (date_validation_created, date_validation_modified) =
            get_date_validations(&frontmatter, &path)?;

        let do_not_back_populate_regexes = frontmatter
            .as_ref()
            .map(|fm| fm.get_do_not_back_populate_regexes())
            .flatten();

        Ok(MarkdownFileInfo {
            content,
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
    fs_date: &DateTime<Local>
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

    let dates = [(
        frontmatter.as_ref().and_then(|fm| fm.date_created().cloned()),
        metadata.created().map(|t| t.into()).unwrap_or_else(|_| Local::now()),
    ), (
        frontmatter.as_ref().and_then(|fm| fm.date_modified().cloned()),
        metadata.modified().map(|t| t.into()).unwrap_or_else(|_| Local::now()),
    )];

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

pub fn process_date_modified_helper(date_modified: Option<String>) -> (Option<String>, bool) {
    let today = Local::now().format("[[%Y-%m-%d]]").to_string();

    match date_modified {
        Some(date_modified) => {
            if !is_wikilink(Some(&date_modified)) && is_valid_date(&date_modified) {
                let fix = format!("[[{}]]", date_modified.trim());
                (Some(fix), true)
            } else {
                (Some(date_modified), false)
            }
        }
        None => (Some(today), true),
    }
}

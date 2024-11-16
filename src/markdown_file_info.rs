#[cfg(test)]
mod date_fix_tests;
#[cfg(test)]
mod persist_frontmatter_tests;

use crate::file_utils::read_contents_from_file;
use crate::frontmatter::FrontMatter;
use crate::regex_utils::build_case_insensitive_word_finder;
use crate::wikilink::is_wikilink;
use crate::wikilink_types::InvalidWikilink;
use crate::yaml_frontmatter::{YamlFrontMatter, YamlFrontMatterError};
use crate::{CLOSING_WIKILINK, OPENING_WIKILINK};

use chrono::{DateTime, Local, NaiveDate};
use regex::Regex;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub enum DateValidationStatus {
    Valid,
    Missing,
    InvalidFormat,
    InvalidWikilink,
    FileSystemMismatch,
}

#[derive(Debug)]
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

        let metadata = fs::metadata(&path)?;

        let (frontmatter, frontmatter_error) = match FrontMatter::from_markdown_str(&content) {
            Ok(fm) => (Some(fm), None),
            Err(error) => (None, Some(error)),
        };

        // Construct DateValidation instances after frontmatter parsing
        let date_validation_created = DateValidation {
            frontmatter_date: frontmatter
                .as_ref()
                .and_then(|fm| fm.date_created().cloned()),
            file_system_date: metadata
                .created()
                .map(|t| t.into())
                .unwrap_or_else(|_| Local::now()),
            status: DateValidationStatus::Missing,
        };

        let date_validation_modified = DateValidation {
            frontmatter_date: frontmatter
                .as_ref()
                .and_then(|fm| fm.date_modified().cloned()),
            file_system_date: metadata
                .modified()
                .map(|t| t.into())
                .unwrap_or_else(|_| Local::now()),
            status: DateValidationStatus::Missing,
        };

        let do_not_back_populate_regexes = frontmatter
            .as_ref()
            .and_then(|fm| fm.get_do_not_back_populate_regexes());

        let mut info = MarkdownFileInfo {
            content,
            do_not_back_populate_regexes,
            date_validation_created,
            date_validation_modified,
            frontmatter,
            frontmatter_error,
            invalid_wikilinks: Vec::new(),
            image_links: Vec::new(),
            path,
        };

        info.process_dates();

        Ok(info)
    }

    // Helper method to add invalid wikilinks
    pub fn add_invalid_wikilinks(&mut self, wikilinks: Vec<InvalidWikilink>) {
        self.invalid_wikilinks.extend(wikilinks);
    }

    // New method for processing dates after deserialization
    fn process_dates(&mut self) {
        self.process_date_modified();
        // Later we'll add process_date_created here too
    }

    fn process_date_modified(&mut self) {
        if let Some(fm) = &mut self.frontmatter {
            let (new_date, needs_persist) =
                process_date_modified_helper(fm.date_modified().map(|s| s.clone()));

            if needs_persist {
                fm.update_date_modified(new_date);
                fm.set_needs_persist(true);
            }
        }
    }
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

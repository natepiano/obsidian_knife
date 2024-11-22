#[cfg(test)]
mod date_fix_tests;
#[cfg(test)]
mod parse_and_persist_tests;
#[cfg(test)]
mod persist_reason_tests;

use crate::frontmatter::FrontMatter;
use crate::utils::read_contents_from_file;
use crate::wikilink::is_wikilink;
use crate::wikilink_types::InvalidWikilink;
use crate::yaml_frontmatter::{find_yaml_section, YamlFrontMatter, YamlFrontMatterError};
use crate::{CLOSING_WIKILINK, OPENING_WIKILINK};

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use filetime::FileTime;
use itertools::Itertools;
use regex::Regex;
use std::error::Error;
use std::path::PathBuf;
use std::{fs, io};

#[derive(Debug, Clone, PartialEq)]
pub enum PersistReason {
    DateCreatedUpdated {
        reason: DateValidationIssue,
    },
    DateModifiedUpdated {
        reason: DateValidationIssue,
    },
    DateCreatedFixApplied {
        original_date: String,
        requested_fix: String,
    },
    BackPopulated,
    ImageReferencesModified,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DateValidationIssue {
    Missing,
    InvalidFormat,
    InvalidWikilink,
    FileSystemMismatch,
}

#[derive(Debug, PartialEq)]
pub struct DateValidation {
    pub frontmatter_date: Option<String>,
    pub file_system_date: DateTime<Utc>,
    pub issue: Option<DateValidationIssue>,
}

impl DateValidation {
    pub fn to_issue_string(&self) -> String {
        match &self.issue {
            None => "valid".to_string(),
            Some(issue) => match issue {
                DateValidationIssue::Missing => "missing".to_string(),
                DateValidationIssue::InvalidFormat => {
                    format_invalid_date("invalid format", &self.frontmatter_date)
                }
                DateValidationIssue::InvalidWikilink => {
                    format_invalid_date("invalid wikilink", &self.frontmatter_date)
                }
                DateValidationIssue::FileSystemMismatch => {
                    format_invalid_date("mismatch", &self.frontmatter_date)
                }
            },
        }
    }

    pub fn to_action_string(&self) -> Option<String> {
        self.issue
            .as_ref()
            .map(|_| format!("\"\\[[{}]]\"", self.file_system_date.format("%Y-%m-%d")))
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
    pub fix_date: Option<DateTime<Utc>>,
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
        file_created_date: DateTime<Utc>,
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
                    Utc.from_local_datetime(&naive_datetime).unwrap()
                })
        });

        DateCreatedFixValidation {
            date_string,
            fix_date: parsed_date,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackPopulateMatch {
    pub found_text: String,
    pub in_markdown_table: bool,
    pub line_number: usize,
    pub line_text: String,
    pub position: usize,
    pub relative_path: String,
    pub replacement: String,
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
    pub matches: Vec<BackPopulateMatch>,
    pub path: PathBuf,
    pub persist_reasons: Vec<PersistReason>,
}

impl MarkdownFileInfo {
    pub fn new(path: PathBuf) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let full_content = read_contents_from_file(&path)?;

        let (mut frontmatter, content, frontmatter_error) = match find_yaml_section(&full_content) {
            Ok(Some((yaml_section, after_yaml))) => {
                match FrontMatter::from_yaml_str(yaml_section) {
                    Ok(fm) => (Some(fm), after_yaml.to_string(), None),
                    Err(e) => (None, after_yaml.to_string(), Some(e)),
                }
            }
            Ok(None) => (None, full_content, Some(YamlFrontMatterError::Missing)),
            Err(e) => (None, full_content, Some(e)),
        };

        let (date_validation_created, date_validation_modified) =
            get_date_validations(&frontmatter, &path)?;

        let mut persist_reasons = process_date_validations(
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
                persist_reasons.push(PersistReason::DateCreatedFixApplied {
                    original_date: date_created_fix
                        .date_string
                        .as_ref()
                        .map(String::clone)
                        .unwrap_or_default(),
                    requested_fix: fix_date.format("%Y-%m-%d").to_string(),
                });
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
            matches: Vec::new(),
            path,
            persist_reasons,
        })
    }

    // Add a method to reconstruct the full markdown content
    pub fn to_full_content(&self) -> String {
        if let Some(ref fm) = self.frontmatter {
            if let Ok(yaml) = fm.to_yaml_str() {
                format!("---\n{}\n---\n{}", yaml.trim(), self.content.trim())
            } else {
                self.content.clone()
            }
        } else {
            self.content.clone()
        }
    }

    pub fn persist(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Write the updated content to the file
        fs::write(&self.path, self.to_full_content())?;

        let frontmatter = self.frontmatter.as_ref().expect("Frontmatter is required");
        let modified_date = frontmatter
            .raw_date_modified
            .ok_or_else(|| "raw_date_modified must be set for persist".to_string())?;

        if let Some(created_date) = frontmatter.raw_date_created {
            filetime::set_file_times(
                &self.path,
                FileTime::from_system_time(created_date.into()),
                FileTime::from_system_time(modified_date.into()),
            )?;
        } else {
            filetime::set_file_mtime(&self.path, FileTime::from_system_time(modified_date.into()))?;
        }

        Ok(())
    }

    // Helper method to add invalid wikilinks
    pub fn add_invalid_wikilinks(&mut self, wikilinks: Vec<InvalidWikilink>) {
        self.invalid_wikilinks.extend(wikilinks);
    }

    pub fn mark_as_back_populated(&mut self) {
        let fm = self
            .frontmatter
            .as_mut()
            .expect("Attempted to mark file as back populated without frontmatter");
        fm.set_date_modified_now();
        self.persist_reasons.push(PersistReason::BackPopulated);
    }

    pub fn record_image_references_change(&mut self) {
        let fm = self
            .frontmatter
            .as_mut()
            .expect("Attempted to record image references change on a file without frontmatter");
        fm.set_date_modified_now();
        self.persist_reasons
            .push(PersistReason::ImageReferencesModified);
    }
}

fn get_date_validation_issue(
    date_opt: Option<&String>,
    fs_date: &DateTime<Utc>,
) -> Option<DateValidationIssue> {
    match date_opt {
        None => Some(DateValidationIssue::Missing),
        Some(date_str) => {
            if !is_wikilink(Some(date_str)) {
                Some(DateValidationIssue::InvalidWikilink)
            } else {
                let extracted_date = extract_date(date_str);
                if !is_valid_date(extracted_date) {
                    Some(DateValidationIssue::InvalidFormat)
                } else if extracted_date != fs_date.format("%Y-%m-%d").to_string() {
                    Some(DateValidationIssue::FileSystemMismatch)
                } else {
                    None
                }
            }
        }
    }
}

fn get_date_validations(
    frontmatter: &Option<FrontMatter>,
    path: &PathBuf,
) -> Result<(DateValidation, DateValidation), io::Error> {
    let metadata = fs::metadata(path)?;

    let dates = [
        (
            frontmatter
                .as_ref()
                .and_then(|fm| fm.date_created().cloned()),
            metadata
                .created()
                .map(|t| t.into())
                .unwrap_or_else(|_| Utc::now()),
        ),
        (
            frontmatter
                .as_ref()
                .and_then(|fm| fm.date_modified().cloned()),
            metadata
                .modified()
                .map(|t| t.into())
                .unwrap_or_else(|_| Utc::now()),
        ),
    ];

    Ok(dates
        .into_iter()
        .map(|(frontmatter_date, fs_date)| {
            let issue = get_date_validation_issue(frontmatter_date.as_ref(), &fs_date);
            DateValidation {
                frontmatter_date,
                file_system_date: fs_date,
                issue,
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
    created_validation: &DateValidation,
    modified_validation: &DateValidation,
) -> Vec<PersistReason> {
    let mut reasons = Vec::new();

    if let Some(ref mut fm) = frontmatter {
        // Update created date if there's an issue
        if let Some(ref issue) = created_validation.issue {
            fm.set_date_created(created_validation.file_system_date);
            reasons.push(PersistReason::DateCreatedUpdated {
                reason: issue.clone(),
            });
        }

        // Update modified date if there's an issue
        if let Some(ref issue) = modified_validation.issue {
            fm.set_date_modified(modified_validation.file_system_date);
            reasons.push(PersistReason::DateModifiedUpdated {
                reason: issue.clone(),
            });
        }
    }

    reasons
}

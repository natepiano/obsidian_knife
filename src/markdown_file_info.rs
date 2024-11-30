#[cfg(test)]
mod date_tests;
#[cfg(test)]
mod parse_tests;
#[cfg(test)]
mod persist_tests;

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
use std::{fmt, fs, io};

#[derive(Debug, Clone, PartialEq)]
pub enum PersistReason {
    DateCreatedUpdated { reason: DateValidationIssue },
    DateModifiedUpdated { reason: DateValidationIssue },
    DateCreatedFixApplied,
    BackPopulated,
    ImageReferencesModified,
}

impl fmt::Display for PersistReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersistReason::DateCreatedUpdated { .. } => write!(f, "date_created updated"),
            PersistReason::DateModifiedUpdated { .. } => write!(f, "date_modified updated"),
            PersistReason::DateCreatedFixApplied => write!(f, "date_created_fix applied"),
            PersistReason::BackPopulated => write!(f, "back populated"),
            PersistReason::ImageReferencesModified => write!(f, "image references updated"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DateValidationIssue {
    Missing,
    InvalidDateFormat,
    InvalidWikilink,
    FileSystemMismatch,
}

impl fmt::Display for DateValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = match self {
            DateValidationIssue::Missing => "missing",
            DateValidationIssue::InvalidDateFormat => "invalid date format",
            DateValidationIssue::InvalidWikilink => "invalid wikilink",
            DateValidationIssue::FileSystemMismatch => "doesn't match file system",
        };
        write!(f, "{}", description)
    }
}

#[derive(Debug, PartialEq)]
pub struct DateValidation {
    pub frontmatter_date: Option<String>,
    pub file_system_date: DateTime<Utc>,
    pub issue: Option<DateValidationIssue>,
    pub operational_timezone: String,
}
// In markdown_file_info.rs
#[derive(Debug)]
pub struct DateCreatedFixValidation {
    pub date_string: Option<String>,
    pub fix_date: Option<DateTime<Utc>>,
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

#[derive(Debug, Default, Clone)]
pub struct BackPopulateMatch {
    pub found_text: String,
    pub frontmatter_line_count: usize,
    pub in_markdown_table: bool,
    pub line_number: usize,
    pub line_text: String,
    pub position: usize,
    pub relative_path: String,
    pub replacement: String,
}

#[derive(Debug, Default)]
pub struct BackPopulateMatchCollections {
    pub ambiguous: Vec<BackPopulateMatch>,
    pub unambiguous: Vec<BackPopulateMatch>,
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
    pub frontmatter_line_count: usize,
    pub image_links: Vec<String>,
    pub invalid_wikilinks: Vec<InvalidWikilink>,
    pub matches: BackPopulateMatchCollections,
    pub path: PathBuf,
    pub persist_reasons: Vec<PersistReason>,
}

impl MarkdownFileInfo {
    pub fn new(
        path: PathBuf,
        operational_timezone: &str,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let full_content = read_contents_from_file(&path)?;

        let yaml_result = find_yaml_section(&full_content);
        let frontmatter_line_count = match &yaml_result {
            Ok(Some((yaml_section, _))) => yaml_section.lines().count() + 2,
            _ => 0,
        };

        let (mut frontmatter, content, frontmatter_error) = match yaml_result {
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
            get_date_validations(&frontmatter, &path, operational_timezone)?;

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
                persist_reasons.push(PersistReason::DateCreatedFixApplied);
            }
        }

        let do_not_back_populate_regexes = frontmatter
            .as_ref()
            .and_then(|fm| fm.get_do_not_back_populate_regexes());

        Ok(MarkdownFileInfo {
            content,
            date_created_fix,
            do_not_back_populate_regexes,
            date_validation_created,
            date_validation_modified,
            frontmatter,
            frontmatter_error,
            frontmatter_line_count,
            invalid_wikilinks: Vec::new(),
            image_links: Vec::new(),
            matches: BackPopulateMatchCollections::default(),
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
    operational_timezone: &str,
) -> Option<DateValidationIssue> {
    // Check if the date is missing
    let date_str = match date_opt {
        Some(s) => s,
        None => return Some(DateValidationIssue::Missing),
    };

    // Check if the date string is a valid wikilink
    if !is_wikilink(Some(date_str)) {
        return Some(DateValidationIssue::InvalidWikilink);
    }

    let extracted_date = extract_date(date_str);

    // Validate the extracted date format
    if !is_valid_date(extracted_date) {
        return Some(DateValidationIssue::InvalidDateFormat);
    }

    // Parse the frontmatter date string into a NaiveDate
    let frontmatter_date = match NaiveDate::parse_from_str(extracted_date.trim(), "%Y-%m-%d") {
        Ok(date) => date,
        Err(_) => return Some(DateValidationIssue::InvalidDateFormat),
    };

    // Parse timezone string into a Tz
    let tz = match operational_timezone.parse::<chrono_tz::Tz>() {
        Ok(tz) => tz,
        Err(_) => return Some(DateValidationIssue::InvalidDateFormat),
    };

    // Convert UTC fs_date to the specified timezone
    let fs_date_local = fs_date.with_timezone(&tz);
    let fs_date_ymd = fs_date_local.format("%Y-%m-%d").to_string();

    // Compare the dates
    if frontmatter_date.format("%Y-%m-%d").to_string() != fs_date_ymd {
        return Some(DateValidationIssue::FileSystemMismatch);
    }

    // All validations passed
    None
}

fn get_date_validations(
    frontmatter: &Option<FrontMatter>,
    path: &PathBuf,
    operational_timezone: &str,
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
            let issue = get_date_validation_issue(
                frontmatter_date.as_ref(),
                &fs_date,
                operational_timezone,
            );
            DateValidation {
                frontmatter_date,
                file_system_date: fs_date,
                issue,
                operational_timezone: operational_timezone.to_string(),
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

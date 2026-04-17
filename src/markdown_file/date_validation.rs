use std::fmt;
use std::io;
use std::path::Path;

use chrono::DateTime;
use chrono::NaiveDate;
use chrono::TimeZone;
use chrono::Utc;
use itertools::Itertools;

use crate::constants::CLOSING_WIKILINK;
use crate::constants::FORMAT_DATE;
use crate::constants::NOON_HOUR;
use crate::constants::OPENING_WIKILINK;
use crate::frontmatter::FrontMatter;
use crate::wikilink;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistReason {
    DateCreatedUpdated { reason: DateValidationIssue },
    DateModifiedUpdated { reason: DateValidationIssue },
    DateCreatedFixApplied,
    BackPopulated,
    FrontmatterCreated,
    ImageReferencesModified,
}

impl fmt::Display for PersistReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DateCreatedUpdated { .. } => write!(f, "date_created updated"),
            Self::DateModifiedUpdated { .. } => write!(f, "date_modified updated"),
            Self::DateCreatedFixApplied => write!(f, "date_created_fix applied"),
            Self::BackPopulated => write!(f, "back populated"),
            Self::FrontmatterCreated => write!(f, "frontmatter created"),
            Self::ImageReferencesModified => write!(f, "image references updated"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateValidationIssue {
    Missing,
    InvalidDateFormat,
    InvalidWikilink,
    FileSystemMismatch,
}

impl fmt::Display for DateValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = match self {
            Self::Missing => "missing",
            Self::InvalidDateFormat => "invalid date format",
            Self::InvalidWikilink => "invalid wikilink",
            Self::FileSystemMismatch => "doesn't match file system",
        };
        write!(f, "{description}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateValidation {
    pub frontmatter_date:     Option<String>,
    pub file_system_date:     DateTime<Utc>,
    pub issue:                Option<DateValidationIssue>,
    pub operational_timezone: String,
}

impl DateValidation {
    pub fn operational_file_system_date(&self) -> DateTime<Utc> {
        self.operational_timezone
            .parse::<chrono_tz::Tz>()
            .map_or(self.file_system_date, |tz| {
                let local = self.file_system_date.with_timezone(&tz);
                let naive = local.naive_local();
                DateTime::from_naive_utc_and_offset(naive, Utc)
            })
    }
}

#[derive(Debug, Default, Clone)]
pub struct DateCreatedFixValidation {
    #[cfg(test)]
    pub date_string: Option<String>,
    pub fix_date:    Option<DateTime<Utc>>,
}

impl DateCreatedFixValidation {
    pub(super) fn from_frontmatter(
        frontmatter: Option<&FrontMatter>,
        file_created_date: DateTime<Utc>,
        operational_timezone: &str,
    ) -> Self {
        let fix_str =
            frontmatter.and_then(|frontmatter| frontmatter.date_created_fix().map(String::from));

        let parsed_date = fix_str.as_ref().and_then(|date_str| {
            let date = if wikilink::is_wikilink(Some(date_str)) {
                extract_date(date_str)
            } else {
                date_str.trim().trim_matches('"')
            };

            NaiveDate::parse_from_str(date.trim(), FORMAT_DATE)
                .ok()
                .map(|naive_date| {
                    let tz: chrono_tz::Tz = operational_timezone.parse().unwrap_or(chrono_tz::UTC);

                    #[allow(clippy::unwrap_used, reason = "noon (12:00:00) is always a valid time")]
                    let naive_datetime = naive_date.and_hms_opt(NOON_HOUR, 0, 0).unwrap();

                    let fixed_date = tz
                        .from_local_datetime(&naive_datetime)
                        .single()
                        .map_or_else(|| file_created_date, |dt| dt.with_timezone(&Utc));

                    let fixed_date_local = fixed_date.with_timezone(&tz);
                    assert_eq!(
                        fixed_date_local.date_naive(),
                        naive_date,
                        "Date mismatch: fixed_date converts to {} in {operational_timezone} but should be {naive_date}",
                        fixed_date_local.date_naive(),
                    );

                    fixed_date
                })
        });

        Self {
            #[cfg(test)]
            date_string:              fix_str,
            fix_date:                 parsed_date,
        }
    }
}

#[allow(
    clippy::unwrap_used,
    reason = "iterator always yields exactly 2 elements from fixed-size array"
)]
pub(super) fn get_date_validations(
    frontmatter: Option<&FrontMatter>,
    path: &Path,
    operational_timezone: &str,
) -> Result<(DateValidation, DateValidation), io::Error> {
    let metadata = std::fs::metadata(path)?;

    let dates = [
        (
            frontmatter.and_then(|frontmatter| frontmatter.date_created().map(String::from)),
            metadata.created().map_or_else(|_| Utc::now(), Into::into),
        ),
        (
            frontmatter.and_then(|frontmatter| frontmatter.date_modified().map(String::from)),
            metadata.modified().map_or_else(|_| Utc::now(), Into::into),
        ),
    ];

    // skip when the create date has a `date_created_fix` in place, we don't need to validate as
    // it's moot
    Ok(dates
        .into_iter()
        .map(|(frontmatter_date, fs_date)| {
            let issue = get_date_validation_issue(
                frontmatter_date.as_deref(),
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

pub(super) fn get_date_validation_issue(
    date_opt: Option<&str>,
    fs_date: &DateTime<Utc>,
    operational_timezone: &str,
) -> Option<DateValidationIssue> {
    // Check if the date is missing
    let Some(date_str) = date_opt else {
        return Some(DateValidationIssue::Missing);
    };

    // Check if the date string is a valid wikilink
    if !wikilink::is_wikilink(Some(date_str)) {
        return Some(DateValidationIssue::InvalidWikilink);
    }

    let extracted_date = extract_date(date_str);

    // Validate the extracted date format
    if !is_valid_date(extracted_date) {
        return Some(DateValidationIssue::InvalidDateFormat);
    }

    // Parse the frontmatter date string into a `NaiveDate`
    let Ok(frontmatter_date) = NaiveDate::parse_from_str(extracted_date.trim(), FORMAT_DATE) else {
        return Some(DateValidationIssue::InvalidDateFormat);
    };

    // Parse timezone string into a Tz
    let Ok(tz) = operational_timezone.parse::<chrono_tz::Tz>() else {
        return Some(DateValidationIssue::InvalidDateFormat);
    };

    // Convert UTC fs_date to the specified timezone
    let fs_date_local = fs_date.with_timezone(&tz);
    let fs_date_naive = fs_date_local.date_naive();

    // Compare the dates
    if frontmatter_date != fs_date_naive {
        return Some(DateValidationIssue::FileSystemMismatch);
    }

    // All validations passed
    None
}

// Extracts the date string from a possible wikilink format
pub(super) fn extract_date(date_str: &str) -> &str {
    let date_str = date_str.trim();
    if wikilink::is_wikilink(Some(date_str)) {
        date_str
            .trim_start_matches(OPENING_WIKILINK)
            .trim_end_matches(CLOSING_WIKILINK)
            .trim()
    } else {
        date_str
    }
}

// Validates if a string is a valid YYYY-MM-DD date
pub(super) fn is_valid_date(date_str: &str) -> bool {
    NaiveDate::parse_from_str(date_str.trim(), FORMAT_DATE).is_ok()
}

pub(super) fn process_date_validations(
    frontmatter: &mut Option<FrontMatter>,
    created_validation: &DateValidation,
    modified_validation: &DateValidation,
    date_created_fix_validation: &DateCreatedFixValidation,
    operational_timezone: &str,
) -> Vec<PersistReason> {
    let mut reasons = Vec::new();

    if let Some(frontmatter) = frontmatter {
        let mut skip_date_created = false;

        if let Some(fix_date) = date_created_fix_validation.fix_date {
            skip_date_created = true;

            frontmatter.set_date_created(fix_date, operational_timezone);
            frontmatter.remove_date_created_fix();
            reasons.push(PersistReason::DateCreatedFixApplied);
        }

        // Update created date if there's an issue
        if let Some(ref issue) = created_validation.issue
            && !skip_date_created
        {
            frontmatter.set_date_created(created_validation.file_system_date, operational_timezone);
            reasons.push(PersistReason::DateCreatedUpdated {
                reason: issue.clone(),
            });
        }

        // Update modified date if there's an issue
        if let Some(ref issue) = modified_validation.issue {
            frontmatter
                .set_date_modified(modified_validation.file_system_date, operational_timezone);
            reasons.push(PersistReason::DateModifiedUpdated {
                reason: issue.clone(),
            });
        }
    }

    reasons
}

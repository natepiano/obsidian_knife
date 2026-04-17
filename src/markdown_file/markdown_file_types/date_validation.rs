use std::fmt;

use chrono::DateTime;
use chrono::NaiveDate;
use chrono::TimeZone;
use chrono::Utc;

use crate::constants::FORMAT_DATE;
use crate::constants::NOON_HOUR;
use crate::frontmatter::FrontMatter;
use crate::markdown_file::date_validation;
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
    pub fn from_frontmatter(
        frontmatter: Option<&FrontMatter>,
        file_created_date: DateTime<Utc>,
        operational_timezone: &str,
    ) -> Self {
        let fix_str =
            frontmatter.and_then(|frontmatter| frontmatter.date_created_fix().map(String::from));

        let parsed_date = fix_str.as_ref().and_then(|date_str| {
            let date = if wikilink::is_wikilink(Some(date_str)) {
                date_validation::extract_date(date_str)
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

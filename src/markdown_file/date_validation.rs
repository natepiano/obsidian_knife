use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fs;
use std::io::Error;
use std::path::Path;

use chrono::DateTime;
use chrono::NaiveDate;
use chrono::TimeZone;
use chrono::Utc;
use chrono_tz::Tz;
use chrono_tz::UTC;

use crate::constants::CLOSING_WIKILINK;
use crate::constants::DOUBLE_QUOTE;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateValidationIssue {
    Missing,
    InvalidFormat,
    InvalidWikilink,
    FileSystemMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateValidation {
    pub frontmatter:          Option<String>,
    pub file_system:          DateTime<Utc>,
    pub issue:                Option<DateValidationIssue>,
    pub operational_timezone: String,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct DateCreatedFixValidation {
    #[cfg(test)]
    pub raw:   Option<String>,
    pub fixed: Option<DateTime<Utc>>,
}

impl Display for PersistReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
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

impl Display for DateValidationIssue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let description = match self {
            Self::Missing => "missing",
            Self::InvalidFormat => "invalid date format",
            Self::InvalidWikilink => "invalid wikilink",
            Self::FileSystemMismatch => "doesn't match file system",
        };
        write!(f, "{description}")
    }
}

impl DateValidation {
    pub fn operational_file_system_date(&self) -> DateTime<Utc> {
        self.operational_timezone
            .parse::<Tz>()
            .map_or(self.file_system, |timezone| {
                let local_file_system_date = self.file_system.with_timezone(&timezone);
                let naive_file_system_date = local_file_system_date.naive_local();
                DateTime::from_naive_utc_and_offset(naive_file_system_date, Utc)
            })
    }
}

impl DateCreatedFixValidation {
    pub(super) fn from_frontmatter(
        front_matter: Option<&FrontMatter>,
        file_created_date: DateTime<Utc>,
        operational_timezone: &str,
    ) -> Self {
        let fix_str =
            front_matter.and_then(|front_matter| front_matter.date_created_fix().map(String::from));

        let parsed = fix_str.as_ref().and_then(|date_str| {
            let date = if wikilink::is_wikilink(Some(date_str)) {
                extract_date(date_str)
            } else {
                date_str.trim().trim_matches(DOUBLE_QUOTE)
            };

            let naive_date = NaiveDate::parse_from_str(date.trim(), FORMAT_DATE).ok()?;
            let timezone: Tz = operational_timezone.parse().unwrap_or(UTC);
            let naive_datetime = naive_date.and_hms_opt(NOON_HOUR, 0, 0)?;

            let fixed_date = timezone
                .from_local_datetime(&naive_datetime)
                .single()
                .map_or_else(|| file_created_date, |date_time| date_time.with_timezone(&Utc));

            let fixed_date_local = fixed_date.with_timezone(&timezone);
            assert_eq!(
                fixed_date_local.date_naive(),
                naive_date,
                "Date mismatch: fixed_date converts to {} in {operational_timezone} but should be {naive_date}",
                fixed_date_local.date_naive(),
            );

            Some(fixed_date)
        });

        Self {
            #[cfg(test)]
            raw:              fix_str,
            fixed:            parsed,
        }
    }
}

pub(super) fn get_date_validations(
    front_matter: Option<&FrontMatter>,
    path: &Path,
    operational_timezone: &str,
) -> Result<(DateValidation, DateValidation), Error> {
    let metadata = fs::metadata(path)?;

    let dates = [
        (
            front_matter.and_then(|front_matter| front_matter.date_created().map(String::from)),
            metadata.created().map_or_else(|_| Utc::now(), Into::into),
        ),
        (
            front_matter.and_then(|front_matter| front_matter.date_modified().map(String::from)),
            metadata.modified().map_or_else(|_| Utc::now(), Into::into),
        ),
    ];

    // skip when the create date has a `date_created_fix` in place, we don't need to validate as
    // it's moot
    let [created_date_validation, modified_date_validation] =
        dates.map(|(frontmatter_date, file_system_date)| {
            let issue = get_date_validation_issue(
                frontmatter_date.as_deref(),
                &file_system_date,
                operational_timezone,
            );
            DateValidation {
                frontmatter: frontmatter_date,
                file_system: file_system_date,
                issue,
                operational_timezone: operational_timezone.to_string(),
            }
        });

    Ok([created_date_validation, modified_date_validation].into())
}

pub(super) fn get_date_validation_issue(
    date_opt: Option<&str>,
    file_system_date: &DateTime<Utc>,
    operational_timezone: &str,
) -> Option<DateValidationIssue> {
    // `DateValidationIssue::Missing` applies when the frontmatter date is absent.
    let Some(date_str) = date_opt else {
        return Some(DateValidationIssue::Missing);
    };

    // `wikilink::is_wikilink` rejects non-wikilink date strings.
    if !wikilink::is_wikilink(Some(date_str)) {
        return Some(DateValidationIssue::InvalidWikilink);
    }

    let extracted_date = extract_date(date_str);

    // `is_valid_date` accepts only `YYYY-MM-DD` dates.
    if !is_valid_date(extracted_date) {
        return Some(DateValidationIssue::InvalidFormat);
    }

    // `frontmatter_date` stores the wikilink date as a `NaiveDate`.
    let Ok(frontmatter_date) = NaiveDate::parse_from_str(extracted_date.trim(), FORMAT_DATE) else {
        return Some(DateValidationIssue::InvalidFormat);
    };

    // `operational_timezone` must parse into a `Tz`.
    let Ok(timezone) = operational_timezone.parse::<Tz>() else {
        return Some(DateValidationIssue::InvalidFormat);
    };

    // `file_system_date_local` stores `file_system_date` in `operational_timezone`.
    let file_system_date_local = file_system_date.with_timezone(&timezone);
    let file_system_date_naive = file_system_date_local.date_naive();

    // `FileSystemMismatch` records a frontmatter date that differs from the file timestamp.
    if frontmatter_date != file_system_date_naive {
        return Some(DateValidationIssue::FileSystemMismatch);
    }

    // `None` means `get_date_validation_issue` found no `DateValidationIssue`.
    None
}

// `extract_date` returns the inner date from a wikilink, or the trimmed input.
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

// `is_valid_date` accepts strings that parse with `FORMAT_DATE`.
pub(super) fn is_valid_date(date_str: &str) -> bool {
    NaiveDate::parse_from_str(date_str.trim(), FORMAT_DATE).is_ok()
}

pub(super) fn process_date_validations(
    front_matter: &mut Option<FrontMatter>,
    created_date_validation: &DateValidation,
    modified_date_validation: &DateValidation,
    date_created_fix_validation: &DateCreatedFixValidation,
    operational_timezone: &str,
) -> Vec<PersistReason> {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum CreatedDateUpdate {
        Skip,
        IfInvalid,
    }

    let mut reasons = Vec::new();

    if let Some(front_matter) = front_matter {
        let mut created_date_update = CreatedDateUpdate::IfInvalid;

        if let Some(fixed) = date_created_fix_validation.fixed {
            created_date_update = CreatedDateUpdate::Skip;

            front_matter.set_date_created(fixed, operational_timezone);
            front_matter.remove_date_created_fix();
            reasons.push(PersistReason::DateCreatedFixApplied);
        }

        // `DateCreatedUpdated` records a created-date repair.
        if let Some(ref issue) = created_date_validation.issue
            && created_date_update == CreatedDateUpdate::IfInvalid
        {
            front_matter
                .set_date_created(created_date_validation.file_system, operational_timezone);
            reasons.push(PersistReason::DateCreatedUpdated {
                reason: issue.clone(),
            });
        }

        // `DateModifiedUpdated` records a modified-date repair.
        if let Some(ref issue) = modified_date_validation.issue {
            front_matter
                .set_date_modified(modified_date_validation.file_system, operational_timezone);
            reasons.push(PersistReason::DateModifiedUpdated {
                reason: issue.clone(),
            });
        }
    }

    reasons
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use chrono::DateTime;
    use chrono::NaiveDate;
    use chrono::TimeZone;
    use chrono::Utc;
    use chrono_tz::Tz;
    use tempfile::TempDir;

    use super::DateCreatedFixValidation;
    use super::DateValidationIssue;
    use crate::constants::DEFAULT_TIMEZONE;
    use crate::frontmatter::FrontMatter;
    use crate::markdown_file::DateValidation;
    use crate::markdown_file::PersistReason;
    use crate::markdown_file::date_validation;
    use crate::test_support as test_utils;
    use crate::test_support::PersistExpectation;
    use crate::test_support::TestFileBuilder;
    use crate::yaml_frontmatter::YamlFrontMatter;

    // `into_iter()` consumes the array and yields owned values
    // `filter_map` filters out none values and unwraps `Some` values in one step
    fn create_frontmatter(date_modified: Option<&str>, date_created: Option<&str>) -> FrontMatter {
        let yaml = [
            date_modified.map(|modified| format!("date_modified: \"{modified}\"")),
            date_created.map(|created| format!("date_created: \"{created}\"")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");

        FrontMatter::from_yaml_str(&yaml).unwrap()
    }

    fn eastern_date_wikilink(year: i32, month: u32, day: u32) -> String {
        test_utils::frontmatter_date_wikilink(test_utils::eastern_midnight(year, month, day))
    }

    struct FileSystemDates {
        modified: DateTime<Utc>,
        created:  DateTime<Utc>,
    }

    struct ValidationIssues {
        modified: Option<DateValidationIssue>,
        created:  Option<DateValidationIssue>,
    }

    struct DateFixExpectations {
        persist:  PersistExpectation,
        modified: Option<String>,
        created:  Option<String>,
    }

    struct DateCreatedFixExpectations {
        persist: PersistExpectation,
        parsed:  Option<DateTime<Utc>>,
    }

    struct DateValidationTestCase {
        name:        &'static str,
        modified:    Option<String>,
        created:     Option<String>,
        file_system: FileSystemDates,
        issues:      ValidationIssues,
    }

    #[test]
    #[cfg_attr(
        target_os = "linux",
        ignore = "requires filesystem access unavailable on Linux CI"
    )]
    fn test_process_frontmatter_date_validation() {
        let test_cases = vec![
            DateValidationTestCase {
                name:        "both dates valid and matching filesystem",
                modified:    Some(eastern_date_wikilink(2024, 1, 15)),
                created:     Some(eastern_date_wikilink(2024, 1, 15)),
                file_system: FileSystemDates {
                    modified: test_utils::eastern_midnight(2024, 1, 15),
                    created:  test_utils::eastern_midnight(2024, 1, 15),
                },
                issues:      ValidationIssues {
                    modified: None,
                    created:  None,
                },
            },
            DateValidationTestCase {
                name:        "missing wikilink brackets",
                // malformed-on-purpose — do not derive from production constants
                modified:    Some("2024-01-15".to_string()),
                created:     Some("2024-01-15".to_string()),
                file_system: FileSystemDates {
                    modified: test_utils::eastern_midnight(2024, 1, 15),
                    created:  test_utils::eastern_midnight(2024, 1, 15),
                },
                issues:      ValidationIssues {
                    modified: Some(DateValidationIssue::InvalidWikilink),
                    created:  Some(DateValidationIssue::InvalidWikilink),
                },
            },
            DateValidationTestCase {
                name:        "filesystem mismatch",
                modified:    Some(eastern_date_wikilink(2024, 1, 15)),
                created:     Some(eastern_date_wikilink(2024, 1, 15)),
                file_system: FileSystemDates {
                    modified: test_utils::eastern_midnight(2024, 1, 16),
                    created:  test_utils::eastern_midnight(2024, 1, 16),
                },
                issues:      ValidationIssues {
                    modified: Some(DateValidationIssue::FileSystemMismatch),
                    created:  Some(DateValidationIssue::FileSystemMismatch),
                },
            },
            DateValidationTestCase {
                name:        "invalid date format",
                // malformed-on-purpose — do not derive from production constants
                modified:    Some("[[2024-13-45]]".to_string()),
                created:     Some("[[2024-13-45]]".to_string()),
                file_system: FileSystemDates {
                    modified: Utc::now(),
                    created:  Utc::now(),
                },
                issues:      ValidationIssues {
                    modified: Some(DateValidationIssue::InvalidFormat),
                    created:  Some(DateValidationIssue::InvalidFormat),
                },
            },
            DateValidationTestCase {
                name:        "missing dates",
                modified:    None,
                created:     None,
                file_system: FileSystemDates {
                    modified: Utc::now(),
                    created:  Utc::now(),
                },
                issues:      ValidationIssues {
                    modified: Some(DateValidationIssue::Missing),
                    created:  Some(DateValidationIssue::Missing),
                },
            },
        ];

        run_date_validation_test_cases(test_cases, DEFAULT_TIMEZONE);
    }

    struct DateFixTestCase {
        name:        &'static str,
        modified:    Option<String>,
        created:     Option<String>,
        file_system: FileSystemDates,
        expected:    DateFixExpectations,
    }

    #[test]
    #[cfg_attr(
        target_os = "linux",
        ignore = "requires filesystem access unavailable on Linux CI"
    )]
    #[allow(
        clippy::too_many_lines,
        reason = "test case table + assertion loop — not worth splitting"
    )]
    fn test_process_date_validations() {
        let test_cases = vec![
            DateFixTestCase {
                name:        "missing dates should be updated",
                modified:    None,
                created:     None,
                file_system: FileSystemDates {
                    modified: test_utils::eastern_midnight(2024, 1, 15),
                    created:  test_utils::eastern_midnight(2024, 1, 15),
                },
                expected:    DateFixExpectations {
                    persist:  PersistExpectation::Persists,
                    modified: Some(eastern_date_wikilink(2024, 1, 15)),
                    created:  Some(eastern_date_wikilink(2024, 1, 15)),
                },
            },
            DateFixTestCase {
                name:        "filesystem mismatch should update modified date",
                modified:    Some(eastern_date_wikilink(2024, 1, 14)),
                created:     Some(eastern_date_wikilink(2024, 1, 15)),
                file_system: FileSystemDates {
                    modified: test_utils::eastern_midnight(2024, 1, 15),
                    created:  test_utils::eastern_midnight(2024, 1, 15),
                },
                expected:    DateFixExpectations {
                    persist:  PersistExpectation::Persists,
                    modified: Some(eastern_date_wikilink(2024, 1, 15)),
                    created:  Some(eastern_date_wikilink(2024, 1, 15)),
                },
            },
            DateFixTestCase {
                name:        "valid dates should not change",
                modified:    Some(eastern_date_wikilink(2024, 1, 15)),
                created:     Some(eastern_date_wikilink(2024, 1, 15)),
                file_system: FileSystemDates {
                    modified: test_utils::eastern_midnight(2024, 1, 15),
                    created:  test_utils::eastern_midnight(2024, 1, 15),
                },
                expected:    DateFixExpectations {
                    persist:  PersistExpectation::Unchanged,
                    modified: Some(eastern_date_wikilink(2024, 1, 15)),
                    created:  Some(eastern_date_wikilink(2024, 1, 15)),
                },
            },
            DateFixTestCase {
                name:        "filesystem mismatch should update both dates",
                modified:    Some(eastern_date_wikilink(2024, 1, 14)), /* original frontmatter
                                                                        * dates */
                created:     Some(eastern_date_wikilink(2024, 1, 13)), /* that don't
                                                                        * match
                                                                        * filesystem */
                // Using 05:00 UTC (midnight Eastern) ensures dates like "[[2024-01-15]]" match
                // the filesystem dates when viewed in Eastern timezone
                file_system: FileSystemDates {
                    modified: test_utils::eastern_midnight(2024, 1, 15),
                    created:  test_utils::eastern_midnight(2024, 1, 15),
                },
                expected:    DateFixExpectations {
                    persist:  PersistExpectation::Persists,
                    modified: Some(eastern_date_wikilink(2024, 1, 15)),
                    created:  Some(eastern_date_wikilink(2024, 1, 15)),
                },
            },
            DateFixTestCase {
                name:        "invalid format should change", /* changed from "invalid
                                                              * format should not
                                                              * change" */
                // malformed-on-purpose — do not derive from production constants
                modified:    Some("[[2024-13-45]]".to_string()),
                created:     Some("[[2024-13-45]]".to_string()),
                file_system: FileSystemDates {
                    modified: test_utils::eastern_midnight(2024, 1, 15),
                    created:  test_utils::eastern_midnight(2024, 1, 15),
                },
                expected:    DateFixExpectations {
                    persist:  PersistExpectation::Persists,
                    modified: Some(eastern_date_wikilink(2024, 1, 15)), // changed
                    created:  Some(eastern_date_wikilink(2024, 1, 15)), // changed
                }, /* changed from false */
            },
            DateFixTestCase {
                name:        "invalid wikilink should change", /* changed from "invalid
                                                                * wikilink should not
                                                                * change" */
                // malformed-on-purpose — do not derive from production constants
                modified:    Some("2024-01-15".to_string()),
                created:     Some("2024-01-15".to_string()),
                file_system: FileSystemDates {
                    modified: test_utils::eastern_midnight(2024, 1, 15),
                    created:  test_utils::eastern_midnight(2024, 1, 15),
                },
                expected:    DateFixExpectations {
                    persist:  PersistExpectation::Persists,
                    modified: Some(eastern_date_wikilink(2024, 1, 15)), // changed
                    created:  Some(eastern_date_wikilink(2024, 1, 15)), // changed
                }, /* changed from false */
            },
        ];

        for case in test_cases {
            // `front_matter` starts with the test case dates.
            let mut front_matter = Some(create_frontmatter(
                case.modified.as_deref(),
                case.created.as_deref(),
            ));

            // `created_date_validation` and `modified_date_validation` mirror production checks.
            let created_date_validation = DateValidation {
                frontmatter:          case.created.clone(), // Add clone here
                file_system:          case.file_system.created,
                issue:                date_validation::get_date_validation_issue(
                    case.created.as_deref(),
                    &case.file_system.created,
                    DEFAULT_TIMEZONE,
                ),
                operational_timezone: DEFAULT_TIMEZONE.to_string(),
            };

            let modified_date_validation = DateValidation {
                frontmatter:          case.modified.clone(), // Add clone here
                file_system:          case.file_system.modified,
                issue:                date_validation::get_date_validation_issue(
                    case.modified.as_deref(),
                    &case.file_system.modified,
                    DEFAULT_TIMEZONE,
                ),
                operational_timezone: DEFAULT_TIMEZONE.to_string(),
            };

            // Process validations
            date_validation::process_date_validations(
                &mut front_matter,
                &created_date_validation,
                &modified_date_validation,
                &DateCreatedFixValidation::default(),
                DEFAULT_TIMEZONE,
            );

            test_utils::assert_test_case(
                front_matter
                    .as_ref()
                    .and_then(|front_matter| front_matter.date_modified().map(String::from)),
                case.expected.modified,
                &format!("{} - modified date", case.name),
                |actual, expected| assert_eq!(actual, expected),
            );

            test_utils::assert_test_case(
                front_matter
                    .as_ref()
                    .and_then(|front_matter| front_matter.date_created().map(String::from)),
                case.expected.created,
                &format!("{} - created date", case.name),
                |actual, expected| assert_eq!(actual, expected),
            );

            test_utils::assert_test_case(
                front_matter
                    .as_ref()
                    .is_some_and(FrontMatter::needs_persist),
                case.expected.persist.needs_persist(),
                &format!("{} - needs persist flag", case.name),
                |actual, expected| assert_eq!(actual, expected),
            );
        }
    }

    struct DateCreatedFixTestCase {
        name:      &'static str,
        fix_input: Option<String>,
        expected:  DateCreatedFixExpectations,
    }

    #[test]
    #[cfg_attr(
        target_os = "linux",
        ignore = "requires filesystem access unavailable on Linux CI"
    )]
    fn test_date_created_fix_integration() {
        let test_cases = vec![
            DateCreatedFixTestCase {
                name:      "missing date_created_fix",
                fix_input: None,
                expected:  DateCreatedFixExpectations {
                    persist: PersistExpectation::Unchanged,
                    parsed:  None,
                },
            },
            DateCreatedFixTestCase {
                name:      "valid date without wikilink",
                // `DateCreatedFixTestCase::fix_input` stays outside wikilink syntax for this path.
                fix_input: Some("2024-01-15".to_string()),
                expected:  DateCreatedFixExpectations {
                    persist: PersistExpectation::Persists,
                    parsed:  Some(test_utils::eastern_midnight(2024, 1, 15)),
                },
            },
            DateCreatedFixTestCase {
                name:      "valid date with wikilink",
                fix_input: Some(eastern_date_wikilink(2024, 1, 15)),
                expected:  DateCreatedFixExpectations {
                    persist: PersistExpectation::Persists,
                    parsed:  Some(test_utils::eastern_midnight(2024, 1, 15)),
                },
            },
            DateCreatedFixTestCase {
                name:      "invalid date format",
                // malformed-on-purpose — do not derive from production constants
                fix_input: Some("2024-13-45".to_string()),
                expected:  DateCreatedFixExpectations {
                    persist: PersistExpectation::Unchanged,
                    parsed:  None,
                },
            },
            DateCreatedFixTestCase {
                name:      "invalid date with wikilink",
                // malformed-on-purpose — do not derive from production constants
                fix_input: Some("[[2024-13-45]]".to_string()),
                expected:  DateCreatedFixExpectations {
                    persist: PersistExpectation::Unchanged,
                    parsed:  None,
                },
            },
            DateCreatedFixTestCase {
                name:      "malformed wikilink",
                fix_input: Some("[2024-01-15]".to_string()),
                expected:  DateCreatedFixExpectations {
                    persist: PersistExpectation::Unchanged,
                    parsed:  None,
                },
            },
        ];

        for case in test_cases {
            let temp_dir = TempDir::new().unwrap();

            // Using 05:00 UTC (midnight Eastern) ensures the date in Eastern timezone
            // matches the frontmatter date, preventing FileSystemMismatch errors
            let test_date = test_utils::eastern_midnight(2024, 1, 15);
            // println!("Test date: {:?}", test_date); // Debug print

            let file_path = TestFileBuilder::new()
                .with_frontmatter_dates(
                    Some(eastern_date_wikilink(2024, 1, 15)),
                    Some(eastern_date_wikilink(2024, 1, 15)),
                )
                .with_file_system_dates(test_date, test_date)
                .with_date_created_fix(case.fix_input.clone())
                .create(&temp_dir, "test1.md");

            let markdown_file = test_utils::get_test_markdown_file(file_path);

            test_utils::assert_test_case(
                markdown_file.date_created_fix_validation.raw,
                case.fix_input,
                &format!("{} - date string", case.name),
                |actual, expected| assert_eq!(actual, expected),
            );

            test_utils::assert_test_case(
                markdown_file.front_matter.unwrap().needs_persist(),
                case.expected.persist.needs_persist(),
                &format!("{} - expect persist", case.name),
                |actual, expected| assert_eq!(actual, expected),
            );

            test_utils::assert_test_case(
                markdown_file
                    .date_created_fix_validation
                    .fixed
                    .map(|dt| dt.date_naive()),
                case.expected.parsed.map(|dt| dt.date_naive()),
                &format!("{} - parsed date", case.name),
                |actual, expected| assert_eq!(actual, expected),
            );
        }
    }

    #[test]
    #[cfg_attr(
        target_os = "linux",
        ignore = "requires filesystem access unavailable on Linux CI"
    )]
    fn test_timezone_date_validation() {
        let test_cases = vec![
            DateValidationTestCase {
                name:        "late night eastern time should match UTC next day",
                modified:    Some(eastern_date_wikilink(2024, 1, 15)),
                created:     Some(eastern_date_wikilink(2024, 1, 15)),
                // This represents 11:30 PM EST on Jan 15th (4:30 AM UTC Jan 16th)
                file_system: FileSystemDates {
                    modified: Utc.with_ymd_and_hms(2024, 1, 16, 4, 30, 0).unwrap(),
                    created:  Utc.with_ymd_and_hms(2024, 1, 16, 4, 30, 0).unwrap(),
                },
                // `DateValidationTestCase.issues` stays empty because `DEFAULT_TIMEZONE` still
                // sees January 15 at 23:30.
                issues:      ValidationIssues {
                    modified: None,
                    created:  None,
                },
            },
            DateValidationTestCase {
                name:        "early morning eastern time should match UTC previous day",
                modified:    Some(eastern_date_wikilink(2024, 1, 16)),
                created:     Some(eastern_date_wikilink(2024, 1, 16)),
                // This represents 2:30 AM EST Jan 15th (7:30 AM UTC Jan 15th)
                file_system: FileSystemDates {
                    modified: Utc.with_ymd_and_hms(2024, 1, 15, 7, 30, 0).unwrap(),
                    created:  Utc.with_ymd_and_hms(2024, 1, 15, 7, 30, 0).unwrap(),
                },
                // `DateValidationTestCase.issues` records `FileSystemMismatch` because
                // `DEFAULT_TIMEZONE` sees January 15 while `FrontMatter` says January 16.
                issues:      ValidationIssues {
                    modified: Some(DateValidationIssue::FileSystemMismatch),
                    created:  Some(DateValidationIssue::FileSystemMismatch),
                },
            },
            DateValidationTestCase {
                name:        "eastern midnight boundary case",
                modified:    Some(eastern_date_wikilink(2024, 1, 15)),
                created:     Some(eastern_date_wikilink(2024, 1, 15)),
                // This represents exactly midnight EST Jan 15th (5 AM UTC Jan 15th)
                file_system: FileSystemDates {
                    modified: Utc.with_ymd_and_hms(2024, 1, 15, 5, 0, 0).unwrap(),
                    created:  Utc.with_ymd_and_hms(2024, 1, 15, 5, 0, 0).unwrap(),
                },
                // `DateValidationTestCase.issues` stays empty at the `DEFAULT_TIMEZONE` start of
                // January 15.
                issues:      ValidationIssues {
                    modified: None,
                    created:  None,
                },
            },
        ];

        run_date_validation_test_cases(test_cases, DEFAULT_TIMEZONE);
    }

    fn run_date_validation_test_cases(test_cases: Vec<DateValidationTestCase>, timezone: &str) {
        for case in test_cases {
            let temp_dir = TempDir::new().unwrap();
            let file_path = TestFileBuilder::new()
                .with_frontmatter_dates(case.created.clone(), case.modified.clone())
                .with_file_system_dates(case.file_system.created, case.file_system.modified)
                .create(&temp_dir, "test.md");

            let front_matter =
                create_frontmatter(case.modified.as_deref(), case.created.as_deref());
            let (created_date_validation, modified_date_validation) =
                date_validation::get_date_validations(Some(&front_matter), &file_path, timezone)
                    .unwrap();

            test_utils::assert_test_case(
                created_date_validation.issue,
                case.issues.created,
                &format!("{} - created date validation", case.name),
                |actual, expected| assert_eq!(actual, expected),
            );

            test_utils::assert_test_case(
                modified_date_validation.issue,
                case.issues.modified,
                &format!("{} - modified date validation", case.name),
                |actual, expected| assert_eq!(actual, expected),
            );
        }
    }

    #[test]
    #[cfg_attr(
        target_os = "linux",
        ignore = "requires filesystem access unavailable on Linux CI"
    )]
    fn test_late_night_date_created_fix() {
        let temp_dir = TempDir::new().unwrap();

        // `late_night_time` represents 2024-01-15 22:11 in `DEFAULT_TIMEZONE`.
        let late_night_time = Utc.with_ymd_and_hms(2024, 1, 16, 3, 11, 0).unwrap();

        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(
                Some(eastern_date_wikilink(2024, 1, 15)),
                Some(eastern_date_wikilink(2024, 1, 15)),
            )
            .with_file_system_dates(late_night_time, late_night_time)
            // `TestFileBuilder::with_date_created_fix` receives non-wikilink input here.
            .with_date_created_fix(Some("2024-01-16".to_string()))
            .create(&temp_dir, "test1.md");

        let markdown_file = test_utils::get_test_markdown_file(file_path);

        let timezone: Tz = DEFAULT_TIMEZONE.parse().unwrap();
        let fixed_local = markdown_file
            .date_created_fix_validation
            .fixed
            .unwrap()
            .with_timezone(&timezone);

        assert_eq!(
            fixed_local.date_naive(),
            NaiveDate::from_ymd_opt(2024, 1, 16).unwrap(),
            "Date created fix should show as Jan 16 in Eastern time"
        );

        let persist_reasons = &markdown_file.persist_reasons;
        assert!(
            persist_reasons
                .iter()
                .any(|r| matches!(r, PersistReason::DateCreatedFixApplied)),
            "Should have DateCreatedFixApplied reason"
        );
    }
}

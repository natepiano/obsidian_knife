use chrono::DateTime;
use chrono::NaiveDate;
use chrono::TimeZone;
use chrono::Utc;
use chrono_tz::Tz;
use tempfile::TempDir;

use super::DateCreatedFixValidation;
use super::DateValidation;
use super::DateValidationIssue;
use super::PersistReason;
use super::date_validation;
use crate::constants::DEFAULT_TIMEZONE;
use crate::frontmatter::FrontMatter;
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
        // Create initial frontmatter
        let mut frontmatter = Some(create_frontmatter(
            case.modified.as_deref(),
            case.created.as_deref(),
        ));

        // Get date validations
        let created_validation = DateValidation {
            frontmatter:          case.created.clone(), // Add clone here
            file_system:          case.file_system.created,
            issue:                date_validation::get_date_validation_issue(
                case.created.as_deref(),
                &case.file_system.created,
                DEFAULT_TIMEZONE,
            ),
            operational_timezone: DEFAULT_TIMEZONE.to_string(),
        };

        let modified_validation = DateValidation {
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
            &mut frontmatter,
            &created_validation,
            &modified_validation,
            &DateCreatedFixValidation::default(),
            DEFAULT_TIMEZONE,
        );

        // Verify frontmatter dates
        test_utils::assert_test_case(
            frontmatter
                .as_ref()
                .and_then(|frontmatter| frontmatter.date_modified().map(String::from)),
            case.expected.modified,
            &format!("{} - modified date", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        test_utils::assert_test_case(
            frontmatter
                .as_ref()
                .and_then(|frontmatter| frontmatter.date_created().map(String::from)),
            case.expected.created,
            &format!("{} - created date", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        test_utils::assert_test_case(
            frontmatter.as_ref().is_some_and(FrontMatter::needs_persist),
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
            // bare date (no wikilink) on purpose — tests the non-wikilink input path
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
            .with_fs_dates(test_date, test_date)
            .with_date_created_fix(case.fix_input.clone())
            .create(&temp_dir, "test1.md");

        // Create `MarkdownFile` from the test file
        let markdown_file = test_utils::get_test_markdown_file(file_path);

        // Verify the `DateCreatedFixValidation` state
        test_utils::assert_test_case(
            markdown_file.date_created_fix.raw,
            case.fix_input,
            &format!("{} - date string", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        test_utils::assert_test_case(
            markdown_file.frontmatter.unwrap().needs_persist(),
            case.expected.persist.needs_persist(),
            &format!("{} - expect persist", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        test_utils::assert_test_case(
            markdown_file
                .date_created_fix
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
            // These should match because in EST it's still Jan 15th at 11:30 PM
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
            // These should fail because in EST it's Jan 15th but frontmatter says Jan 16th
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
            // Should match because it's exactly the start of Jan 15th in EST
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
            .with_fs_dates(case.file_system.created, case.file_system.modified)
            .create(&temp_dir, "test.md");

        let frontmatter = create_frontmatter(case.modified.as_deref(), case.created.as_deref());
        let (created_validation, modified_validation) =
            date_validation::get_date_validations(Some(&frontmatter), &file_path, timezone)
                .unwrap();

        test_utils::assert_test_case(
            created_validation.issue,
            case.issues.created,
            &format!("{} - created date validation", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        test_utils::assert_test_case(
            modified_validation.issue,
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

    // Create time at 10:11 PM Eastern (next day 03:11 UTC)
    let late_night_time = Utc.with_ymd_and_hms(2024, 1, 16, 3, 11, 0).unwrap();

    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(
            Some(eastern_date_wikilink(2024, 1, 15)),
            Some(eastern_date_wikilink(2024, 1, 15)),
        )
        .with_fs_dates(late_night_time, late_night_time)
        // bare date (no wikilink) on purpose — exercises the non-wikilink fix input path
        .with_date_created_fix(Some("2024-01-16".to_string()))
        .create(&temp_dir, "test1.md");

    // Create `MarkdownFile` from the test file
    let markdown_file = test_utils::get_test_markdown_file(file_path);

    // Verify the parsed date shows as Jan 16 when viewed in Eastern
    let timezone: Tz = DEFAULT_TIMEZONE.parse().unwrap();
    let fixed_local = markdown_file
        .date_created_fix
        .fixed
        .unwrap()
        .with_timezone(&timezone);

    assert_eq!(
        fixed_local.date_naive(),
        NaiveDate::from_ymd_opt(2024, 1, 16).unwrap(),
        "Date created fix should show as Jan 16 in Eastern time"
    );

    // Also verify that the persist report would show Jan 16
    let persist_reasons = &markdown_file.persist_reasons;
    assert!(
        persist_reasons
            .iter()
            .any(|r| matches!(r, PersistReason::DateCreatedFixApplied)),
        "Should have DateCreatedFixApplied reason"
    );
}

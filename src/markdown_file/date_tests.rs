use super::*;
use crate::frontmatter::FrontMatter;

use crate::test_utils;
use crate::test_utils::TestFileBuilder;
use crate::yaml_frontmatter::YamlFrontMatter;
use crate::DEFAULT_TIMEZONE;
use chrono::TimeZone;
use tempfile::TempDir;

// into_iter() consumes the array and yields owned values
// filter_map filters out none values and unwraps Some values in one step
fn create_frontmatter(
    date_modified: &Option<String>,
    date_created: &Option<String>,
) -> FrontMatter {
    let yaml = [
        date_modified
            .as_ref()
            .map(|modified| format!("date_modified: \"{}\"", modified)),
        date_created
            .as_ref()
            .map(|created| format!("date_created: \"{}\"", created)),
    ]
    .into_iter()
    .filter_map(|part| part)
    .collect::<Vec<_>>()
    .join("\n");

    FrontMatter::from_yaml_str(&yaml).unwrap()
}

// Main test case struct for validating dates
struct DateValidationTestCase {
    name: &'static str,
    date_modified: Option<String>,
    date_created: Option<String>,
    file_system_mod_date: DateTime<Utc>,
    file_system_create_date: DateTime<Utc>,
    expected_modified_issue: Option<DateValidationIssue>,
    expected_created_issue: Option<DateValidationIssue>,
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_process_frontmatter_date_validation() {
    let test_cases = vec![
        DateValidationTestCase {
            name: "both dates valid and matching filesystem",
            date_modified: Some("[[2024-01-15]]".to_string()),
            date_created: Some("[[2024-01-15]]".to_string()),
            file_system_mod_date: test_utils::eastern_midnight(2024, 1, 15),
            file_system_create_date: test_utils::eastern_midnight(2024, 1, 15),
            expected_modified_issue: None,
            expected_created_issue: None,
        },
        DateValidationTestCase {
            name: "missing wikilink brackets",
            date_modified: Some("2024-01-15".to_string()),
            date_created: Some("2024-01-15".to_string()),
            file_system_mod_date: test_utils::eastern_midnight(2024, 1, 15),
            file_system_create_date: test_utils::eastern_midnight(2024, 1, 15),
            expected_modified_issue: Some(DateValidationIssue::InvalidWikilink),
            expected_created_issue: Some(DateValidationIssue::InvalidWikilink),
        },
        DateValidationTestCase {
            name: "filesystem mismatch",
            date_modified: Some("[[2024-01-15]]".to_string()),
            date_created: Some("[[2024-01-15]]".to_string()),
            file_system_mod_date: test_utils::eastern_midnight(2024, 1, 16),
            file_system_create_date: test_utils::eastern_midnight(2024, 1, 16),
            expected_modified_issue: Some(DateValidationIssue::FileSystemMismatch),
            expected_created_issue: Some(DateValidationIssue::FileSystemMismatch),
        },
        DateValidationTestCase {
            name: "invalid date format",
            date_modified: Some("[[2024-13-45]]".to_string()),
            date_created: Some("[[2024-13-45]]".to_string()),
            file_system_mod_date: Utc::now(),
            file_system_create_date: Utc::now(),
            expected_modified_issue: Some(DateValidationIssue::InvalidDateFormat),
            expected_created_issue: Some(DateValidationIssue::InvalidDateFormat),
        },
        DateValidationTestCase {
            name: "missing dates",
            date_modified: None,
            date_created: None,
            file_system_mod_date: Utc::now(),
            file_system_create_date: Utc::now(),
            expected_modified_issue: Some(DateValidationIssue::Missing),
            expected_created_issue: Some(DateValidationIssue::Missing),
        },
    ];

    run_date_validation_test_cases(test_cases, DEFAULT_TIMEZONE);
}

struct DateFixTestCase {
    name: &'static str,
    // Initial state
    date_modified: Option<String>,
    date_created: Option<String>,
    file_system_mod_date: DateTime<Utc>,
    file_system_create_date: DateTime<Utc>,

    // Expected outcomes
    should_persist: bool,
    expected_modified_date: Option<String>, // The expected frontmatter date after processing
    expected_created_date: Option<String>,  // The expected frontmatter date after processing
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_process_date_validations() {
    let test_cases = vec![
        DateFixTestCase {
            name: "missing dates should be updated",
            date_modified: None,
            date_created: None,
            file_system_mod_date: test_utils::eastern_midnight(2024, 1, 15),
            file_system_create_date: test_utils::eastern_midnight(2024, 1, 15),
            expected_modified_date: Some("[[2024-01-15]]".to_string()),
            expected_created_date: Some("[[2024-01-15]]".to_string()),
            should_persist: true,
        },
        DateFixTestCase {
            name: "filesystem mismatch should update modified date",
            date_modified: Some("[[2024-01-14]]".to_string()),
            date_created: Some("[[2024-01-15]]".to_string()),
            file_system_mod_date: test_utils::eastern_midnight(2024, 1, 15),
            file_system_create_date: test_utils::eastern_midnight(2024, 1, 15),
            expected_modified_date: Some("[[2024-01-15]]".to_string()),
            expected_created_date: Some("[[2024-01-15]]".to_string()),
            should_persist: true,
        },
        DateFixTestCase {
            name: "valid dates should not change",
            date_modified: Some("[[2024-01-15]]".to_string()),
            date_created: Some("[[2024-01-15]]".to_string()),
            file_system_mod_date: test_utils::eastern_midnight(2024, 1, 15),
            file_system_create_date: test_utils::eastern_midnight(2024, 1, 15),
            expected_modified_date: Some("[[2024-01-15]]".to_string()),
            expected_created_date: Some("[[2024-01-15]]".to_string()),
            should_persist: false,
        },
        DateFixTestCase {
            name: "filesystem mismatch should update both dates",
            date_modified: Some("[[2024-01-14]]".to_string()), // original frontmatter dates
            date_created: Some("[[2024-01-13]]".to_string()),  // that don't match filesystem
            // Using 05:00 UTC (midnight Eastern) ensures dates like "[[2024-01-15]]" match
            // the filesystem dates when viewed in Eastern timezone
            file_system_mod_date: test_utils::eastern_midnight(2024, 1, 15),
            file_system_create_date: test_utils::eastern_midnight(2024, 1, 15),
            expected_modified_date: Some("[[2024-01-15]]".to_string()),
            expected_created_date: Some("[[2024-01-15]]".to_string()),
            should_persist: true,
        },
        DateFixTestCase {
            name: "invalid format should change", // changed from "invalid format should not change"
            date_modified: Some("[[2024-13-45]]".to_string()),
            date_created: Some("[[2024-13-45]]".to_string()),
            file_system_mod_date: test_utils::eastern_midnight(2024, 1, 15),
            file_system_create_date: test_utils::eastern_midnight(2024, 1, 15),
            expected_modified_date: Some("[[2024-01-15]]".to_string()), // changed
            expected_created_date: Some("[[2024-01-15]]".to_string()),  // changed
            should_persist: true,                                       // changed from false
        },
        DateFixTestCase {
            name: "invalid wikilink should change", // changed from "invalid wikilink should not change"
            date_modified: Some("2024-01-15".to_string()),
            date_created: Some("2024-01-15".to_string()),
            file_system_mod_date: test_utils::eastern_midnight(2024, 1, 15),
            file_system_create_date: test_utils::eastern_midnight(2024, 1, 15),
            expected_modified_date: Some("[[2024-01-15]]".to_string()), // changed
            expected_created_date: Some("[[2024-01-15]]".to_string()),  // changed
            should_persist: true,                                       // changed from false
        },
    ];

    for case in test_cases {
        // Create initial frontmatter
        let mut frontmatter = Some(create_frontmatter(&case.date_modified, &case.date_created));

        // Get date validations
        let created_validation = DateValidation {
            frontmatter_date: case.date_created.clone(), // Add clone here
            file_system_date: case.file_system_create_date,
            issue: get_date_validation_issue(
                case.date_created.as_ref(),
                &case.file_system_create_date,
                DEFAULT_TIMEZONE,
            ),
            operational_timezone: DEFAULT_TIMEZONE.to_string(),
        };

        let modified_validation = DateValidation {
            frontmatter_date: case.date_modified.clone(), // Add clone here
            file_system_date: case.file_system_mod_date,
            issue: get_date_validation_issue(
                case.date_modified.as_ref(),
                &case.file_system_mod_date,
                DEFAULT_TIMEZONE,
            ),
            operational_timezone: DEFAULT_TIMEZONE.to_string(),
        };

        // Process validations
        process_date_validations(
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
                .and_then(|fm| fm.date_modified().cloned()),
            case.expected_modified_date,
            &format!("{} - modified date", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        test_utils::assert_test_case(
            frontmatter
                .as_ref()
                .and_then(|fm| fm.date_created().cloned()),
            case.expected_created_date,
            &format!("{} - created date", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        test_utils::assert_test_case(
            frontmatter
                .as_ref()
                .map(|fm| fm.needs_persist())
                .unwrap_or(false),
            case.should_persist,
            &format!("{} - needs persist flag", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );
    }
}

struct DateCreatedFixTestCase {
    name: &'static str,
    date_created_fix: Option<String>,
    expect_persist: bool,
    expected_parsed_date: Option<DateTime<Utc>>,
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_date_created_fix_integration() {
    let test_cases = vec![
        DateCreatedFixTestCase {
            name: "missing date_created_fix",
            date_created_fix: None,
            expect_persist: false,
            expected_parsed_date: None,
        },
        DateCreatedFixTestCase {
            name: "valid date without wikilink",
            date_created_fix: Some("2024-01-15".to_string()),
            expect_persist: true,
            expected_parsed_date: Some(test_utils::eastern_midnight(2024, 1, 15)),
        },
        DateCreatedFixTestCase {
            name: "valid date with wikilink",
            date_created_fix: Some("[[2024-01-15]]".to_string()),
            expect_persist: true,
            expected_parsed_date: Some(test_utils::eastern_midnight(2024, 1, 15)),
        },
        DateCreatedFixTestCase {
            name: "invalid date format",
            date_created_fix: Some("2024-13-45".to_string()),
            expect_persist: false,
            expected_parsed_date: None,
        },
        DateCreatedFixTestCase {
            name: "invalid date with wikilink",
            date_created_fix: Some("[[2024-13-45]]".to_string()),
            expect_persist: false,
            expected_parsed_date: None,
        },
        DateCreatedFixTestCase {
            name: "malformed wikilink",
            date_created_fix: Some("[2024-01-15]".to_string()),
            expect_persist: false,
            expected_parsed_date: None,
        },
    ];

    for case in test_cases {
        let temp_dir = TempDir::new().unwrap();

        // Using 05:00 UTC (midnight Eastern) ensures the date in Eastern timezone
        // matches the frontmatter date, preventing FileSystemMismatch errors
        let test_date = test_utils::eastern_midnight(2024, 1, 15);
        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(
                Some("[[2024-01-15]]".to_string()),
                Some("[[2024-01-15]]".to_string()),
            )
            .with_fs_dates(test_date, test_date)
            .with_date_created_fix(case.date_created_fix.clone())
            .create(&temp_dir, "test1.md");

        // Create MarkdownFile from the test file
        let markdown_info = test_utils::get_test_markdown_file(file_path);

        // Verify the DateCreatedFixValidation state
        test_utils::assert_test_case(
            markdown_info.date_created_fix.date_string,
            case.date_created_fix,
            &format!("{} - date string", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        test_utils::assert_test_case(
            markdown_info.frontmatter.unwrap().needs_persist(),
            case.expect_persist,
            &format!("{} - expect persist", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        test_utils::assert_test_case(
            markdown_info
                .date_created_fix
                .fix_date
                .map(|dt| dt.date_naive()),
            case.expected_parsed_date.map(|dt| dt.date_naive()),
            &format!("{} - parsed date", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );
    }
}

#[test]
#[cfg_attr(target_os = "linux", ignore)]
fn test_timezone_date_validation() {
    let test_cases = vec![
        DateValidationTestCase {
            name: "late night eastern time should match UTC next day",
            date_modified: Some("[[2024-01-15]]".to_string()),
            date_created: Some("[[2024-01-15]]".to_string()),
            // This represents 11:30 PM EST on Jan 15th (4:30 AM UTC Jan 16th)
            file_system_mod_date: Utc.with_ymd_and_hms(2024, 1, 16, 4, 30, 0).unwrap(),
            file_system_create_date: Utc.with_ymd_and_hms(2024, 1, 16, 4, 30, 0).unwrap(),
            // These should match because in EST it's still Jan 15th at 11:30 PM
            expected_modified_issue: None,
            expected_created_issue: None,
        },
        DateValidationTestCase {
            name: "early morning eastern time should match UTC previous day",
            date_modified: Some("[[2024-01-16]]".to_string()),
            date_created: Some("[[2024-01-16]]".to_string()),
            // This represents 2:30 AM EST Jan 15th (7:30 AM UTC Jan 15th)
            file_system_mod_date: Utc.with_ymd_and_hms(2024, 1, 15, 7, 30, 0).unwrap(),
            file_system_create_date: Utc.with_ymd_and_hms(2024, 1, 15, 7, 30, 0).unwrap(),
            // These should fail because in EST it's Jan 15th but frontmatter says Jan 16th
            expected_modified_issue: Some(DateValidationIssue::FileSystemMismatch),
            expected_created_issue: Some(DateValidationIssue::FileSystemMismatch),
        },
        DateValidationTestCase {
            name: "eastern midnight boundary case",
            date_modified: Some("[[2024-01-15]]".to_string()),
            date_created: Some("[[2024-01-15]]".to_string()),
            // This represents exactly midnight EST Jan 15th (5 AM UTC Jan 15th)
            file_system_mod_date: Utc.with_ymd_and_hms(2024, 1, 15, 5, 0, 0).unwrap(),
            file_system_create_date: Utc.with_ymd_and_hms(2024, 1, 15, 5, 0, 0).unwrap(),
            // Should match because it's exactly the start of Jan 15th in EST
            expected_modified_issue: None,
            expected_created_issue: None,
        },
    ];

    run_date_validation_test_cases(test_cases, DEFAULT_TIMEZONE);
}

fn run_date_validation_test_cases(test_cases: Vec<DateValidationTestCase>, timezone: &str) {
    for case in test_cases {
        let temp_dir = TempDir::new().unwrap();
        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(case.date_created.clone(), case.date_modified.clone())
            .with_fs_dates(case.file_system_create_date, case.file_system_mod_date)
            .create(&temp_dir, "test.md");

        let fm = create_frontmatter(&case.date_modified, &case.date_created);
        let (created_validation, modified_validation) =
            get_date_validations(&Some(fm), &file_path, timezone).unwrap();

        test_utils::assert_test_case(
            created_validation.issue,
            case.expected_created_issue,
            &format!("{} - created date validation", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        test_utils::assert_test_case(
            modified_validation.issue,
            case.expected_modified_issue,
            &format!("{} - modified date validation", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );
    }
}

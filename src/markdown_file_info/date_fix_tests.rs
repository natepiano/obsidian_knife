use std::{fs::File, io::Write};
use chrono::TimeZone;
use super::*;
use crate::frontmatter::FrontMatter;
use crate::test_utils::assert_test_case;
use crate::yaml_frontmatter::YamlFrontMatter;

fn create_frontmatter(
    date_modified: &Option<String>,
    date_created: &Option<String>,
    needs_persist: bool,
    needs_filesystem_update: Option<String>,
) -> FrontMatter {
    let mut yaml_parts = vec!["---".to_string()];

    if let Some(modified) = date_modified {
        let modified_str = format!("date_modified: \"{}\"", modified);
        yaml_parts.push(modified_str);
    }
    if let Some(created) = date_created {
        let created_str = format!("date_created: \"{}\"", created);
        yaml_parts.push(created_str);
    }
    if yaml_parts.len() == 1 {
        yaml_parts.push("title: test".to_string());
    }
    yaml_parts.push("---\n".to_string());

    let yaml = yaml_parts.join("\n");

    let mut fm = FrontMatter::from_markdown_str(&yaml).unwrap();
    fm.set_needs_persist(needs_persist);
    fm.set_needs_filesystem_update(needs_filesystem_update);
    fm
}

// Test case struct for date modification
struct DateModTestCase {
    name: &'static str,
    initial_date: Option<String>,
    expected_format: bool,
    should_persist: bool,
}

// Main test case struct for validating dates
struct DateValidationTestCase {
    name: &'static str,
    date_modified: Option<String>,
    date_created: Option<String>,
    file_system_mod_date: DateTime<Local>,
    file_system_create_date: DateTime<Local>,
    expected_modified_status: DateValidationStatus,
    expected_created_status: DateValidationStatus,
}

// Test case specifically for report string formatting
struct ReportStringTestCase {
    name: &'static str,
    frontmatter_date: Option<String>,
    file_system_date: DateTime<Local>,
    expected_status: DateValidationStatus,
    expected_report: &'static str,
}

#[test]
fn test_process_frontmatter_date_validation() {
    use tempfile::TempDir;

    let test_cases = vec![
        DateValidationTestCase {
            name: "both dates valid and matching filesystem",
            date_modified: Some("[[2024-01-15]]".to_string()),
            date_created: Some("[[2024-01-15]]".to_string()),
            file_system_mod_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_status: DateValidationStatus::Valid,
            expected_created_status: DateValidationStatus::Valid,
        },
        DateValidationTestCase {
            name: "missing wikilink brackets",
            date_modified: Some("2024-01-15".to_string()),
            date_created: Some("2024-01-15".to_string()),
            file_system_mod_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_status: DateValidationStatus::InvalidWikilink,
            expected_created_status: DateValidationStatus::InvalidWikilink,
        },
        DateValidationTestCase {
            name: "filesystem mismatch",
            date_modified: Some("[[2024-01-15]]".to_string()),
            date_created: Some("[[2024-01-15]]".to_string()),
            file_system_mod_date: Local.with_ymd_and_hms(2024, 1, 16, 0, 0, 0).unwrap(),
            file_system_create_date: Local.with_ymd_and_hms(2024, 1, 16, 0, 0, 0).unwrap(),
            expected_modified_status: DateValidationStatus::FileSystemMismatch,
            expected_created_status: DateValidationStatus::FileSystemMismatch,
        },
        DateValidationTestCase {
            name: "invalid date format",
            date_modified: Some("[[2024-13-45]]".to_string()),
            date_created: Some("[[2024-13-45]]".to_string()),
            file_system_mod_date: Local::now(),
            file_system_create_date: Local::now(),
            expected_modified_status: DateValidationStatus::InvalidFormat,
            expected_created_status: DateValidationStatus::InvalidFormat,
        },
        DateValidationTestCase {
            name: "missing dates",
            date_modified: None,
            date_created: None,
            file_system_mod_date: Local::now(),
            file_system_create_date: Local::now(),
            expected_modified_status: DateValidationStatus::Missing,
            expected_created_status: DateValidationStatus::Missing,
        },
    ];

    for case in test_cases {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create test file with frontmatter
        let fm = create_frontmatter(&case.date_modified, &case.date_created, false, None);

        // Create test file with frontmatter content
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "---").unwrap();
        if let Some(date) = fm.date_modified() {
            writeln!(file, "date_modified: \"{}\"", date).unwrap();
        }
        if let Some(date) = fm.date_created() {
            writeln!(file, "date_created: \"{}\"", date).unwrap();
        }
        writeln!(file, "---").unwrap();
        writeln!(file, "Test content").unwrap();

        // Set file timestamps to match test case
        let filetime = filetime::FileTime::from_system_time(case.file_system_create_date.into());
        filetime::set_file_times(&file_path, filetime, filetime).unwrap();

        // Mock filesystem dates using the provided test case dates
        let (created_validation, modified_validation) =
            get_date_validations(&Some(fm), &file_path).unwrap();

        assert_test_case(
            created_validation.status,
            case.expected_created_status,
            &format!("{} - created date validation", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        assert_test_case(
            modified_validation.status,
            case.expected_modified_status,
            &format!("{} - modified date validation", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );
    }
}

#[test]
fn test_date_validation_report_strings() {
    let test_cases = vec![
        ReportStringTestCase {
            name: "missing date",
            frontmatter_date: None,
            file_system_date: Local::now(),
            expected_status: DateValidationStatus::Missing,
            expected_report: "missing",
        },
        ReportStringTestCase {
            name: "invalid format",
            frontmatter_date: Some("[[2024-13-45]]".to_string()),
            file_system_date: Local::now(),
            expected_status: DateValidationStatus::InvalidFormat,
            expected_report: "invalid date format: '[[2024-13-45]]'",
        },
        ReportStringTestCase {
            name: "missing wikilink",
            frontmatter_date: Some("2024-01-15".to_string()),
            file_system_date: Local::now(),
            expected_status: DateValidationStatus::InvalidWikilink,
            expected_report: "missing wikilink: '2024-01-15'",
        },
        ReportStringTestCase {
            name: "filesystem mismatch",
            frontmatter_date: Some("[[2024-01-15]]".to_string()),
            file_system_date: Local.with_ymd_and_hms(2024, 1, 16, 0, 0, 0).unwrap(),
            expected_status: DateValidationStatus::FileSystemMismatch,
            expected_report: "modified date mismatch: frontmatter='[[2024-01-15]]', filesystem='2024-01-16'",
        },
        ReportStringTestCase {
            name: "valid date",
            frontmatter_date: Some("[[2024-01-15]]".to_string()),
            file_system_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_status: DateValidationStatus::Valid,
            expected_report: "valid",
        },
    ];

    for case in test_cases {
        let validation = DateValidation {
            frontmatter_date: case.frontmatter_date,
            file_system_date: case.file_system_date,
            status: case.expected_status,
        };

        assert_test_case(
            validation.to_report_string(),
            case.expected_report.to_string(),
            case.name,
            |actual, expected| assert_eq!(actual, expected),
        );
    }
}

use std::{fs::File, io::Write};
use chrono::TimeZone;
use tempfile::TempDir;
use super::*;
use crate::frontmatter::FrontMatter;
use crate::test_utils::assert_test_case;
use crate::yaml_frontmatter::YamlFrontMatter;

fn create_frontmatter(
    date_modified: &Option<String>,
    date_created: &Option<String>,
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

    FrontMatter::from_markdown_str(&yaml).unwrap()
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
        let fm = create_frontmatter(&case.date_modified, &case.date_created);

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
        // Set file timestamps to the respective dates
        let create_time = filetime::FileTime::from_system_time(case.file_system_create_date.into());
        let mod_time = filetime::FileTime::from_system_time(case.file_system_mod_date.into());
        filetime::set_file_times(&file_path, create_time, mod_time).unwrap();

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


// Test case specifically for report string formatting
struct ReportStringTestCase {
    name: &'static str,
    frontmatter_date: Option<String>,
    file_system_date: DateTime<Local>,
    expected_status: DateValidationStatus,
    expected_report: &'static str,
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

struct DateFixTestCase {
    name: &'static str,
    // Initial state
    date_modified: Option<String>,
    date_created: Option<String>,
    file_system_mod_date: DateTime<Local>,
    file_system_create_date: DateTime<Local>,

    // Expected outcomes
    should_persist: bool,
    expected_modified_date: Option<String>,  // The expected frontmatter date after processing
    expected_created_date: Option<String>,   // The expected frontmatter date after processing
}

#[test]
fn test_process_date_validations() {
    let test_cases = vec![
        DateFixTestCase {
            name: "missing dates should be updated",
            date_modified: None,
            date_created: None,
            file_system_mod_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_date: Some("[[2024-01-15]]".to_string()),
            expected_created_date: Some("[[2024-01-15]]".to_string()),
            should_persist: true,
        },
        DateFixTestCase {
            name: "filesystem mismatch should update modified date only",
            date_modified: Some("[[2024-01-14]]".to_string()),
            date_created: Some("[[2024-01-15]]".to_string()),
            file_system_mod_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_date: Some("[[2024-01-15]]".to_string()),
            expected_created_date: Some("[[2024-01-15]]".to_string()),
            should_persist: true,
        },
        DateFixTestCase {
            name: "valid dates should not change",
            date_modified: Some("[[2024-01-15]]".to_string()),
            date_created: Some("[[2024-01-15]]".to_string()),
            file_system_mod_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_date: Some("[[2024-01-15]]".to_string()),
            expected_created_date: Some("[[2024-01-15]]".to_string()),
            should_persist: false,
        },
        DateFixTestCase {
            name: "invalid format should not change",
            date_modified: Some("[[2024-13-45]]".to_string()),
            date_created: Some("[[2024-13-45]]".to_string()),
            file_system_mod_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_date: Some("[[2024-13-45]]".to_string()),
            expected_created_date: Some("[[2024-13-45]]".to_string()),
            should_persist: false,
        },
        DateFixTestCase {
            name: "invalid wikilink should not change",
            date_modified: Some("2024-01-15".to_string()),
            date_created: Some("2024-01-15".to_string()),
            file_system_mod_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_date: Some("2024-01-15".to_string()),
            expected_created_date: Some("2024-01-15".to_string()),
            should_persist: false,
        },
    ];

    for case in test_cases {
        // Create initial frontmatter
        let mut frontmatter = Some(create_frontmatter(&case.date_modified, &case.date_created));

        // Get date validations
        let created_validation = DateValidation {
            frontmatter_date: case.date_created.clone(), // Add clone here
            file_system_date: case.file_system_create_date,
            status: get_date_validation_status(case.date_created.as_ref(), &case.file_system_create_date),
        };

        let modified_validation = DateValidation {
            frontmatter_date: case.date_modified.clone(), // Add clone here
            file_system_date: case.file_system_mod_date,
            status: get_date_validation_status(case.date_modified.as_ref(), &case.file_system_mod_date),
        };

        // Process validations
        process_date_validations(&mut frontmatter, &created_validation, &modified_validation);

        // Verify frontmatter dates
        assert_test_case(
            frontmatter.as_ref().and_then(|fm| fm.date_modified().cloned()),
            case.expected_modified_date,
            &format!("{} - modified date", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        assert_test_case(
            frontmatter.as_ref().and_then(|fm| fm.date_created().cloned()),
            case.expected_created_date,
            &format!("{} - created date", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        assert_test_case(
            frontmatter.as_ref().map(|fm| fm.needs_persist()).unwrap_or(false),
            case.should_persist,
            &format!("{} - needs persist flag", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );
    }
}

struct DateCreatedFixTestCase {
    name: &'static str,
    date_created_fix: Option<String>,
    expected_needs_fix: bool,
    expected_parsed_date: Option<DateTime<Local>>,
}

fn create_test_file(temp_dir: &TempDir, date_created_fix: Option<&str>) -> PathBuf {
    let file_path = temp_dir.path().join("test.md");
    let mut file = File::create(&file_path).unwrap();

    // Write frontmatter
    writeln!(file, "---").unwrap();
    if let Some(date) = date_created_fix {
        writeln!(file, "date_created_fix: \"{}\"", date).unwrap();
    }
    writeln!(file, "title: test").unwrap();
    writeln!(file, "---").unwrap();
    writeln!(file, "Test content").unwrap();

    file_path
}

#[test]
fn test_date_created_fix_integration() {
    let test_cases = vec![
        DateCreatedFixTestCase {
            name: "missing date_created_fix",
            date_created_fix: None,
            expected_needs_fix: false,
            expected_parsed_date: None,
        },
        DateCreatedFixTestCase {
            name: "valid date without wikilink",
            date_created_fix: Some("2024-01-15".to_string()),
            expected_needs_fix: true,
            expected_parsed_date: Some(
                Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap()
            ),
        },
        DateCreatedFixTestCase {
            name: "valid date with wikilink",
            date_created_fix: Some("[[2024-01-15]]".to_string()),
            expected_needs_fix: true,
            expected_parsed_date: Some(
                Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap()
            ),
        },
        DateCreatedFixTestCase {
            name: "invalid date format",
            date_created_fix: Some("2024-13-45".to_string()),
            expected_needs_fix: false,
            expected_parsed_date: None,
        },
        DateCreatedFixTestCase {
            name: "invalid date with wikilink",
            date_created_fix: Some("[[2024-13-45]]".to_string()),
            expected_needs_fix: false,
            expected_parsed_date: None,
        },
        DateCreatedFixTestCase {
            name: "malformed wikilink",
            date_created_fix: Some("[2024-01-15]".to_string()),
            expected_needs_fix: false,
            expected_parsed_date: None,
        },
    ];

    for case in test_cases {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_test_file(&temp_dir, case.date_created_fix.as_deref());

        // Create MarkdownFileInfo from the test file
        let markdown_info = MarkdownFileInfo::new(file_path).unwrap();

        // Verify the DateCreatedFixValidation state
        assert_test_case(
            markdown_info.date_created_fix.date_string,
            case.date_created_fix,
            &format!("{} - date string", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        assert_test_case(
            markdown_info.date_created_fix.parsed_date,
            case.expected_parsed_date,
            &format!("{} - parsed date", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        // Verify the frontmatter needs_create_date_fix state
        if let Some(frontmatter) = markdown_info.frontmatter {
            assert_test_case(
                frontmatter.needs_create_date_fix(),
                case.expected_needs_fix,
                &format!("{} - needs create date fix", case.name),
                |actual, expected| assert_eq!(actual, expected),
            );
        }
    }
}

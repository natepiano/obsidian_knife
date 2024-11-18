use super::*;
use crate::frontmatter::FrontMatter;
use crate::test_utils::{assert_test_case, TestFileBuilder};
use crate::yaml_frontmatter::YamlFrontMatter;
use chrono::TimeZone;
use tempfile::TempDir;

fn create_frontmatter(
    date_modified: &Option<String>,
    date_created: &Option<String>,
) -> FrontMatter {
    let mut yaml_parts = vec![];

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
  //  yaml_parts.push("---\n".to_string());

    let yaml = yaml_parts.join("\n");

    FrontMatter::from_yaml_str(&yaml).unwrap()
}

// Main test case struct for validating dates
struct DateValidationTestCase {
    name: &'static str,
    date_modified: Option<String>,
    date_created: Option<String>,
    file_system_mod_date: DateTime<Utc>,
    file_system_create_date: DateTime<Utc>,
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
            file_system_mod_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_status: DateValidationStatus::Valid,
            expected_created_status: DateValidationStatus::Valid,
        },
        DateValidationTestCase {
            name: "missing wikilink brackets",
            date_modified: Some("2024-01-15".to_string()),
            date_created: Some("2024-01-15".to_string()),
            file_system_mod_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_status: DateValidationStatus::InvalidWikilink,
            expected_created_status: DateValidationStatus::InvalidWikilink,
        },
        DateValidationTestCase {
            name: "filesystem mismatch",
            date_modified: Some("[[2024-01-15]]".to_string()),
            date_created: Some("[[2024-01-15]]".to_string()),
            file_system_mod_date: Utc.with_ymd_and_hms(2024, 1, 16, 0, 0, 0).unwrap(),
            file_system_create_date: Utc.with_ymd_and_hms(2024, 1, 16, 0, 0, 0).unwrap(),
            expected_modified_status: DateValidationStatus::FileSystemMismatch,
            expected_created_status: DateValidationStatus::FileSystemMismatch,
        },
        DateValidationTestCase {
            name: "invalid date format",
            date_modified: Some("[[2024-13-45]]".to_string()),
            date_created: Some("[[2024-13-45]]".to_string()),
            file_system_mod_date: Utc::now(),
            file_system_create_date: Utc::now(),
            expected_modified_status: DateValidationStatus::InvalidFormat,
            expected_created_status: DateValidationStatus::InvalidFormat,
        },
        DateValidationTestCase {
            name: "missing dates",
            date_modified: None,
            date_created: None,
            file_system_mod_date: Utc::now(),
            file_system_create_date: Utc::now(),
            expected_modified_status: DateValidationStatus::Missing,
            expected_created_status: DateValidationStatus::Missing,
        },
    ];

    for case in test_cases {
        let temp_dir = TempDir::new().unwrap();
        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(case.date_created.clone(), case.date_modified.clone())
            .with_fs_dates(case.file_system_create_date, case.file_system_mod_date)
            .create(&temp_dir, "test.md");

        // we're creating an in memory FrontMatter that matches what we wrote out to the file
        let fm = create_frontmatter(&case.date_modified, &case.date_created);

        // get_date_validations will make sure they all match up as per the test cases
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
fn test_process_date_validations() {
    let test_cases = vec![
        DateFixTestCase {
            name: "missing dates should be updated",
            date_modified: None,
            date_created: None,
            file_system_mod_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_date: Some("[[2024-01-15]]".to_string()),
            expected_created_date: Some("[[2024-01-15]]".to_string()),
            should_persist: true,
        },
        DateFixTestCase {
            name: "filesystem mismatch should update created date",
            date_modified: Some("[[2024-01-15]]".to_string()),
            date_created: Some("[[2024-01-14]]".to_string()),
            file_system_mod_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_date: Some("[[2024-01-15]]".to_string()),
            expected_created_date: Some("[[2024-01-15]]".to_string()),
            should_persist: true,
        },
        DateFixTestCase {
            name: "filesystem mismatch should update modified date",
            date_modified: Some("[[2024-01-14]]".to_string()),
            date_created: Some("[[2024-01-15]]".to_string()),
            file_system_mod_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_date: Some("[[2024-01-15]]".to_string()),
            expected_created_date: Some("[[2024-01-15]]".to_string()),
            should_persist: true,
        },
        DateFixTestCase {
            name: "valid dates should not change",
            date_modified: Some("[[2024-01-15]]".to_string()),
            date_created: Some("[[2024-01-15]]".to_string()),
            file_system_mod_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_date: Some("[[2024-01-15]]".to_string()),
            expected_created_date: Some("[[2024-01-15]]".to_string()),
            should_persist: false,
        },
        DateFixTestCase {
            name: "filesystem mismatch should update both dates when both differ",
            date_modified: Some("[[2024-01-14]]".to_string()),
            date_created: Some("[[2024-01-13]]".to_string()),
            file_system_mod_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_date: Some("[[2024-01-15]]".to_string()),
            expected_created_date: Some("[[2024-01-15]]".to_string()),
            should_persist: true,
        },
        DateFixTestCase {
            name: "invalid format should change", // changed from "invalid format should not change"
            date_modified: Some("[[2024-13-45]]".to_string()),
            date_created: Some("[[2024-13-45]]".to_string()),
            file_system_mod_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            expected_modified_date: Some("[[2024-01-15]]".to_string()), // changed
            expected_created_date: Some("[[2024-01-15]]".to_string()),  // changed
            should_persist: true,                                       // changed from false
        },
        DateFixTestCase {
            name: "invalid wikilink should change", // changed from "invalid wikilink should not change"
            date_modified: Some("2024-01-15".to_string()),
            date_created: Some("2024-01-15".to_string()),
            file_system_mod_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
            file_system_create_date: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
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
            status: get_date_validation_status(
                case.date_created.as_ref(),
                &case.file_system_create_date,
            ),
        };

        let modified_validation = DateValidation {
            frontmatter_date: case.date_modified.clone(), // Add clone here
            file_system_date: case.file_system_mod_date,
            status: get_date_validation_status(
                case.date_modified.as_ref(),
                &case.file_system_mod_date,
            ),
        };

        // Process validations
        process_date_validations(&mut frontmatter, &created_validation, &modified_validation);

        // Verify frontmatter dates
        assert_test_case(
            frontmatter
                .as_ref()
                .and_then(|fm| fm.date_modified().cloned()),
            case.expected_modified_date,
            &format!("{} - modified date", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        assert_test_case(
            frontmatter
                .as_ref()
                .and_then(|fm| fm.date_created().cloned()),
            case.expected_created_date,
            &format!("{} - created date", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        assert_test_case(
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
            expected_parsed_date: Some(Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap()),
        },
        DateCreatedFixTestCase {
            name: "valid date with wikilink",
            date_created_fix: Some("[[2024-01-15]]".to_string()),
            expect_persist: true,
            expected_parsed_date: Some(Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap()),
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
        let test_date = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();
        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(
                Some("[[2024-01-15]]".to_string()),
                Some("[[2024-01-15]]".to_string()),
            )
            .with_fs_dates(test_date, test_date)
            .with_date_created_fix(case.date_created_fix.clone())
            .create(&temp_dir, "test1.md");

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
            markdown_info.frontmatter.unwrap().needs_persist(),
            case.expect_persist,
            &format!("{} - expect persist", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        assert_test_case(
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

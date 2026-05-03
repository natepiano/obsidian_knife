use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use chrono::DateTime;
use chrono::NaiveDate;
use chrono::Utc;
use filetime::FileTime;
use tempfile::TempDir;

use super::*;
use crate::constants::FORMAT_DATE;
use crate::markdown_file::MarkdownFile;
use crate::test_support as test_utils;
use crate::test_support::TestFileBuilder;

#[derive(Debug)]
struct PersistenceTestCase {
    name:                         &'static str,
    // Input state
    initial_frontmatter_created:  Option<String>,
    initial_frontmatter_modified: Option<String>,
    initial_file_system_created:  DateTime<Utc>,
    initial_file_system_modified: DateTime<Utc>,

    // Expected outcomes
    expected_frontmatter_created:  Option<String>,
    expected_frontmatter_modified: Option<String>,
    expected_file_system_created:  NaiveDate,
    expected_file_system_modified: NaiveDate,
    should_persist:                bool,
}

fn create_test_file_from_case(temp_dir: &TempDir, case: &PersistenceTestCase) -> PathBuf {
    // Format dates in wikilink format if they exist
    let created = case
        .initial_frontmatter_created
        .as_ref()
        .map(|d| format!("[[{d}]]"));
    let modified = case
        .initial_frontmatter_modified
        .as_ref()
        .map(|d| format!("[[{d}]]"));

    TestFileBuilder::new()
        .with_frontmatter_dates(created, modified)
        .with_fs_dates(
            case.initial_file_system_created,
            case.initial_file_system_modified,
        )
        .create(temp_dir, "test.md")
}

fn verify_dates(
    info: &MarkdownFile,
    case: &PersistenceTestCase,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(frontmatter) = &info.frontmatter {
        assert_eq!(
            frontmatter.needs_persist(),
            case.should_persist,
            "Persistence flag mismatch for case: {}",
            case.name
        );

        assert_eq!(
            frontmatter.date_created().map(|d| d
                .trim_matches('"')
                .trim_matches('[')
                .trim_matches(']')
                .to_string()),
            case.expected_frontmatter_created,
            "Frontmatter created date mismatch for case: {}",
            case.name
        );

        assert_eq!(
            frontmatter.date_modified().map(|d| d
                .trim_matches('"')
                .trim_matches('[')
                .trim_matches(']')
                .to_string()),
            case.expected_frontmatter_modified,
            "Frontmatter modified date mismatch for case: {}",
            case.name
        );
    } else if case.expected_frontmatter_created.is_some()
        || case.expected_frontmatter_modified.is_some()
    {
        panic!(
            "Expected frontmatter but none found for case: {}",
            case.name
        );
    }

    // Verify filesystem dates
    let metadata = fs::metadata(&info.path)?;
    let file_system_created_time = FileTime::from_creation_time(&metadata).unwrap();
    let file_system_modified_time = FileTime::from_last_modification_time(&metadata);

    // Convert to UTC for comparison
    let file_system_created_date = DateTime::<Utc>::from(
        SystemTime::UNIX_EPOCH
            + std::time::Duration::from_secs(
                file_system_created_time.unix_seconds().cast_unsigned(),
            ),
    )
    .date_naive();

    let file_system_modified_date = DateTime::<Utc>::from(
        SystemTime::UNIX_EPOCH
            + std::time::Duration::from_secs(
                file_system_modified_time.unix_seconds().cast_unsigned(),
            ),
    )
    .date_naive();

    // Compare dates
    assert_eq!(
        file_system_created_date, case.expected_file_system_created,
        "Filesystem created date mismatch for case: {}",
        case.name
    );

    assert_eq!(
        file_system_modified_date, case.expected_file_system_modified,
        "Filesystem modified date mismatch for case: {}",
        case.name
    );

    Ok(())
}

#[test]
#[cfg_attr(
    target_os = "linux",
    ignore = "requires filesystem access unavailable on Linux CI"
)]
fn test_persist_modified_files() -> Result<(), Box<dyn Error + Send + Sync>> {
    let test_cases = create_test_cases();

    for case in test_cases {
        // temp_dir needs to go out of scope each time for the file to be cleaned up
        let temp_dir = TempDir::new()?;
        let validated_config = test_utils::get_test_validated_config(&temp_dir, None);

        let file_path = create_test_file_from_case(&temp_dir, &case);

        let mut obsidian_repository = ObsidianRepository::new(&validated_config)?;
        let markdown_file = test_utils::get_test_markdown_file(file_path);

        obsidian_repository.markdown_files.push(markdown_file);

        // Run persistence
        obsidian_repository.persist()?;

        // Verify results
        verify_dates(&obsidian_repository.markdown_files[0], &case)?;
    }

    Ok(())
}

fn create_test_cases() -> Vec<PersistenceTestCase> {
    let last_week = test_utils::eastern_midnight(2024, 1, 8);

    vec![
        PersistenceTestCase {
            name:                          "no changes needed - dates match",
            // Both frontmatter and fs should use January 1st
            initial_frontmatter_created:   Some("2024-01-01".to_string()),
            initial_frontmatter_modified:  Some("2024-01-01".to_string()),
            initial_file_system_created:   test_utils::eastern_midnight(2024, 1, 1),
            initial_file_system_modified:  test_utils::eastern_midnight(2024, 1, 1),
            expected_frontmatter_created:  Some("2024-01-01".to_string()),
            expected_frontmatter_modified: Some("2024-01-01".to_string()),
            expected_file_system_created:  NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            expected_file_system_modified: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            should_persist:                false,
        },
        PersistenceTestCase {
            name:                          "created date mismatch triggers both dates update",
            initial_frontmatter_created:   Some("2024-01-15".to_string()),
            initial_frontmatter_modified:  Some("2024-01-15".to_string()),
            initial_file_system_created:   test_utils::eastern_midnight(2024, 1, 20),
            initial_file_system_modified:  test_utils::eastern_midnight(2024, 1, 20),
            expected_frontmatter_created:  Some("2024-01-20".to_string()),
            expected_frontmatter_modified: Some("2024-01-20".to_string()),
            expected_file_system_created:  NaiveDate::from_ymd_opt(2024, 1, 20).unwrap(),
            expected_file_system_modified: NaiveDate::from_ymd_opt(2024, 1, 20).unwrap(),
            should_persist:                true,
        },
        PersistenceTestCase {
            name:                          "invalid dates fixed from filesystem",
            initial_frontmatter_created:   Some("invalid date".to_string()),
            initial_frontmatter_modified:  Some("also invalid".to_string()),
            initial_file_system_created:   last_week,
            initial_file_system_modified:  last_week,
            expected_frontmatter_created:  Some(last_week.format(FORMAT_DATE).to_string()),
            expected_frontmatter_modified: Some(last_week.format(FORMAT_DATE).to_string()),
            expected_file_system_created:  last_week.date_naive(),
            expected_file_system_modified: last_week.date_naive(),
            should_persist:                true,
        },
    ]
}

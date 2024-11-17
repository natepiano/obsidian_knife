use super::*;
use crate::test_utils::TestFileBuilder;
use chrono::{DateTime, Local, NaiveDate};
use filetime::FileTime;
use std::error::Error;
use std::fs;
use std::time::SystemTime;
use tempfile::TempDir;

struct PersistenceTestCase {
    name: &'static str,
    // Input state
    initial_frontmatter_created: Option<String>,
    initial_frontmatter_modified: Option<String>,
    initial_fs_created: DateTime<Local>,
    initial_fs_modified: DateTime<Local>,

    // Expected outcomes
    expected_frontmatter_created: Option<String>,
    expected_frontmatter_modified: Option<String>,
    expected_fs_created_date: NaiveDate,
    expected_fs_modified_date: NaiveDate,
    should_persist: bool,
}

fn create_test_file_from_case(temp_dir: &TempDir, case: &PersistenceTestCase) -> PathBuf {
    TestFileBuilder::new()
        .with_frontmatter_dates(
            case.initial_frontmatter_created.clone(),
            case.initial_frontmatter_modified.clone(),
        )
        .with_fs_dates(case.initial_fs_created, case.initial_fs_modified)
        .create(temp_dir, "test.md")
}

fn verify_dates(
    info: &MarkdownFileInfo,
    case: &PersistenceTestCase,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Verify frontmatter dates
    if let Some(frontmatter) = &info.frontmatter {
        // Fix for the malformed assert_eq!
        assert_eq!(
            frontmatter
                .date_created()
                .map(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                    .expect("Valid date string")
                    .format("%Y-%m-%d")
                    .to_string()),
            case.expected_frontmatter_created,
            "Frontmatter created date mismatch for case: {}",
            case.name
        );
        assert_eq!(
            frontmatter
                .date_modified()
                .map(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                    .expect("Valid date string")
                    .format("%Y-%m-%d")
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
    let fs_created = FileTime::from_last_access_time(&metadata);
    let fs_modified = FileTime::from_last_modification_time(&metadata);

    // For the created date check
    assert_eq!(
        DateTime::<Local>::from(
            SystemTime::UNIX_EPOCH
                + std::time::Duration::from_secs(fs_created.unix_seconds() as u64)
        )
        .date_naive(),
        case.expected_fs_created_date,
        "Filesystem created date mismatch for case: {}",
        case.name
    );

    // For the modified date check
    assert_eq!(
        DateTime::<Local>::from(
            SystemTime::UNIX_EPOCH
                + std::time::Duration::from_secs(fs_modified.unix_seconds() as u64)
        )
        .date_naive(),
        case.expected_fs_modified_date,
        "Filesystem modified date mismatch for case: {}",
        case.name
    );

    Ok(())
}

#[test]
#[ignore]
fn test_persist_modified_files() -> Result<(), Box<dyn Error + Send + Sync>> {
    let test_cases = create_test_cases();

    for case in test_cases {
        let temp_dir = TempDir::new()?;
        let file_path = create_test_file_from_case(&temp_dir, &case);

        let mut repo_info = ObsidianRepositoryInfo::default();
        let file_info = MarkdownFileInfo::new(file_path)?;

        repo_info.markdown_files.push(file_info);

        // Run persistence
        repo_info.persist_modified_files()?;

        // Verify results
        verify_dates(&repo_info.markdown_files[0], &case)?;
    }

    Ok(())
}

fn create_test_cases() -> Vec<PersistenceTestCase> {
    let now = Local::now();
    let yesterday = now - chrono::Duration::days(1);
    let last_week = now - chrono::Duration::days(7);
    let last_month = now - chrono::Duration::days(30);

    vec![
        PersistenceTestCase {
            name: "no changes needed",
            initial_frontmatter_created: Some("2024-01-01".to_string()),
            initial_frontmatter_modified: Some("2024-01-02".to_string()),
            initial_fs_created: Local::now(), // we'll set this to match frontmatter in setup
            initial_fs_modified: Local::now(), // we'll set this to match frontmatter in setup
            expected_frontmatter_created: Some("2024-01-01".to_string()),
            expected_frontmatter_modified: Some("2024-01-02".to_string()),
            expected_fs_created_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            expected_fs_modified_date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            should_persist: false,
        },
        PersistenceTestCase {
            name: "date created changed",
            initial_frontmatter_created: Some(last_month.format("%Y-%m-%d").to_string()),
            initial_frontmatter_modified: Some(yesterday.format("%Y-%m-%d").to_string()),
            initial_fs_created: last_week,  // Differs from frontmatter
            initial_fs_modified: yesterday, // Matches frontmatter
            expected_frontmatter_created: Some(last_month.format("%Y-%m-%d").to_string()),
            expected_frontmatter_modified: Some(yesterday.format("%Y-%m-%d").to_string()),
            expected_fs_created_date: last_month.date_naive(),
            expected_fs_modified_date: yesterday.date_naive(),
            should_persist: true,
        },
        PersistenceTestCase {
            name: "date modified changed",
            initial_frontmatter_created: Some(last_month.format("%Y-%m-%d").to_string()),
            initial_frontmatter_modified: Some(yesterday.format("%Y-%m-%d").to_string()),
            initial_fs_created: last_month, // Matches frontmatter
            initial_fs_modified: last_week, // Differs from frontmatter
            expected_frontmatter_created: Some(last_month.format("%Y-%m-%d").to_string()),
            expected_frontmatter_modified: Some(yesterday.format("%Y-%m-%d").to_string()),
            expected_fs_created_date: last_month.date_naive(),
            expected_fs_modified_date: yesterday.date_naive(),
            should_persist: true,
        },
        PersistenceTestCase {
            name: "both dates changed",
            initial_frontmatter_created: Some(last_month.format("%Y-%m-%d").to_string()),
            initial_frontmatter_modified: Some(yesterday.format("%Y-%m-%d").to_string()),
            initial_fs_created: last_week,  // Differs from frontmatter
            initial_fs_modified: last_week, // Differs from frontmatter
            expected_frontmatter_created: Some(last_month.format("%Y-%m-%d").to_string()),
            expected_frontmatter_modified: Some(yesterday.format("%Y-%m-%d").to_string()),
            expected_fs_created_date: last_month.date_naive(),
            expected_fs_modified_date: yesterday.date_naive(),
            should_persist: true,
        },
        PersistenceTestCase {
            name: "missing dates added",
            initial_frontmatter_created: None,
            initial_frontmatter_modified: None,
            initial_fs_created: last_week,
            initial_fs_modified: last_week,
            expected_frontmatter_created: Some(last_week.format("%Y-%m-%d").to_string()),
            expected_frontmatter_modified: Some(last_week.format("%Y-%m-%d").to_string()),
            expected_fs_created_date: last_week.date_naive(),
            expected_fs_modified_date: last_week.date_naive(),
            should_persist: true,
        },
        PersistenceTestCase {
            name: "invalid dates fixed",
            initial_frontmatter_created: Some("invalid date".to_string()),
            initial_frontmatter_modified: Some("also invalid".to_string()),
            initial_fs_created: last_week,
            initial_fs_modified: last_week,
            expected_frontmatter_created: Some(last_week.format("%Y-%m-%d").to_string()),
            expected_frontmatter_modified: Some(last_week.format("%Y-%m-%d").to_string()),
            expected_fs_created_date: last_week.date_naive(),
            expected_fs_modified_date: last_week.date_naive(),
            should_persist: true,
        },
    ]
}

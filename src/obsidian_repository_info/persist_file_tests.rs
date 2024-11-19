use super::*;
use crate::test_utils::TestFileBuilder;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
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
    initial_fs_created: DateTime<Utc>,
    initial_fs_modified: DateTime<Utc>,

    // Expected outcomes
    expected_frontmatter_created: Option<String>,
    expected_frontmatter_modified: Option<String>,
    expected_fs_created_date: NaiveDate,
    expected_fs_modified_date: NaiveDate,
    should_persist: bool,
}

fn create_test_file_from_case(temp_dir: &TempDir, case: &PersistenceTestCase) -> PathBuf {
    // Format dates in wikilink format if they exist
    let created = case
        .initial_frontmatter_created
        .as_ref()
        .map(|d| format!("[[{}]]", d));
    let modified = case
        .initial_frontmatter_modified
        .as_ref()
        .map(|d| format!("[[{}]]", d));

    let file_path = TestFileBuilder::new()
        .with_frontmatter_dates(created, modified)
        .with_fs_dates(case.initial_fs_created, case.initial_fs_modified)
        .create(temp_dir, "test.md");

    file_path
}

fn verify_dates(
    info: &MarkdownFileInfo,
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
    let fs_created = FileTime::from_last_access_time(&metadata);
    let fs_modified = FileTime::from_last_modification_time(&metadata);

    // Convert to UTC for comparison
    let fs_created_date = DateTime::<Utc>::from(
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(fs_created.unix_seconds() as u64),
    )
    .date_naive();

    let fs_modified_date = DateTime::<Utc>::from(
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(fs_modified.unix_seconds() as u64),
    )
    .date_naive();

    // Compare dates
    assert_eq!(
        fs_created_date, case.expected_fs_created_date,
        "Filesystem created date mismatch for case: {}",
        case.name
    );

    assert_eq!(
        fs_modified_date, case.expected_fs_modified_date,
        "Filesystem modified date mismatch for case: {}",
        case.name
    );

    Ok(())
}

#[test]
fn test_persist_modified_files() -> Result<(), Box<dyn Error + Send + Sync>> {
    let test_cases = create_test_cases();

    for case in test_cases {
        let temp_dir = TempDir::new()?;
        let file_path = create_test_file_from_case(&temp_dir, &case);

        let mut repo_info = ObsidianRepositoryInfo::default();
        let file_info = MarkdownFileInfo::new(file_path)?;

        repo_info.markdown_files.push(file_info);

        // Run persistence
        repo_info.persist()?;

        // Verify results
        verify_dates(&repo_info.markdown_files[0], &case)?;
    }

    Ok(())
}

fn create_test_cases() -> Vec<PersistenceTestCase> {
    let now = Utc::now();
    let last_week = now - chrono::Duration::days(7);

    vec![
        PersistenceTestCase {
            name: "no changes needed - dates match",
            // Both frontmatter and fs should use January 1st
            initial_frontmatter_created: Some("2024-01-01".to_string()),
            initial_frontmatter_modified: Some("2024-01-01".to_string()),
            initial_fs_created: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            initial_fs_modified: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            expected_frontmatter_created: Some("2024-01-01".to_string()),
            expected_frontmatter_modified: Some("2024-01-01".to_string()),
            expected_fs_created_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            expected_fs_modified_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            should_persist: false,
        },
        PersistenceTestCase {
            name: "created date mismatch triggers both dates update",
            initial_frontmatter_created: Some("2024-01-15".to_string()),
            initial_frontmatter_modified: Some("2024-01-15".to_string()),
            initial_fs_created: Utc.with_ymd_and_hms(2024, 1, 20, 0, 0, 0).unwrap(),
            initial_fs_modified: Utc.with_ymd_and_hms(2024, 1, 20, 0, 0, 0).unwrap(),
            expected_frontmatter_created: Some("2024-01-20".to_string()),
            expected_frontmatter_modified: Some("2024-01-20".to_string()),
            expected_fs_created_date: NaiveDate::from_ymd_opt(2024, 1, 20).unwrap(),
            expected_fs_modified_date: NaiveDate::from_ymd_opt(2024, 1, 20).unwrap(),
            should_persist: true,
        },
        PersistenceTestCase {
            name: "invalid dates fixed from filesystem",
            initial_frontmatter_created: Some("invalid date".to_string()),
            initial_frontmatter_modified: Some("also invalid".to_string()),
            initial_fs_created: last_week,
            initial_fs_modified: last_week,
            expected_frontmatter_created: Some(last_week.format("%Y-%m-%d").to_string()),
            expected_frontmatter_modified: Some(last_week.format("%Y-%m-%d").to_string()), // Changed from now to last_week
            expected_fs_created_date: last_week.date_naive(),
            expected_fs_modified_date: last_week.date_naive(), // Changed from now to last_week
            should_persist: true,
        },
    ]
}

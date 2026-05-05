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
use crate::test_support::PersistExpectation;
use crate::test_support::TestFileBuilder;

#[derive(Debug)]
struct FrontmatterDates {
    created:  Option<String>,
    modified: Option<String>,
}

#[derive(Debug)]
struct FileSystemDates<T> {
    created:  T,
    modified: T,
}

#[derive(Debug)]
struct PersistenceState<T> {
    frontmatter: FrontmatterDates,
    file_system: FileSystemDates<T>,
}

#[derive(Debug)]
struct PersistenceOutcome {
    dates:   PersistenceState<NaiveDate>,
    persist: PersistExpectation,
}

#[derive(Debug)]
struct PersistenceTestCase {
    name:     &'static str,
    initial:  PersistenceState<DateTime<Utc>>,
    expected: PersistenceOutcome,
}

fn create_test_file_from_case(temp_dir: &TempDir, case: &PersistenceTestCase) -> PathBuf {
    // Format dates in wikilink format if they exist
    let created = case
        .initial
        .frontmatter
        .created
        .as_ref()
        .map(|d| format!("[[{d}]]"));
    let modified = case
        .initial
        .frontmatter
        .modified
        .as_ref()
        .map(|d| format!("[[{d}]]"));

    TestFileBuilder::new()
        .with_frontmatter_dates(created, modified)
        .with_fs_dates(
            case.initial.file_system.created,
            case.initial.file_system.modified,
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
            case.expected.persist.needs_persist(),
            "Persistence flag mismatch for case: {}",
            case.name
        );

        assert_eq!(
            frontmatter.date_created().map(|d| d
                .trim_matches('"')
                .trim_matches('[')
                .trim_matches(']')
                .to_string()),
            case.expected.dates.frontmatter.created,
            "Frontmatter created date mismatch for case: {}",
            case.name
        );

        assert_eq!(
            frontmatter.date_modified().map(|d| d
                .trim_matches('"')
                .trim_matches('[')
                .trim_matches(']')
                .to_string()),
            case.expected.dates.frontmatter.modified,
            "Frontmatter modified date mismatch for case: {}",
            case.name
        );
    } else if case.expected.dates.frontmatter.created.is_some()
        || case.expected.dates.frontmatter.modified.is_some()
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
        file_system_created_date, case.expected.dates.file_system.created,
        "Filesystem created date mismatch for case: {}",
        case.name
    );

    assert_eq!(
        file_system_modified_date, case.expected.dates.file_system.modified,
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
            name:     "no changes needed - dates match",
            initial:  PersistenceState {
                frontmatter: FrontmatterDates {
                    created:  Some("2024-01-01".to_string()),
                    modified: Some("2024-01-01".to_string()),
                },
                file_system: FileSystemDates {
                    created:  test_utils::eastern_midnight(2024, 1, 1),
                    modified: test_utils::eastern_midnight(2024, 1, 1),
                },
            },
            expected: PersistenceOutcome {
                dates:   PersistenceState {
                    frontmatter: FrontmatterDates {
                        created:  Some("2024-01-01".to_string()),
                        modified: Some("2024-01-01".to_string()),
                    },
                    file_system: FileSystemDates {
                        created:  NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                        modified: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                    },
                },
                persist: PersistExpectation::DoesNotPersist,
            },
        },
        PersistenceTestCase {
            name:     "created date mismatch triggers both dates update",
            initial:  PersistenceState {
                frontmatter: FrontmatterDates {
                    created:  Some("2024-01-15".to_string()),
                    modified: Some("2024-01-15".to_string()),
                },
                file_system: FileSystemDates {
                    created:  test_utils::eastern_midnight(2024, 1, 20),
                    modified: test_utils::eastern_midnight(2024, 1, 20),
                },
            },
            expected: PersistenceOutcome {
                dates:   PersistenceState {
                    frontmatter: FrontmatterDates {
                        created:  Some("2024-01-20".to_string()),
                        modified: Some("2024-01-20".to_string()),
                    },
                    file_system: FileSystemDates {
                        created:  NaiveDate::from_ymd_opt(2024, 1, 20).unwrap(),
                        modified: NaiveDate::from_ymd_opt(2024, 1, 20).unwrap(),
                    },
                },
                persist: PersistExpectation::Persists,
            },
        },
        PersistenceTestCase {
            name:     "invalid dates fixed from filesystem",
            initial:  PersistenceState {
                frontmatter: FrontmatterDates {
                    created:  Some("invalid date".to_string()),
                    modified: Some("also invalid".to_string()),
                },
                file_system: FileSystemDates {
                    created:  last_week,
                    modified: last_week,
                },
            },
            expected: PersistenceOutcome {
                dates:   PersistenceState {
                    frontmatter: FrontmatterDates {
                        created:  Some(last_week.format(FORMAT_DATE).to_string()),
                        modified: Some(last_week.format(FORMAT_DATE).to_string()),
                    },
                    file_system: FileSystemDates {
                        created:  last_week.date_naive(),
                        modified: last_week.date_naive(),
                    },
                },
                persist: PersistExpectation::Persists,
            },
        },
    ]
}

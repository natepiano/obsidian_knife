use std::error::Error;
use std::ffi::OsStr;
use std::str::FromStr;

use chrono::TimeZone;
use chrono::Utc;
use tempfile::TempDir;

use super::ObsidianRepository;
use crate::markdown_file::MarkdownFile;
use crate::test_support;
use crate::test_support as test_utils;
use crate::test_support::TestFileBuilder;

#[derive(Debug)]
struct FileLimitTestCase {
    name:               &'static str,
    file_count:         usize,
    process_limit:      Option<usize>,
    expected_processed: usize,
}

fn create_test_files(temp_dir: &TempDir, count: usize, timezone: &str) {
    let timezone = chrono_tz::Tz::from_str(timezone).unwrap();
    let base_date = timezone.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

    let _: Vec<MarkdownFile> = (0..count)
        .map(|i| {
            let created = base_date + chrono::Duration::days(i64::try_from(i).expect("test index"));
            let modified = created + chrono::Duration::hours(1);

            // Convert to UTC for the filesystem dates
            let created_utc = created.with_timezone(&Utc);
            let modified_utc = modified.with_timezone(&Utc);

            let file = TestFileBuilder::new()
                .with_frontmatter_dates(
                    Some(format!("[[{}-01-01]]", 2023 - i)),
                    Some(format!("[[{}-01-01]]", 2023 - i)),
                )
                .with_fs_dates(created_utc, modified_utc)
                .create(temp_dir, &format!("test_{i}.md"));

            test_utils::get_test_markdown_file(file)
        })
        .collect();
}

#[test]
#[cfg_attr(
    target_os = "linux",
    ignore = "requires filesystem access unavailable on Linux CI"
)]
fn test_file_limit() -> Result<(), Box<dyn Error + Send + Sync>> {
    let test_cases = vec![
        FileLimitTestCase {
            name:               "no limit processes all files",
            file_count:         3,
            process_limit:      None,
            expected_processed: 3,
        },
        FileLimitTestCase {
            name:               "limit of 1 processes single file",
            file_count:         3,
            process_limit:      Some(1),
            expected_processed: 1,
        },
        FileLimitTestCase {
            name:               "limit of 2 processes two files",
            file_count:         3,
            process_limit:      Some(2),
            expected_processed: 2,
        },
        FileLimitTestCase {
            name:               "limit larger than file count processes all files",
            file_count:         2,
            process_limit:      Some(5),
            expected_processed: 2,
        },
    ];

    for case in test_cases {
        let temp_dir = TempDir::new()?;

        let mut builder = test_support::get_test_validated_config_builder(&temp_dir);
        builder.file_limit(case.process_limit);
        let validated_config = builder.build()?;

        // Create test files
        create_test_files(
            &temp_dir,
            case.file_count,
            validated_config.operational_timezone(),
        );
        let obsidian_repository = ObsidianRepository::new(&validated_config)?;

        // Run persistence
        obsidian_repository.persist()?;

        // Verify files were actually processed by checking their content
        let processed_count = obsidian_repository
            .markdown_files
            .files_to_persist()
            .iter()
            .take(case.expected_processed)
            .filter(|file| {
                std::fs::read_to_string(&file.path).is_ok_and(|content| {
                    let file_index = file
                        .path
                        .file_stem()
                        .and_then(OsStr::to_str)
                        .and_then(|s| s.strip_prefix("test_"))
                        .and_then(|s| s.parse::<i64>().ok())
                        .unwrap_or(0);

                    let expected_date = format!("2024-01-{:02}", file_index + 1);

                    let has_created =
                        content.contains(&format!("date_created: '[[{expected_date}]]'"));
                    let has_modified =
                        content.contains(&format!("date_modified: '[[{expected_date}]]'"));

                    has_created && has_modified
                })
            })
            .count();

        assert_eq!(
            processed_count, case.expected_processed,
            "Test case '{}' failed: expected {} files to be processed, but {} were processed",
            case.name, case.expected_processed, processed_count
        );
    }

    Ok(())
}

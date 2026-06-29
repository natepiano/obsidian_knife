mod back_populate;
mod constants;
mod image_processing;

use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use aho_corasick::AhoCorasick;
use aho_corasick::AhoCorasickBuilder;
use aho_corasick::MatchKind;
use anyhow::Result as AnyhowResult;
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;

use self::constants::ANALYZE_TIMER_LABEL;
use self::constants::ERROR_PROCESSING_FILE;
use self::constants::MARKDOWN_FILE_COLLECTION_SHARED_REFERENCES;
use self::constants::PRESCAN_ANALYZE_TIMER_LABEL;
use crate::constants::MARKDOWN_FILE_COLLECTION_LOCK_POISONED;
use crate::image_file::ImageFiles;
use crate::markdown_file::MarkdownFile;
use crate::markdown_files::MarkdownFiles;
use crate::support;
use crate::timer::Timer;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::Wikilink;

#[derive(Default)]
pub(crate) struct ObsidianRepository {
    pub markdown_files:      MarkdownFiles,
    pub image_files:         ImageFiles,
    pub wikilinks_automaton: Option<AhoCorasick>,
    pub wikilinks_sorted:    Vec<Wikilink>,
}

impl ObsidianRepository {
    pub(crate) fn new(
        validated_config: &ValidatedConfig,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let _timer = Timer::new(PRESCAN_ANALYZE_TIMER_LABEL);
        let ignore_folders = validated_config.ignore_folders().unwrap_or(&[]);

        let repository_files = support::collect_repository_files(validated_config, ignore_folders)?;

        let markdown_files = Self::initialize_markdown_files(
            &repository_files.markdown,
            validated_config.operational_timezone(),
            validated_config.file_limit(),
        )?;

        let (sorted, automaton) = Self::initialize_wikilinks(&markdown_files)?;

        let mut repository = Self {
            markdown_files,
            image_files: ImageFiles::default(),
            wikilinks_automaton: Some(automaton),
            wikilinks_sorted: sorted,
        };

        repository.image_files =
            repository.initialize_image_files(&repository_files.images, validated_config)?;

        repository.analyze_repository(validated_config)?;

        Ok(repository)
    }

    fn initialize_markdown_files(
        markdown_paths: &[PathBuf],
        timezone: &str,
        file_limit: Option<usize>,
    ) -> Result<MarkdownFiles, Box<dyn Error + Send + Sync>> {
        let markdown_files = Arc::new(Mutex::new(MarkdownFiles::default()));

        markdown_paths.par_iter().try_for_each(|file_path| {
            match MarkdownFile::new(file_path.clone(), timezone) {
                Ok(markdown_file) => {
                    markdown_files
                        .lock()
                        .map_err(|error| {
                            format!("{MARKDOWN_FILE_COLLECTION_LOCK_POISONED}: {error}")
                        })?
                        .push(markdown_file);
                    Ok(())
                },
                Err(e) => {
                    eprintln!("{ERROR_PROCESSING_FILE} {}: {e}", file_path.display());
                    Err(e)
                },
            }
        })?;

        let markdown_files_mutex = Arc::try_unwrap(markdown_files)
            .map_err(|_| MARKDOWN_FILE_COLLECTION_SHARED_REFERENCES.to_string())?;
        let mut markdown_files = markdown_files_mutex
            .into_inner()
            .map_err(|error| format!("{MARKDOWN_FILE_COLLECTION_LOCK_POISONED}: {error}"))?;

        markdown_files.file_limit = file_limit;

        Ok(markdown_files)
    }

    fn initialize_wikilinks(
        markdown_files: &MarkdownFiles,
    ) -> Result<(Vec<Wikilink>, AhoCorasick), Box<dyn Error + Send + Sync>> {
        let all_wikilinks: HashSet<Wikilink> = markdown_files
            .iter()
            .flat_map(|markdown_file| markdown_file.wikilinks.valid.clone())
            .collect();
        Self::sort_and_build_wikilinks_automaton(all_wikilinks)
    }

    fn sort_and_build_wikilinks_automaton(
        all_wikilinks: HashSet<Wikilink>,
    ) -> Result<(Vec<Wikilink>, AhoCorasick), Box<dyn Error + Send + Sync>> {
        let mut wikilinks: Vec<_> = all_wikilinks.into_iter().collect();
        wikilinks.sort_unstable();

        let mut patterns = Vec::with_capacity(wikilinks.len());
        patterns.extend(wikilinks.iter().map(|w| w.display_text.as_str()));

        let automaton = AhoCorasickBuilder::new()
            .ascii_case_insensitive(true)
            .match_kind(MatchKind::LeftmostLongest)
            .build(&patterns)?;

        Ok((wikilinks, automaton))
    }

    fn analyze_repository(&mut self, validated_config: &ValidatedConfig) -> AnyhowResult<()> {
        let _timer = Timer::new(ANALYZE_TIMER_LABEL);
        self.find_all_back_populate_matches(validated_config)?;
        self.identify_ambiguous_matches();
        self.identify_image_reference_replacements();
        self.apply_replaceable_matches(validated_config.operational_timezone())?;
        self.mark_image_files_for_deletion();
        Ok(())
    }

    pub(crate) fn persist(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.image_files.delete_marked()?;
        self.markdown_files.files_to_persist().persist_all()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::cmp::Ordering;
    use std::collections::HashMap;
    use std::collections::HashSet;
    use std::error::Error;
    use std::ffi::OsStr;
    use std::fs;
    use std::fs::read_to_string;
    use std::path::Path;
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::time::Duration;
    use std::time::SystemTime;

    use chrono::DateTime;
    use chrono::Duration as ChronoDuration;
    use chrono::NaiveDate;
    use chrono::TimeZone;
    use chrono::Utc;
    use filetime::FileTime;
    use rayon::prelude::*;
    use serde_json::Value;
    use serde_json::from_str;
    use tempfile::TempDir;

    use super::ObsidianRepository;
    use crate::constants::CACHE_FILE;
    use crate::constants::CACHE_FOLDER;
    use crate::constants::DEFAULT_TIMEZONE;
    use crate::constants::FORMAT_DATE;
    use crate::constants::MARKDOWN_EXTENSION;
    use crate::image_file::ImageFile;
    use crate::image_file::ImageFileState;
    use crate::image_file::ImageFiles;
    use crate::image_file::IncompatibilityReason;
    use crate::markdown_file::ImageLink;
    use crate::markdown_file::MarkdownFile;
    use crate::markdown_file::PersistReason;
    use crate::test_support;
    use crate::test_support as test_utils;
    use crate::test_support::PersistExpectation;
    use crate::test_support::TestFileBuilder;
    use crate::validated_config::ValidatedConfig;
    use crate::validated_config::ValidatedConfigBuilder;

    #[derive(Debug)]
    struct FileLimitTestCase {
        name:               &'static str,
        file_count:         usize,
        limit:              Option<usize>,
        expected_processed: usize,
    }

    fn create_test_files(temp_dir: &TempDir, count: usize, timezone: &str) {
        let timezone = chrono_tz::Tz::from_str(timezone).unwrap();
        let base_date = timezone.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

        let _: Vec<MarkdownFile> = (0..count)
            .map(|i| {
                let created =
                    base_date + ChronoDuration::days(i64::try_from(i).expect("test index"));
                let modified = created + ChronoDuration::hours(1);

                // `created_utc` and `modified_utc` store filesystem dates in UTC.
                let created_utc = created.with_timezone(&Utc);
                let modified_utc = modified.with_timezone(&Utc);

                let file = TestFileBuilder::new()
                    .with_frontmatter_dates(
                        Some(format!("[[{}-01-01]]", 2023 - i)),
                        Some(format!("[[{}-01-01]]", 2023 - i)),
                    )
                    .with_file_system_dates(created_utc, modified_utc)
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
                limit:              None,
                expected_processed: 3,
            },
            FileLimitTestCase {
                name:               "limit of 1 processes single file",
                file_count:         3,
                limit:              Some(1),
                expected_processed: 1,
            },
            FileLimitTestCase {
                name:               "limit of 2 processes two files",
                file_count:         3,
                limit:              Some(2),
                expected_processed: 2,
            },
            FileLimitTestCase {
                name:               "limit larger than file count processes all files",
                file_count:         2,
                limit:              Some(5),
                expected_processed: 2,
            },
        ];

        for case in test_cases {
            let temp_dir = TempDir::new()?;

            let mut builder = test_utils::get_test_validated_config_builder(&temp_dir);
            builder.file_limit(case.limit);
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
                    read_to_string(&file.path).is_ok_and(|content| {
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

    fn setup_test_repo() -> (TempDir, ValidatedConfig) {
        let temp_dir = TempDir::new().unwrap();

        // First create the validated config so we know the correct media path
        let validated_config = get_validated_config(&temp_dir);

        // Now create our test files using the config's media path
        let media_path = temp_dir.path().join("media");
        fs::create_dir_all(&media_path).unwrap();

        // Create test cases with `TestFileBuilder`, putting them in the media folder
        let markdown_content = r"---
date_created: 2024-01-01
date_modified: 2024-01-01
---
# Test Special Images
![[zero_byte.png]]
![[test.tiff]]";

        TestFileBuilder::new()
            .with_content(markdown_content.as_bytes().to_vec())
            .create(&temp_dir, "special_images.md");

        TestFileBuilder::new()
            .with_content(vec![]) // Empty content for zero byte file
            .create(&temp_dir, "media/zero_byte.png");

        TestFileBuilder::new()
            .with_content(vec![0x4D, 0x4D, 0x00, 0x2A]) // TIFF header
            .create(&temp_dir, "media/test.tiff");

        (temp_dir, validated_config)
    }

    fn get_validated_config(temp_dir: &TempDir) -> ValidatedConfig {
        ValidatedConfigBuilder::default()
            .obsidian_path(temp_dir.path().to_path_buf())
            .output_folder(PathBuf::from("output")) // Just the subfolder name
            .operational_timezone("UTC".to_string())
            .build()
            .unwrap()
    }

    pub(crate) fn find_image_file<'a>(
        files: &'a ImageFiles,
        path: &'a Path,
    ) -> Option<&'a ImageFile> {
        files.images.iter().find(|image| image.path == *path)
    }

    #[test]
    fn test_new_handles_empty_repo() -> Result<(), Box<dyn Error + Send + Sync>> {
        let temp_dir = TempDir::new().unwrap();

        let validated_config = get_validated_config(&temp_dir);

        let obsidian_repository = ObsidianRepository::new(&validated_config)?;

        assert!(obsidian_repository.image_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_new_handles_special_cases() -> Result<(), Box<dyn Error + Send + Sync>> {
        fn assert_incompatible_state(
            image_files: &ImageFiles,
            path: &Path,
            expected_reason: IncompatibilityReason,
            message: &str,
        ) {
            if let Some(image) = find_image_file(image_files, path) {
                assert_eq!(
                    image.state,
                    ImageFileState::Incompatible {
                        reason: expected_reason,
                    },
                    "{message}"
                );
            } else {
                panic!("Expected to find file at {path:?}");
            }
        }

        let (temp_dir, validated_config) = setup_test_repo();

        // Create test cases with `TestFileBuilder`
        let zero_byte_path = TestFileBuilder::new()
            .with_content(vec![])
            .create(&temp_dir, "media/zero_byte.png");
        let tiff_path = TestFileBuilder::new()
            .with_content(vec![0x4D, 0x4D, 0x00, 0x2A])
            .create(&temp_dir, "media/test.tiff");

        let markdown_content = r"---
date_created: 2024-01-01
date_modified: 2024-01-01
---
# Test Special Images
![[zero_byte.png]]
![[test.tiff]]";

        let _ = TestFileBuilder::new()
            .with_content(markdown_content.as_bytes().to_vec())
            .create(&temp_dir, "special_images.md");

        let obsidian_repository = ObsidianRepository::new(&validated_config)?;

        assert_incompatible_state(
            &obsidian_repository.image_files,
            &zero_byte_path,
            IncompatibilityReason::ZeroByte,
            "Zero-byte file should have ZeroByte state",
        );

        assert_incompatible_state(
            &obsidian_repository.image_files,
            &tiff_path,
            IncompatibilityReason::TiffFormat,
            "TIFF file should have Tiff state",
        );

        Ok(())
    }

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
            .with_file_system_dates(
                case.initial.file_system.created,
                case.initial.file_system.modified,
            )
            .create(temp_dir, "test.md")
    }

    fn verify_dates(
        markdown_file: &MarkdownFile,
        case: &PersistenceTestCase,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(frontmatter) = &markdown_file.frontmatter {
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
        let metadata = fs::metadata(&markdown_file.path)?;
        let file_system_created_time = FileTime::from_creation_time(&metadata).unwrap();
        let file_system_modified_time = FileTime::from_last_modification_time(&metadata);

        // `file_system_created_date` stores the creation timestamp in UTC.
        let file_system_created_date = DateTime::<Utc>::from(
            SystemTime::UNIX_EPOCH
                + Duration::from_secs(file_system_created_time.unix_seconds().cast_unsigned()),
        )
        .date_naive();

        let file_system_modified_date = DateTime::<Utc>::from(
            SystemTime::UNIX_EPOCH
                + Duration::from_secs(file_system_modified_time.unix_seconds().cast_unsigned()),
        )
        .date_naive();

        // `case.expected.dates.file_system` stores the expected file dates.
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
            // Each loop iteration scopes its own `TempDir` so file cleanup runs before the next
            // case.
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
                    persist: PersistExpectation::Unchanged,
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

    #[test]
    fn test_parallel_image_reference_collection() {
        // Common filter logic
        fn has_common_image(markdown_file: &MarkdownFile) -> bool {
            markdown_file
                .image_links
                .iter()
                .any(|link| link.filename == "common.jpg")
        }

        // Helper functions using shared filter
        fn process_parallel(files: &HashMap<PathBuf, MarkdownFile>) -> Vec<PathBuf> {
            files
                .par_iter()
                .filter_map(|(path, info)| has_common_image(info).then(|| path.clone()))
                .collect()
        }

        fn process_sequential(files: &HashMap<PathBuf, MarkdownFile>) -> Vec<PathBuf> {
            files
                .iter()
                .filter_map(|(path, info)| {
                    if has_common_image(info) {
                        Some(path.clone())
                    } else {
                        None
                    }
                })
                .collect()
        }

        let temp_dir = TempDir::new().unwrap();
        let mut markdown_files = HashMap::new();

        for i in 0..100 {
            let filename = format!("note{i}.md");
            let content = format!("![[test{i}.jpg]]\n![[common.jpg]]");
            let file_path = TestFileBuilder::new()
                .with_content(content.clone())
                .create(&temp_dir, &filename);
            let mut info = test_utils::get_test_markdown_file(file_path.clone());

            info.image_links.links = content
                .split('\n')
                .map(|s| ImageLink::new(s.to_string(), 1, 0).unwrap())
                .collect();

            markdown_files.insert(file_path, info);
        }

        // Test parallel processing
        let parallel_results = process_parallel(&markdown_files);

        // Test sequential processing
        let sequential_results = process_sequential(&markdown_files);

        // Verify results
        assert_eq!(parallel_results.len(), sequential_results.len());
        assert_eq!(
            parallel_results.len(),
            100,
            "Should find common image in all files"
        );
    }

    #[test]
    fn test_wikilink_sorting_with_aliases() {
        let temp_dir = TempDir::new().unwrap();

        // Create tomato file with alias
        TestFileBuilder::new()
            .with_aliases(vec!["tomatoes".to_string()])
            .with_content("# Tomato\nBasic tomato info".to_string())
            .create(&temp_dir, "tomato.md");

        // Create recipe file
        TestFileBuilder::new()
            .with_content("# Recipe\nUsing tomatoes in cooking".to_string())
            .create(&temp_dir, "recipe.md");

        // Create other file with wikilink
        TestFileBuilder::new()
            .with_content("# Other\n[[tomatoes]] reference that might confuse things".to_string())
            .create(&temp_dir, "other.md");

        let validated_config = test_utils::get_test_validated_config(&temp_dir, None);

        // Scan folders and check results
        let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        // Find the wikilinks for "tomatoes" in the sorted list
        let tomatoes_wikilinks: Vec<_> = obsidian_repository
            .wikilinks_sorted
            .iter()
            .filter(|w| w.display_text.eq_ignore_ascii_case("tomatoes"))
            .collect();

        // Verify we found the wikilinks
        assert!(
            !tomatoes_wikilinks.is_empty(),
            "Should find wikilinks for 'tomatoes'"
        );

        // The first occurrence should be the alias version
        let first_tomatoes = &tomatoes_wikilinks[0];
        assert!(
            first_tomatoes.is_alias() && first_tomatoes.target == "tomato",
            "First 'tomatoes' wikilink should be the alias version targeting 'tomato'"
        );

        // Add test for total ordering property
        let sorted = obsidian_repository.wikilinks_sorted;
        for i in 1..sorted.len() {
            let comparison = sorted[i - 1]
                .display_text
                .len()
                .cmp(&sorted[i].display_text.len());
            assert_ne!(
                comparison,
                Ordering::Less,
                "Sorting violates length ordering at index {i}"
            );
        }
    }

    #[test]
    fn test_cache_file_cleanup() {
        // Create scope to ensure TempDir is dropped
        {
            let temp_dir = TempDir::new().unwrap();
            let cache_path = temp_dir.path().join(CACHE_FOLDER).join(CACHE_FILE);

            // Create a test file and image using `TestFileBuilder`
            TestFileBuilder::new()
                .with_content("# Test\n![test](test.png)".to_string())
                .with_title("Test Document".to_string())
                .create(&temp_dir, "test.md");

            TestFileBuilder::new()
                .with_content(vec![0xFF, 0xD8, 0xFF, 0xE0]) // Simple PNG header
                .create(&temp_dir, "test.png");

            // Create config that will create cache in temp dir
            let validated_config = test_utils::get_test_validated_config(&temp_dir, None);

            // First scan - creates cache with the image
            let _ = ObsidianRepository::new(&validated_config).unwrap();

            // Delete the image file
            fs::remove_file(temp_dir.path().join("test.png")).unwrap();

            // Second scan - should detect the deleted image
            let _ = ObsidianRepository::new(&validated_config).unwrap();

            // Verify cache was cleaned up
            let cache_content = read_to_string(&cache_path).unwrap();
            let cache: Value = from_str(&cache_content).unwrap();
            let cache = cache
                .as_object()
                .expect("cache file should deserialize to a JSON object");
            assert!(cache.is_empty(), "Cache should be empty after cleanup");

            // Exiting this block drops `TempDir` and removes the cached repository files.
        }

        // Try to create a new temp dir with the same path
        let new_temp = TempDir::new().unwrap();
        assert!(
            new_temp.path().exists(),
            "Should be able to create new temp dir"
        );
    }

    #[test]
    fn test_scan_folders_wikilink_collection() {
        let temp_dir = TempDir::new().unwrap();

        // Create first note using `TestFileBuilder`
        TestFileBuilder::new()
            .with_aliases(vec!["Alias One".to_string()])
            .with_content("# Note 1\n[[Simple Link]]".to_string())
            .create(&temp_dir, "note1.md");

        // Create second note using `TestFileBuilder`
        TestFileBuilder::new()
            .with_aliases(vec!["Alias Two".to_string()])
            .with_content("# Note 2\n[[Target|Display Text]]\n[[Simple Link]]".to_string())
            .create(&temp_dir, "note2.md");

        // Create minimal validated config
        let validated_config = test_support::get_test_validated_config(&temp_dir, None);

        // Scan the folders
        let obsidian_repository = ObsidianRepository::new(&validated_config).unwrap();

        // Filter for .md files only and exclude "obsidian knife output" explicitly
        let wikilinks: HashSet<String> = obsidian_repository
            .markdown_files
            .iter()
            .filter(|markdown_file| {
                markdown_file.path.extension().and_then(OsStr::to_str) == Some(MARKDOWN_EXTENSION)
            })
            .flat_map(|markdown_file| {
                let markdown_file =
                    MarkdownFile::new(markdown_file.path.clone(), DEFAULT_TIMEZONE).unwrap();
                let file_wikilinks = markdown_file.wikilinks.valid;
                file_wikilinks.into_iter().map(|w| w.display_text)
            })
            .filter(|link| link != "obsidian knife output")
            .collect();

        // Verify expected wikilinks are present
        assert!(wikilinks.contains("note1"), "Should contain first filename");
        assert!(
            wikilinks.contains("note2"),
            "Should contain second filename"
        );
        assert!(
            wikilinks.contains("Alias One"),
            "Should contain first alias"
        );
        assert!(
            wikilinks.contains("Alias Two"),
            "Should contain second alias"
        );
        assert!(
            wikilinks.contains("Simple Link"),
            "Should contain simple link"
        );
        assert!(
            wikilinks.contains("Display Text"),
            "Should contain display text from alias"
        );

        // Verify total count
        assert_eq!(
            wikilinks.len(),
            6,
            "Should have collected all unique wikilinks"
        );
    }

    fn eastern_date_wikilink(year: i32, month: u32, day: u32) -> String {
        test_utils::frontmatter_date_wikilink(test_utils::eastern_midnight(year, month, day))
    }

    #[test]
    #[cfg_attr(
        target_os = "linux",
        ignore = "requires filesystem access unavailable on Linux CI"
    )]
    fn test_update_modified_dates_changes_frontmatter() {
        let temp_dir = TempDir::new().unwrap();

        let base_date = test_utils::eastern_midnight(2024, 1, 15);
        let update_date = test_utils::eastern_midnight(2024, 1, 20);

        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(
                Some(eastern_date_wikilink(2024, 1, 15)),
                Some(eastern_date_wikilink(2024, 1, 15)),
            )
            .with_file_system_dates(base_date, base_date)
            .create(&temp_dir, "test1.md");

        let mut obsidian_repository = ObsidianRepository::default();
        let mut markdown_file = test_utils::get_test_markdown_file(file_path);

        // Instead of using mark_image_reference_as_updated which uses current date,
        // set the frontmatter dates directly
        if let Some(frontmatter) = &mut markdown_file.frontmatter {
            frontmatter.set_date_modified(update_date, DEFAULT_TIMEZONE);
        }
        markdown_file
            .persist_reasons
            .push(PersistReason::ImageReferencesModified);

        obsidian_repository.markdown_files.push(markdown_file);

        let frontmatter = obsidian_repository.markdown_files[0]
            .frontmatter
            .as_ref()
            .unwrap();

        assert_eq!(
            frontmatter.date_modified(),
            Some(test_utils::frontmatter_date_wikilink(update_date).as_str()),
            "Modified date should be update date"
        );
        assert_eq!(
            frontmatter.date_created(),
            Some(test_utils::frontmatter_date_wikilink(base_date).as_str()),
            "Created date should not have changed"
        );
        assert!(frontmatter.needs_persist(), "needs_persist should be true");
    }

    #[test]
    #[cfg_attr(
        target_os = "linux",
        ignore = "requires filesystem access unavailable on Linux CI"
    )]
    fn test_update_modified_dates_only_updates_specified_files() {
        let temp_dir = TempDir::new().unwrap();

        let base_date = test_utils::eastern_midnight(2024, 1, 15);
        let update_date = test_utils::eastern_midnight(2024, 1, 20);

        // Create two files
        let file_path1 = TestFileBuilder::new()
            .with_frontmatter_dates(
                Some(eastern_date_wikilink(2024, 1, 15)),
                Some(eastern_date_wikilink(2024, 1, 15)),
            )
            .with_file_system_dates(base_date, base_date)
            .create(&temp_dir, "test1.md");
        let file_path2 = TestFileBuilder::new()
            .with_frontmatter_dates(
                Some(eastern_date_wikilink(2024, 1, 15)),
                Some(eastern_date_wikilink(2024, 1, 15)),
            )
            .with_file_system_dates(base_date, base_date)
            .create(&temp_dir, "test2.md");

        let mut obsidian_repository = ObsidianRepository::default();
        let mut markdown_file1 = test_utils::get_test_markdown_file(file_path1);

        // `markdown_file1` is the only file with a fixed modified date.
        if let Some(frontmatter) = &mut markdown_file1.frontmatter {
            frontmatter.set_date_modified(update_date, DEFAULT_TIMEZONE);
        }
        markdown_file1
            .persist_reasons
            .push(PersistReason::ImageReferencesModified);

        obsidian_repository.markdown_files.push(markdown_file1);
        obsidian_repository
            .markdown_files
            .push(test_utils::get_test_markdown_file(file_path2));

        let file1 = &obsidian_repository.markdown_files[0];
        let file2 = &obsidian_repository.markdown_files[1];

        // First file should have new date and needs_persist
        assert_eq!(
            file1.frontmatter.as_ref().unwrap().date_modified(),
            Some(test_utils::frontmatter_date_wikilink(update_date).as_str())
        );
        assert!(file1.frontmatter.as_ref().unwrap().needs_persist());

        // Second file should have original date and not need persist
        assert_eq!(
            file2.frontmatter.as_ref().unwrap().date_modified(),
            Some(test_utils::frontmatter_date_wikilink(base_date).as_str())
        );
        assert!(!file2.frontmatter.as_ref().unwrap().needs_persist());
    }
}

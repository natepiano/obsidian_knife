mod back_populate;
mod constants;
mod date_validation;
mod image_link;
mod matching;
mod replaceable_content;
mod text_excluder;

use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use aho_corasick::AhoCorasick;
pub use back_populate::BackPopulateMatch;
pub use back_populate::MatchContext;
pub use date_validation::DateValidation;
pub use date_validation::PersistReason;
pub use image_link::ImageLink;
pub use image_link::ImageLinkState;
use regex::Regex;
pub use replaceable_content::MatchType;
pub use replaceable_content::ReplaceableContent;
pub use text_excluder::InlineCodeExcluder;

use self::back_populate::BackPopulateMatches;
use self::date_validation::DateCreatedFixValidation;
use self::image_link::ImageLinkTarget;
use self::image_link::ImageLinkType;
use self::image_link::ImageLinks;
use self::image_link::Wikilinks;
use self::text_excluder::CodeBlockExcluder;
use crate::constants::FRONTMATTER_DELIMITER_LINE_COUNT;
use crate::constants::YAML_CLOSING_DELIMITER;
use crate::constants::YAML_OPENING_DELIMITER;
use crate::frontmatter::FrontMatter;
use crate::support;
use crate::support::IMAGE_REGEX;
use crate::validated_config::ValidatedConfig;
use crate::wikilink;
use crate::wikilink::InvalidWikilink;
use crate::wikilink::Wikilink;
use crate::yaml_frontmatter;
use crate::yaml_frontmatter::YamlFrontMatter;
use crate::yaml_frontmatter::YamlFrontMatterError;

#[derive(Debug, Clone)]
pub(crate) struct MarkdownFile {
    pub(crate) content:                      String,
    pub(crate) date_created_fix:             DateCreatedFixValidation,
    pub(crate) created_validation:           DateValidation,
    pub(crate) modified_validation:          DateValidation,
    pub(crate) do_not_back_populate_regexes: Option<Vec<Regex>>,
    pub(crate) frontmatter:                  Option<FrontMatter>,
    pub(crate) frontmatter_error:            Option<YamlFrontMatterError>,
    pub(crate) frontmatter_line_count:       usize,
    pub(crate) image_links:                  ImageLinks,
    pub(crate) wikilinks:                    Wikilinks,
    pub(crate) matches:                      BackPopulateMatches,
    pub(crate) path:                         PathBuf,
    pub(crate) persist_reasons:              Vec<PersistReason>,
}

#[derive(Debug, Default)]
struct ExtractedWikilinks {
    valid:   Vec<Wikilink>,
    invalid: Vec<InvalidWikilink>,
}

impl MarkdownFile {
    pub(crate) fn new(
        path: PathBuf,
        operational_timezone: &str,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let full_content = support::read_contents_from_file(&path)?;

        let yaml_result = yaml_frontmatter::find_yaml_section(&full_content);
        let frontmatter_line_count = match &yaml_result {
            Ok(Some((yaml_section, _))) => {
                yaml_section.lines().count() + FRONTMATTER_DELIMITER_LINE_COUNT
            },
            _ => 0,
        };

        let (mut frontmatter, content, frontmatter_error) = match yaml_result {
            Ok(Some((yaml_section, after_yaml))) => {
                match FrontMatter::from_yaml_str(yaml_section) {
                    Ok(frontmatter) => (Some(frontmatter), after_yaml.to_string(), None),
                    Err(e) => (None, after_yaml.to_string(), Some(e)),
                }
            },
            Ok(None) => (None, full_content, Some(YamlFrontMatterError::Missing)),
            Err(e) => (None, full_content, Some(e)),
        };

        let (created_validation, modified_validation) = date_validation::get_date_validations(
            frontmatter.as_ref(),
            &path,
            operational_timezone,
        )?;

        let date_created_fix = DateCreatedFixValidation::from_frontmatter(
            frontmatter.as_ref(),
            created_validation.file_system,
            operational_timezone,
        );

        let persist_reasons = date_validation::process_date_validations(
            &mut frontmatter,
            &created_validation,
            &modified_validation,
            &date_created_fix,
            operational_timezone,
        );

        let do_not_back_populate_regexes = frontmatter
            .as_ref()
            .and_then(FrontMatter::get_do_not_back_populate_regexes);

        let mut markdown_file = Self {
            content,
            date_created_fix,
            do_not_back_populate_regexes,
            created_validation,
            modified_validation,
            frontmatter,
            frontmatter_error,
            frontmatter_line_count,
            wikilinks: Wikilinks::default(),
            image_links: ImageLinks::default(),
            matches: BackPopulateMatches::default(),
            path,
            persist_reasons,
        };

        let extracted_wikilinks = markdown_file.process_wikilinks();
        let image_links = markdown_file.process_image_links();

        // Store results directly in `self`.
        markdown_file.wikilinks.invalid = extracted_wikilinks.invalid;
        markdown_file.wikilinks.valid = extracted_wikilinks.valid;
        markdown_file.image_links.links = image_links;

        Ok(markdown_file)
    }

    pub(crate) fn process_file_for_back_populate_replacements(
        &mut self,
        sorted_wikilinks: &[&Wikilink],
        config: &ValidatedConfig,
        automaton: &AhoCorasick,
    ) {
        self.process_file_for_back_populate_replacements_inner(sorted_wikilinks, config, automaton);
    }

    fn to_full_content(&self) -> String {
        self.frontmatter.as_ref().map_or_else(
            || self.content.clone(),
            |frontmatter| {
                frontmatter.to_yaml_str().map_or_else(
                    |_| self.content.clone(),
                    |yaml| {
                        format!(
                            "{YAML_OPENING_DELIMITER}{}\n{YAML_CLOSING_DELIMITER}{}",
                            yaml.trim(),
                            self.content.trim()
                        )
                    },
                )
            },
        )
    }

    pub(crate) fn persist(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        fs::write(&self.path, self.to_full_content())?;

        let Some(frontmatter) = self.frontmatter.as_ref() else {
            return Err("frontmatter is required to persist a markdown file".into());
        };
        let modified_date = frontmatter
            .raw_modified
            .ok_or_else(|| "raw_date_modified must be set for persist".to_string())?;
        let created_date = frontmatter.raw_created;

        support::set_file_dates(&self.path, created_date, modified_date)?;

        Ok(())
    }

    fn ensure_frontmatter(&mut self, operational_timezone: &str) {
        if self.frontmatter.is_none() {
            let mut frontmatter = FrontMatter::default();
            frontmatter.set_date_created(self.created_validation.file_system, operational_timezone);
            self.frontmatter = Some(frontmatter);
            self.frontmatter_error = None;
            self.persist_reasons.push(PersistReason::FrontmatterCreated);
        }
    }

    pub(crate) fn mark_as_back_populated(
        &mut self,
        operational_timezone: &str,
    ) -> anyhow::Result<()> {
        self.ensure_frontmatter(operational_timezone);

        // Remove any `DateModifiedUpdated` reasons since we'll be setting the date to now.
        // This way we won't show extraneous results in `persist_reasons_report`.
        self.persist_reasons
            .retain(|reason| !matches!(reason, PersistReason::DateModifiedUpdated { .. }));

        let frontmatter = self.frontmatter.as_mut().ok_or_else(|| {
            anyhow::anyhow!(
                "frontmatter missing after ensure_frontmatter for {}",
                self.path.display()
            )
        })?;
        frontmatter.set_date_modified_now(operational_timezone);
        self.persist_reasons.push(PersistReason::BackPopulated);
        Ok(())
    }

    pub(crate) fn mark_image_reference_as_updated(
        &mut self,
        operational_timezone: &str,
    ) -> anyhow::Result<()> {
        self.ensure_frontmatter(operational_timezone);

        let frontmatter = self.frontmatter.as_mut().ok_or_else(|| {
            anyhow::anyhow!(
                "frontmatter missing after ensure_frontmatter for {}",
                self.path.display()
            )
        })?;
        frontmatter.set_date_modified_now(operational_timezone);
        self.persist_reasons
            .push(PersistReason::ImageReferencesModified);
        Ok(())
    }

    fn process_wikilinks(&self) -> ExtractedWikilinks {
        let mut result = ExtractedWikilinks::default();

        let aliases = self
            .frontmatter
            .as_ref()
            .and_then(|frontmatter| frontmatter.aliases().map(<[String]>::to_vec));

        let filename = self
            .path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default();

        let filename_wikilink = wikilink::create_filename_wikilink(filename);
        result.valid.push(filename_wikilink.clone());

        if let Some(alias_list) = aliases {
            for alias in alias_list {
                let wikilink = Wikilink {
                    display_text: alias.clone(),
                    target:       filename_wikilink.target.clone(),
                };
                result.valid.push(wikilink);
            }
        }

        let mut state = CodeBlockExcluder::new();

        for (line_idx, line) in self.content.lines().enumerate() {
            state.update(line);
            if state.is_in_code_block() {
                continue;
            }

            let extracted = wikilink::extract_wikilinks(line);
            result.valid.extend(extracted.valid);

            let invalid_with_lines: Vec<InvalidWikilink> = extracted
                .invalid
                .into_iter()
                .map(|parsed| {
                    parsed.into_invalid_wikilink(
                        line.to_string(),
                        self.get_real_line_number(line_idx),
                    )
                })
                .collect();
            result.invalid.extend(invalid_with_lines);
        }

        result
    }

    fn process_image_links(&self) -> Vec<ImageLink> {
        let mut image_links = Vec::new();

        for (line_idx, line) in self.content.lines().enumerate() {
            for capture in IMAGE_REGEX.captures_iter(line) {
                if let Some(raw_image_link) = capture.get(0) {
                    let Ok(image_link) = ImageLink::new(
                        raw_image_link.as_str().to_string(),
                        self.get_real_line_number(line_idx),
                        raw_image_link.start(),
                    ) else {
                        continue;
                    };
                    match image_link.link_type {
                        ImageLinkType::Wiki(_)
                        | ImageLinkType::Markdown(ImageLinkTarget::Internal, _) => {
                            image_links.push(image_link);
                        },
                        ImageLinkType::Markdown(..) => {},
                    }
                }
            }
        }

        image_links
    }

    const fn get_real_line_number(&self, line_idx: usize) -> usize {
        self.frontmatter_line_count + line_idx + 1
    }

    pub(crate) const fn has_ambiguous_matches(&self) -> bool { !self.matches.ambiguous.is_empty() }

    pub(crate) const fn has_unambiguous_matches(&self) -> bool {
        !self.matches.unambiguous.is_empty()
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
    use std::error::Error;
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;

    use filetime::FileTime;
    use tempfile::TempDir;

    use super::BackPopulateMatch;
    use super::ImageLink;
    use super::MarkdownFile;
    use super::MatchContext;
    use super::PersistReason;
    use super::date_validation::DateValidationIssue;
    use super::image_link::ImageLinkTarget;
    use super::image_link::ImageLinkType;
    use super::image_link::ImageRendering;
    use crate::constants::DEFAULT_TIMEZONE;
    use crate::constants::FRONTMATTER_DELIMITER_LINE_COUNT;
    use crate::constants::YAML_CLOSING_DELIMITER_NEWLINE;
    use crate::constants::YAML_OPENING_DELIMITER;
    use crate::markdown_files::MarkdownFiles;
    use crate::support::IMAGE_REGEX;
    use crate::test_support;
    use crate::test_support as test_utils;
    use crate::test_support::AliasExpectation;
    use crate::test_support::TestFileBuilder;
    use crate::validated_config::ChangeMode;
    use crate::wikilink::InvalidWikilinkReason;
    use crate::wikilink::Wikilink;

    #[test]
    fn test_parse_content_separation() {
        let temp_dir = TempDir::new().unwrap();

        // Test 1: File with frontmatter and content
        let file_with_fm = TestFileBuilder::new()
            .with_title("Test".to_string())
            .with_content("This is the actual content")
            .create(&temp_dir, "with_fm.md");

        let mfi = test_utils::get_test_markdown_file(file_with_fm);
        assert_eq!(mfi.content.trim(), "This is the actual content");

        // Test 2: File with no frontmatter
        let file_no_fm = TestFileBuilder::new()
            .with_content("Pure content\nNo frontmatter")
            .create(&temp_dir, "no_fm.md");

        let mfi = test_utils::get_test_markdown_file(file_no_fm);
        assert_eq!(mfi.content.trim(), "Pure content\nNo frontmatter");

        // Test 3: File with --- separators in content
        let delimiter = YAML_OPENING_DELIMITER.trim_end();
        let content = format!("First line\n{delimiter}\nMiddle section\n{delimiter}\nLast section");
        let file_with_separators = TestFileBuilder::new()
            .with_title("Test".to_string())
            .with_content(content.clone())
            .create(&temp_dir, "with_separators.md");

        let mfi = test_utils::get_test_markdown_file(file_with_separators);
        assert_eq!(mfi.content.trim(), content);
    }

    fn create_test_file(content: &str, temp_dir: &Path) -> PathBuf {
        let file_path = temp_dir.join("test.md");
        fs::write(&file_path, content).unwrap();
        file_path
    }

    fn expected_frontmatter_line_count(yaml_line_count: usize) -> usize {
        yaml_line_count + FRONTMATTER_DELIMITER_LINE_COUNT
    }

    #[test]
    fn test_frontmatter_line_counting() {
        let temp_dir = TempDir::new().unwrap();

        let test_cases = vec![
            (
                format!(
                    "{YAML_OPENING_DELIMITER}title: test{YAML_CLOSING_DELIMITER_NEWLINE}Content"
                ),
                expected_frontmatter_line_count(1), // 1 line of YAML plus delimiter lines
            ),
            (
                format!(
                    "{YAML_OPENING_DELIMITER}title: test\ntags: [a,b]{YAML_CLOSING_DELIMITER_NEWLINE}Content"
                ),
                expected_frontmatter_line_count(2), // 2 lines of YAML plus delimiter lines
            ),
            (
                format!(
                    "{YAML_OPENING_DELIMITER}title: test\ntags:\n  - a\n  - b{YAML_CLOSING_DELIMITER_NEWLINE}Content"
                ),
                expected_frontmatter_line_count(4), // 4 lines of YAML plus delimiter lines
            ),
        ];

        for (content, expected_frontmatter_lines) in &test_cases {
            let file_path = create_test_file(content, temp_dir.path());
            let markdown_file = MarkdownFile::new(file_path, "UTC").unwrap();
            assert_eq!(
                markdown_file.frontmatter_line_count, *expected_frontmatter_lines,
                "Failed for content:\n{content}"
            );
        }
    }

    #[test]
    fn test_date_validation_persist_reasons() -> Result<(), Box<dyn Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;

        // Test missing dates
        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(None, None)
            .with_title("test".to_string()) // to force valid frontmatter with missing dates
            .create(&temp_dir, "missing_dates.md");

        let markdown_file = test_utils::get_test_markdown_file(file_path);

        assert!(
            markdown_file
                .persist_reasons
                .contains(&PersistReason::DateCreatedUpdated {
                    reason: DateValidationIssue::Missing,
                })
        );
        assert!(
            markdown_file
                .persist_reasons
                .contains(&PersistReason::DateModifiedUpdated {
                    reason: DateValidationIssue::Missing,
                })
        );

        // Test invalid format dates
        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(
                Some("[[2024-13-45]]".to_string()),
                Some("[[2024-13-45]]".to_string()),
            )
            .create(&temp_dir, "invalid_dates.md");

        let markdown_file = test_utils::get_test_markdown_file(file_path);

        assert!(
            markdown_file
                .persist_reasons
                .contains(&PersistReason::DateCreatedUpdated {
                    reason: DateValidationIssue::InvalidFormat,
                })
        );
        assert!(
            markdown_file
                .persist_reasons
                .contains(&PersistReason::DateModifiedUpdated {
                    reason: DateValidationIssue::InvalidFormat,
                })
        );

        Ok(())
    }

    #[test]
    fn test_date_created_fix_persist_reason() -> Result<(), Box<dyn Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;
        let test_date = test_utils::eastern_midnight(2024, 1, 15);

        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(
                Some("[[2024-01-15]]".to_string()),
                Some("[[2024-01-15]]".to_string()),
            )
            .with_fs_dates(test_date, test_date)
            .with_date_created_fix(Some("2024-01-01".to_string()))
            .create(&temp_dir, "date_fix.md");

        let markdown_file = test_utils::get_test_markdown_file(file_path);

        assert!(
            markdown_file
                .persist_reasons
                .contains(&PersistReason::DateCreatedFixApplied)
        );

        Ok(())
    }

    #[test]
    fn test_back_populate_persist_reason() -> Result<(), Box<dyn Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;
        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(
                Some("[[2024-01-15]]".to_string()),
                Some("[[2024-01-15]]".to_string()),
            )
            .create(&temp_dir, "back_populate.md");

        let mut markdown_file = test_utils::get_test_markdown_file(file_path);
        markdown_file.mark_as_back_populated(DEFAULT_TIMEZONE)?;

        assert!(
            markdown_file
                .persist_reasons
                .contains(&PersistReason::BackPopulated)
        );

        Ok(())
    }

    #[test]
    fn test_image_references_persist_reason() -> Result<(), Box<dyn Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;
        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(
                Some("[[2024-01-15]]".to_string()),
                Some("[[2024-01-15]]".to_string()),
            )
            .create(&temp_dir, "image_refs.md");

        let mut markdown_file = test_utils::get_test_markdown_file(file_path);
        markdown_file.mark_image_reference_as_updated(DEFAULT_TIMEZONE)?;

        assert!(
            markdown_file
                .persist_reasons
                .contains(&PersistReason::ImageReferencesModified)
        );

        Ok(())
    }

    #[test]
    fn test_multiple_persist_reasons() -> Result<(), Box<dyn Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;
        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(None, None)
            .with_title("test".to_string()) // to force frontmatter creation
            .create(&temp_dir, "multiple_reasons.md");

        let mut markdown_file = test_utils::get_test_markdown_file(file_path);

        // This will add DateCreatedUpdated and DateModifiedUpdated
        assert!(
            markdown_file
                .persist_reasons
                .contains(&PersistReason::DateCreatedUpdated {
                    reason: DateValidationIssue::Missing,
                })
        );

        // Add back populate reason
        markdown_file.mark_as_back_populated(DEFAULT_TIMEZONE)?;

        // Add image reference change
        markdown_file.mark_image_reference_as_updated(DEFAULT_TIMEZONE)?;

        // Verify all reasons are present
        // the 3 reasons are DateCreatedUpdated { reason: Missing }, BackPopulated,
        // ImageReferencesModified we don't have an update date missing because if we
        // BackPopulate we automatically remove the modified date reason
        assert_eq!(markdown_file.persist_reasons.len(), 3);
        assert!(
            markdown_file
                .persist_reasons
                .contains(&PersistReason::BackPopulated)
        );
        assert!(
            markdown_file
                .persist_reasons
                .contains(&PersistReason::ImageReferencesModified)
        );

        Ok(())
    }

    #[test]
    fn test_persist_frontmatter() -> Result<(), Box<dyn Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;
        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(Some("2024-01-01".to_string()), None)
            .create(&temp_dir, "test.md");

        let mut markdown_file = test_utils::get_test_markdown_file(file_path.clone());

        // Update frontmatter directly
        if let Some(frontmatter) = &mut markdown_file.frontmatter {
            let created_date = test_utils::eastern_midnight(2024, 1, 2); // Instead of parse_datetime
            frontmatter.set_date_created(created_date, DEFAULT_TIMEZONE);
        }

        markdown_file.persist()?;

        // Verify frontmatter was updated but content preserved
        let updated_content = fs::read_to_string(&file_path)?;
        assert!(
            updated_content.contains("[[2024-01-02]]"),
            "Content '{updated_content}' does not contain expected date string"
        );
        assert!(updated_content.contains("Test content"));

        Ok(())
    }

    #[test]
    fn test_persist_frontmatter_preserves_format() -> Result<(), Box<dyn Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;
        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(Some("2024-01-01".to_string()), None)
            .with_tags(vec!["tag1".to_string(), "tag2".to_string()])
            .create(&temp_dir, "test.md");

        let mut markdown_file = test_utils::get_test_markdown_file(file_path.clone());

        if let Some(frontmatter) = &mut markdown_file.frontmatter {
            let created_date = test_utils::eastern_midnight(2024, 1, 2); // Instead of parse_datetime
            frontmatter.set_date_created(created_date, DEFAULT_TIMEZONE);
        }

        markdown_file.persist()?;

        let updated_content = fs::read_to_string(&file_path)?;
        assert!(updated_content.contains("tags:\n- tag1\n- tag2"));
        assert!(updated_content.contains("[[2024-01-02]]"));

        Ok(())
    }

    #[test]
    #[cfg_attr(
        target_os = "linux",
        ignore = "requires filesystem access unavailable on Linux CI"
    )]
    fn test_persist_with_created_and_modified_dates() -> Result<(), Box<dyn Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;

        // Define the created and modified dates
        let created_date = test_utils::parse_datetime("2024-01-05 10:00:00");
        let modified_date = test_utils::parse_datetime("2024-01-06 15:30:00");

        // Use with_matching_dates to set both frontmatter and file system dates
        let file_path = TestFileBuilder::new()
            .with_matching_dates(created_date) // Set both FS and frontmatter dates to created_date
            .create(&temp_dir, "test_with_both_dates.md");

        let mut markdown_file = test_utils::get_test_markdown_file(file_path.clone());

        if let Some(frontmatter) = &mut markdown_file.frontmatter {
            // Update the frontmatter to match the intended created and modified dates
            frontmatter.raw_created = Some(created_date);
            frontmatter.raw_modified = Some(modified_date);
            frontmatter.set_date_created(created_date, DEFAULT_TIMEZONE); // Ensure frontmatter reflects this change
            frontmatter.set_date_modified(modified_date, DEFAULT_TIMEZONE);
        }

        markdown_file.persist()?;

        let metadata_after = fs::metadata(&file_path)?;
        let created_time_after = FileTime::from_creation_time(&metadata_after).unwrap();
        let modified_time_after = FileTime::from_last_modification_time(&metadata_after);

        assert_eq!(created_time_after.unix_seconds(), created_date.timestamp());
        assert_eq!(
            modified_time_after.unix_seconds(),
            modified_date.timestamp()
        );

        Ok(())
    }

    #[test]
    fn test_disallow_persist_if_date_modified_not_set() {
        let temp_dir = TempDir::new().unwrap();

        // Use with_matching_dates to set consistent creation and modification dates
        let matching_date = test_utils::eastern_midnight(2024, 1, 1); // ("2024-01-01 00:00:00");
        let file_path = TestFileBuilder::new()
            .with_matching_dates(matching_date)
            .create(&temp_dir, "test_invalid_state.md");

        let mut markdown_file = test_utils::get_test_markdown_file(file_path);

        // Simulate the absence of `raw_date_modified` by explicitly removing it
        if let Some(frontmatter) = &mut markdown_file.frontmatter {
            frontmatter.raw_modified = None;
        }

        // Attempt to persist and expect an error
        let result = markdown_file.persist();

        assert!(
            result.is_err(),
            "Expected an error, but persist completed successfully"
        );

        if let Err(err) = result {
            assert_eq!(
                err.to_string(),
                "raw_date_modified must be set for persist",
                "Unexpected error message"
            );
        }
    }

    #[test]
    fn test_persist_preserves_file_content() -> Result<(), Box<dyn Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;
        let file_path = TestFileBuilder::new()
            .with_title("Test Title".to_string())
            .with_content("Sample content for testing")
            .with_frontmatter_dates(
                Some("2024-01-01".to_string()),
                Some("2024-01-02".to_string()),
            )
            .create(&temp_dir, "test_content_preservation.md");

        let mut markdown_file = test_utils::get_test_markdown_file(file_path.clone());

        if let Some(frontmatter) = &mut markdown_file.frontmatter {
            frontmatter.set_date_created(
                test_utils::parse_datetime("2024-01-03 10:00:00"),
                DEFAULT_TIMEZONE,
            );
            frontmatter.set_date_modified(
                test_utils::parse_datetime("2024-01-04 15:00:00"),
                DEFAULT_TIMEZONE,
            );
        }

        markdown_file.persist()?;

        // Verify that the file content remains unchanged except for the frontmatter
        let updated_content = fs::read_to_string(&file_path)?;
        assert!(updated_content.contains("Sample content for testing"));
        assert!(updated_content.contains("[[2024-01-03]]"));
        assert!(updated_content.contains("[[2024-01-04]]"));

        Ok(())
    }

    #[test]
    fn test_ensure_frontmatter_creates_frontmatter_on_back_populate()
    -> Result<(), Box<dyn Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;

        // Create a file without frontmatter
        let file_path = TestFileBuilder::new()
            .with_content("Some text that mentions a wikilink target")
            .create(&temp_dir, "no_frontmatter.md");

        let mut markdown_file = test_utils::get_test_markdown_file(file_path);

        // Confirm starting state: no frontmatter, has frontmatter error
        assert!(markdown_file.frontmatter.is_none());
        assert!(markdown_file.frontmatter_error.is_some());

        markdown_file.mark_as_back_populated(DEFAULT_TIMEZONE)?;

        // Frontmatter was created
        assert!(markdown_file.frontmatter.is_some());
        let frontmatter = markdown_file.frontmatter.as_ref().expect("just confirmed");

        // `date_created` set from filesystem date
        assert!(frontmatter.created.is_some());
        assert!(frontmatter.raw_created.is_some());

        // `date_modified` set (by `set_date_created` auto-call and then
        // `set_date_modified_now`)
        assert!(frontmatter.modified.is_some());
        assert!(frontmatter.raw_modified.is_some());

        // Frontmatter error cleared
        assert!(markdown_file.frontmatter_error.is_none());

        // Persist reasons include both `FrontmatterCreated` and `BackPopulated`
        assert!(
            markdown_file
                .persist_reasons
                .contains(&PersistReason::FrontmatterCreated)
        );
        assert!(
            markdown_file
                .persist_reasons
                .contains(&PersistReason::BackPopulated)
        );

        Ok(())
    }

    fn assert_contains_wikilink(
        wikilinks: &[Wikilink],
        target: &str,
        display: Option<&str>,
        alias_expectation: AliasExpectation,
    ) {
        let exists = wikilinks.iter().any(|w| {
            w.target == target
                && w.display_text == display.unwrap_or(target)
                && w.is_alias() == alias_expectation.is_alias()
        });
        assert!(
            exists,
            "Expected wikilink with target '{target}', display '{display:?}', alias '{}'",
            alias_expectation.is_alias()
        );
    }

    #[test]
    fn test_process_content_with_aliases() {
        let content = "# Test\nHere's a [[Regular Link]] and [[Target|Display Text]]";
        let aliases = Some(vec!["Alias One".to_string(), "Alias Two".to_string()]);

        let temp_dir = TempDir::new().unwrap();
        let file_path = TestFileBuilder::new()
            .with_content(content.to_string())
            .with_aliases(aliases.as_ref().unwrap_or(&Vec::new()).clone())
            .create(&temp_dir, "test file.md");

        let markdown_file = MarkdownFile::new(file_path, "UTC").unwrap();
        let extracted = markdown_file.process_wikilinks();
        let image_links = markdown_file.process_image_links();

        // Verify expected wikilinks
        assert_contains_wikilink(
            &extracted.valid,
            "test file",
            None,
            AliasExpectation::DirectLink,
        );
        assert_contains_wikilink(
            &extracted.valid,
            "test file",
            Some("Alias One"),
            AliasExpectation::Aliased,
        );
        assert_contains_wikilink(
            &extracted.valid,
            "test file",
            Some("Alias Two"),
            AliasExpectation::Aliased,
        );
        assert_contains_wikilink(
            &extracted.valid,
            "Regular Link",
            None,
            AliasExpectation::DirectLink,
        );
        assert_contains_wikilink(
            &extracted.valid,
            "Target",
            Some("Display Text"),
            AliasExpectation::Aliased,
        );

        // Verify no invalid wikilinks in this case
        assert!(
            extracted.invalid.is_empty(),
            "Should not have invalid wikilinks"
        );

        // Verify no image links in this case
        assert!(image_links.is_empty(), "Should not have image links");
    }

    #[test]
    fn test_process_content_with_invalid() {
        let content = "Some [[good link]] and [[bad|link|extra]] here\n[[unmatched";

        let temp_dir = TempDir::new().unwrap();
        let file_path = TestFileBuilder::new()
            .with_content(content.to_string())
            .create(&temp_dir, "test.md");

        let markdown_file = MarkdownFile::new(file_path, "UTC").unwrap();
        let extracted = markdown_file.process_wikilinks();
        let image_links = markdown_file.process_image_links();

        // Check valid wikilinks
        assert_contains_wikilink(&extracted.valid, "test", None, AliasExpectation::DirectLink);
        assert_contains_wikilink(
            &extracted.valid,
            "good link",
            None,
            AliasExpectation::DirectLink,
        );

        // Verify invalid wikilinks with line information
        assert_eq!(
            extracted.invalid.len(),
            2,
            "Should have exactly two invalid wikilinks"
        );

        // Find and verify the double alias invalid wikilink
        let double_alias = extracted
            .invalid
            .iter()
            .find(|w| w.reason == InvalidWikilinkReason::DoubleAlias)
            .expect("Should have a double alias invalid wikilink");

        assert_eq!(double_alias.line_number, 1);
        assert_eq!(
            double_alias.line,
            "Some [[good link]] and [[bad|link|extra]] here"
        );
        assert_eq!(double_alias.content, "[[bad|link|extra]]");

        // Find and verify the unmatched opening invalid wikilink
        let unmatched = extracted
            .invalid
            .iter()
            .find(|w| w.reason == InvalidWikilinkReason::UnmatchedOpening)
            .expect("Should have an unmatched opening invalid wikilink");

        assert_eq!(unmatched.line_number, 2);
        assert_eq!(unmatched.line, "[[unmatched");
        assert_eq!(unmatched.content, "[[unmatched");

        // Verify no image links
        assert!(image_links.is_empty(), "Should not have image links");
    }

    #[test]
    fn test_process_content_with_empty() {
        let content = "Test [[]] here\nAnd [[|]] there";

        let temp_dir = TempDir::new().unwrap();
        let file_path = TestFileBuilder::new()
            .with_content(content.to_string())
            .create(&temp_dir, "test.md");

        let markdown_file = MarkdownFile::new(file_path, "UTC").unwrap();
        let extracted = markdown_file.process_wikilinks();
        let image_links = markdown_file.process_image_links();

        assert_eq!(
            extracted.invalid.len(),
            2,
            "Should have two invalid empty wikilinks"
        );

        // Verify first empty wikilink
        let first_empty = &extracted.invalid[0];
        assert_eq!(first_empty.line_number, 1);
        assert_eq!(first_empty.line, "Test [[]] here");
        assert_eq!(first_empty.content, "[[]]");
        assert_eq!(first_empty.reason, InvalidWikilinkReason::Empty);

        // Verify second empty wikilink
        let second_empty = &extracted.invalid[1];
        assert_eq!(second_empty.line_number, 2);
        assert_eq!(second_empty.line, "And [[|]] there");
        assert_eq!(second_empty.content, "[[|]]");
        assert_eq!(second_empty.reason, InvalidWikilinkReason::Empty);

        // Verify no image links
        assert!(image_links.is_empty(), "Should not have image links");
    }

    #[test]
    fn test_process_content_with_images() {
        let content = "# Test\n![[image.png]]\nHere's a [[link]] and ![[another.jpg]]";

        let temp_dir = TempDir::new().unwrap();
        let file_path = TestFileBuilder::new()
            .with_content(content.to_string())
            .create(&temp_dir, "test.md");

        let markdown_file = MarkdownFile::new(file_path, "UTC").unwrap();
        let extracted = markdown_file.process_wikilinks();
        let image_links = markdown_file.process_image_links();

        // Check wikilinks
        assert_contains_wikilink(&extracted.valid, "test", None, AliasExpectation::DirectLink);
        assert_contains_wikilink(&extracted.valid, "link", None, AliasExpectation::DirectLink);

        // Check image links
        assert_eq!(image_links.len(), 2, "Should have two image links");

        // Optionally, also test the filenames were extracted correctly
        assert!(image_links.iter().any(|link| link.filename == "image.png"));
        assert!(
            image_links
                .iter()
                .any(|link| link.filename == "another.jpg")
        );
    }

    #[derive(Debug)]
    struct ImageLinkTestCase {
        input:     &'static str,
        filename:  &'static str,
        link_type: ImageLinkType,
    }

    impl ImageLinkTestCase {
        const fn new(
            input: &'static str,
            filename: &'static str,
            link_type: ImageLinkType,
        ) -> Self {
            Self {
                input,
                filename,
                link_type,
            }
        }
    }

    #[test]
    fn test_image_link_types() {
        let test_cases = [
            // Wikilinks
            ImageLinkTestCase::new(
                "![[image.png]]",
                "image.png",
                ImageLinkType::Wiki(ImageRendering::Embedded),
            ),
            ImageLinkTestCase::new(
                "[[image.jpg]]",
                "image.jpg",
                ImageLinkType::Wiki(ImageRendering::Linked),
            ),
            ImageLinkTestCase::new(
                "![[image.png|alt text]]",
                "image.png",
                ImageLinkType::Wiki(ImageRendering::Embedded),
            ),
            // Markdown Internal Links
            ImageLinkTestCase::new(
                "![alt](image.png)",
                "image.png",
                ImageLinkType::Markdown(ImageLinkTarget::Internal, ImageRendering::Embedded),
            ),
            ImageLinkTestCase::new(
                "[alt](image.jpg)",
                "image.jpg",
                ImageLinkType::Markdown(ImageLinkTarget::Internal, ImageRendering::Linked),
            ),
            // Markdown External Links
            ImageLinkTestCase::new(
                "![alt](https://example.com/image.png)",
                "https://example.com/image.png",
                ImageLinkType::Markdown(ImageLinkTarget::External, ImageRendering::Embedded),
            ),
            ImageLinkTestCase::new(
                "[alt](https://example.com/image.jpg)",
                "https://example.com/image.jpg",
                ImageLinkType::Markdown(ImageLinkTarget::External, ImageRendering::Linked),
            ),
        ];

        for case in &test_cases {
            let captures = IMAGE_REGEX.captures(case.input).unwrap_or_else(|| {
                panic!("Regex failed to match valid image link: {}", case.input)
            });

            let raw_image_link = captures
                .get(0)
                .unwrap_or_else(|| panic!("Failed to get capture group for: {}", case.input))
                .as_str();

            // Add line number 1 and position 0 as test defaults
            let image_link = ImageLink::new(raw_image_link.to_string(), 1, 0).unwrap();

            assert_eq!(
                image_link.filename, case.filename,
                "Filename mismatch for input: {}",
                case.input
            );
            assert_eq!(
                image_link.link_type, case.link_type,
                "ImageLinkType mismatch for input: {}",
                case.input
            );
        }
    }

    #[test]
    fn test_should_create_match_in_table() {
        // Set up the test environment
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, None, None);
        let file_path = temp_dir.path().join("test.md");

        let markdown_file =
            MarkdownFile::new(file_path, validated_config.operational_timezone()).unwrap();

        // Test simple table cell match
        assert!(markdown_file.should_create_match("| Test Link | description |", 2, "Test Link",));

        // Test match in table with existing wikilinks
        assert!(markdown_file.should_create_match("| Test Link | [[Other]] |", 2, "Test Link",));
    }

    #[test]
    fn test_back_populate_content() {
        // Initialize environment with `apply_changes` set to true
        let (temp_dir, validated_config, mut obsidian_repository) =
            test_support::create_test_environment(ChangeMode::Apply, None, None, None);

        let test_cases = vec![(
            "# Test Table\n|Name|Description|\n|---|---|\n|Test Link|Sample text|\n",
            vec![BackPopulateMatch {
                relative_path: "test.md".into(),
                line_number:   4,
                line_text:     "|Test Link|Sample text|".into(),
                found_text:    "Test Link".into(),
                replacement:   "[[Test Link\\|Another Name]]".into(),
                position:      1,
                match_context: MatchContext::MarkdownTable,
            }],
            "Table content replacement",
        )];

        for (content, matches, description) in test_cases {
            // Create and populate the test file
            let file = TestFileBuilder::new()
                .with_content(content.to_string())
                .with_title("test".to_string())
                .create(&temp_dir, "test.md");

            // Prepare markdown info and repository state
            let markdown_file = {
                let mut markdown_file =
                    MarkdownFile::new(file.clone(), validated_config.operational_timezone())
                        .unwrap();
                markdown_file.content = content.to_string();
                markdown_file.matches.unambiguous = matches.clone();
                markdown_file
            };

            obsidian_repository.markdown_files = MarkdownFiles::new(vec![markdown_file], None);

            // Apply back-populate changes
            obsidian_repository
                .apply_replaceable_matches(validated_config.operational_timezone())
                .unwrap();

            // Validate replacements
            if let Some(file) = obsidian_repository
                .markdown_files
                .iter()
                .find(|f| f.path == file)
            {
                for match_info in &matches {
                    assert!(
                        file.content.contains(&match_info.replacement),
                        "Failed for: {}\nReplacement '{}' not found in content:\n{}",
                        description,
                        match_info.replacement,
                        file.content
                    );
                }
            }
        }
    }

    #[test]
    fn test_process_line_table_escaping_combined() {
        // Define multiple wikilinks
        let wikilinks = vec![
            Wikilink {
                display_text: "Another Link".to_string(),
                target:       "Other Page".to_string(),
            },
            Wikilink {
                display_text: "Test Link".to_string(),
                target:       "Target Page".to_string(),
            },
        ];

        // Initialize environment with custom wikilinks
        let (temp_dir, validated_config, obsidian_repository) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(wikilinks), None);

        // Compile the wikilinks
        let sorted_wikilinks = &obsidian_repository.wikilinks_sorted;

        let automaton = test_support::build_aho_corasick(sorted_wikilinks);

        let markdown_file = obsidian_repository.markdown_files.first().unwrap();

        // Define test cases with different table formats and expected replacements
        let test_cases = vec![
            (
                "| Test Link | Another Link | description |",
                vec![
                    "[[Target Page\\|Test Link]]",
                    "[[Other Page\\|Another Link]]",
                ],
                "Multiple matches in one row",
            ),
            (
                "| prefix Test Link suffix | Another Link |",
                vec![
                    "[[Target Page\\|Test Link]]",
                    "[[Other Page\\|Another Link]]",
                ],
                "Table cells with surrounding text",
            ),
            (
                "| column1 | Test Link | Another Link |",
                vec![
                    "[[Target Page\\|Test Link]]",
                    "[[Other Page\\|Another Link]]",
                ],
                "Different column positions",
            ),
            (
                "| Test Link | description | Another Link |",
                vec![
                    "[[Target Page\\|Test Link]]",
                    "[[Other Page\\|Another Link]]",
                ],
                "Multiple replacements in different columns",
            ),
        ];

        // Create references to the compiled wikilinks
        let wikilink_refs: Vec<&Wikilink> = sorted_wikilinks.iter().collect();
        for (line, expected_replacements, description) in test_cases {
            // Create test file using `TestFileBuilder`
            let _ = TestFileBuilder::new()
                .with_title("test".to_string())
                .with_content(line.to_string())
                .create(&temp_dir, "test.md");

            let matches = markdown_file.process_line_for_back_populate_replacements(
                line,
                0,
                &automaton,
                &wikilink_refs,
                &validated_config,
            );

            assert_eq!(
                matches.len(),
                expected_replacements.len(),
                "Incorrect number of replacements for: {description}"
            );

            for (match_info, expected) in matches.iter().zip(expected_replacements.iter()) {
                assert_eq!(
                    match_info.replacement, *expected,
                    "Incorrect replacement for: {description}"
                );
                assert_eq!(
                    match_info.match_context,
                    MatchContext::MarkdownTable,
                    "Should be marked as in table for: {description}"
                );
            }
        }
    }
}

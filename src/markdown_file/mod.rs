mod back_populate;
mod canonical_link;
mod constants;
mod image_link;
mod persist_reason;
mod phantom_link;
mod replaceable_content;
mod text_excluder;

use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use aho_corasick::AhoCorasick;
use anyhow::Result as AnyhowResult;
use anyhow::anyhow;
pub use back_populate::BackPopulateMatch;
pub use back_populate::MatchContext;
pub use canonical_link::CanonicalLinkMatch;
pub use image_link::ImageLink;
pub use image_link::ImageLinkState;
pub use persist_reason::PersistReason;
pub use phantom_link::PhantomLinkMatch;
use regex::Regex;
pub use replaceable_content::MatchType;
pub use replaceable_content::ReplaceableContent;
pub use text_excluder::InlineCodeExcluder;

use self::back_populate::BackPopulateMatches;
use self::constants::IMAGE_LINK_WHOLE_MATCH_CAPTURE_INDEX;
use self::image_link::ImageLinkTarget;
use self::image_link::ImageLinkType;
use self::image_link::ImageLinks;
use self::text_excluder::CodeBlockExcluder;
use crate::constants::FRONTMATTER_DELIMITER_LINE_COUNT;
use crate::constants::PERSIST_REQUIRES_FRONTMATTER;
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

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Wikilinks {
    pub(crate) valid:   Vec<Wikilink>,
    pub(crate) invalid: Vec<InvalidWikilink>,
}

#[derive(Debug, Clone)]
pub struct MarkdownFile {
    pub(crate) content:                String,
    do_not_back_populate_regexes:      Option<Vec<Regex>>,
    pub(crate) front_matter:           Option<FrontMatter>,
    pub(crate) frontmatter_error:      Option<YamlFrontMatterError>,
    pub(crate) frontmatter_line_count: usize,
    pub(crate) image_links:            ImageLinks,
    pub(crate) wikilinks:              Wikilinks,
    pub(crate) back_populate_matches:  BackPopulateMatches,
    pub(crate) canonical_link_matches: Vec<CanonicalLinkMatch>,
    pub(crate) phantom_link_matches:   Vec<PhantomLinkMatch>,
    pub(crate) path:                   PathBuf,
    pub(crate) persist_reasons:        Vec<PersistReason>,
}

impl MarkdownFile {
    pub(crate) fn new(path: PathBuf) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let full_content = support::read_contents_from_file(&path)?;

        let yaml_result = yaml_frontmatter::find_yaml_section(&full_content);
        let frontmatter_line_count = match &yaml_result {
            Ok(Some((yaml_section, _))) => {
                yaml_section.lines().count() + FRONTMATTER_DELIMITER_LINE_COUNT
            },
            _ => 0,
        };

        let (front_matter, content, frontmatter_error) = match yaml_result {
            Ok(Some((yaml_section, after_yaml))) => {
                match FrontMatter::from_yaml_str(yaml_section) {
                    Ok(front_matter) => (Some(front_matter), after_yaml.to_string(), None),
                    Err(e) => (None, after_yaml.to_string(), Some(e)),
                }
            },
            Ok(None) => (None, full_content, Some(YamlFrontMatterError::Missing)),
            Err(e) => (None, full_content, Some(e)),
        };

        let do_not_back_populate_regexes = front_matter
            .as_ref()
            .and_then(FrontMatter::get_do_not_back_populate_regexes);

        let mut markdown_file = Self {
            content,
            do_not_back_populate_regexes,
            front_matter,
            frontmatter_error,
            frontmatter_line_count,
            wikilinks: Wikilinks::default(),
            image_links: ImageLinks::default(),
            back_populate_matches: BackPopulateMatches::default(),
            canonical_link_matches: Vec::new(),
            phantom_link_matches: Vec::new(),
            path,
            persist_reasons: Vec::new(),
        };

        // MarkdownFile keeps parsed Wikilinks and ImageLinks for later reports.
        markdown_file.wikilinks = markdown_file.process_wikilinks();
        markdown_file.image_links.links = markdown_file.process_image_links();

        Ok(markdown_file)
    }

    pub(crate) fn process_file_for_back_populate_replacements(
        &mut self,
        sorted_wikilinks: &[&Wikilink],
        validated_config: &ValidatedConfig,
        automaton: &AhoCorasick,
    ) {
        self.process_file_for_back_populate_replacements_inner(
            sorted_wikilinks,
            validated_config,
            automaton,
        );
    }

    fn to_full_content(&self) -> String {
        self.front_matter.as_ref().map_or_else(
            || self.content.clone(),
            |front_matter| {
                front_matter.to_yaml_str().map_or_else(
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

    /// Writes the file. The filesystem timestamps are left to the operating system: the
    /// frontmatter dates are the vault's record of when a note was written, and a file's
    /// creation timestamp cannot be restored on every platform anyway.
    pub(crate) fn persist(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        if self.front_matter.is_none() {
            return Err(PERSIST_REQUIRES_FRONTMATTER.into());
        }

        fs::write(&self.path, self.to_full_content())?;

        Ok(())
    }

    pub(crate) fn mark_as_back_populated(
        &mut self,
        operational_timezone: &str,
    ) -> AnyhowResult<()> {
        let front_matter = self
            .front_matter
            .as_mut()
            .ok_or_else(|| anyhow!("{PERSIST_REQUIRES_FRONTMATTER}: {}", self.path.display()))?;
        front_matter.set_date_modified_now(operational_timezone);
        self.persist_reasons.push(PersistReason::BackPopulated);
        Ok(())
    }

    pub(crate) fn mark_image_reference_as_updated(
        &mut self,
        operational_timezone: &str,
    ) -> AnyhowResult<()> {
        let front_matter = self
            .front_matter
            .as_mut()
            .ok_or_else(|| anyhow!("{PERSIST_REQUIRES_FRONTMATTER}: {}", self.path.display()))?;
        front_matter.set_date_modified_now(operational_timezone);
        self.persist_reasons
            .push(PersistReason::ImageReferencesModified);
        Ok(())
    }

    pub(crate) fn mark_links_canonicalized(
        &mut self,
        operational_timezone: &str,
    ) -> AnyhowResult<()> {
        let front_matter = self
            .front_matter
            .as_mut()
            .ok_or_else(|| anyhow!("{PERSIST_REQUIRES_FRONTMATTER}: {}", self.path.display()))?;
        front_matter.set_date_modified_now(operational_timezone);
        self.persist_reasons.push(PersistReason::LinksCanonicalized);
        Ok(())
    }

    pub(crate) fn mark_phantom_links_resolved(
        &mut self,
        operational_timezone: &str,
    ) -> AnyhowResult<()> {
        let front_matter = self
            .front_matter
            .as_mut()
            .ok_or_else(|| anyhow!("{PERSIST_REQUIRES_FRONTMATTER}: {}", self.path.display()))?;
        front_matter.set_date_modified_now(operational_timezone);
        self.persist_reasons
            .push(PersistReason::PhantomLinksResolved);
        Ok(())
    }

    fn process_wikilinks(&self) -> Wikilinks {
        let mut wikilinks = Wikilinks::default();

        let aliases = self
            .front_matter
            .as_ref()
            .and_then(|front_matter| front_matter.aliases().map(<[String]>::to_vec));

        let filename = self
            .path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default();

        let filename_wikilink = wikilink::create_filename_wikilink(filename);
        wikilinks.valid.push(filename_wikilink.clone());

        if let Some(alias_list) = aliases {
            for alias in alias_list {
                let wikilink = Wikilink {
                    display_text: alias.clone(),
                    target:       filename_wikilink.target.clone(),
                };
                wikilinks.valid.push(wikilink);
            }
        }

        let mut code_block_excluder = CodeBlockExcluder::new();

        for (line_idx, line) in self.content.lines().enumerate() {
            code_block_excluder.update(line);
            if code_block_excluder.is_in_code_block() {
                continue;
            }

            let extracted = wikilink::extract_wikilinks(line);
            wikilinks.valid.extend(
                extracted
                    .valid
                    .into_iter()
                    .map(|spanned_wikilink| spanned_wikilink.wikilink),
            );

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
            wikilinks.invalid.extend(invalid_with_lines);
        }

        wikilinks
    }

    fn process_image_links(&self) -> Vec<ImageLink> {
        let mut image_links = Vec::new();

        for (line_idx, line) in self.content.lines().enumerate() {
            for capture in IMAGE_REGEX.captures_iter(line) {
                if let Some(raw_image_link) = capture.get(IMAGE_LINK_WHOLE_MATCH_CAPTURE_INDEX) {
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

    pub(crate) const fn has_ambiguous_matches(&self) -> bool {
        !self.back_populate_matches.ambiguous.is_empty()
    }

    pub(crate) const fn has_unambiguous_matches(&self) -> bool {
        !self.back_populate_matches.unambiguous.is_empty()
    }

    pub(crate) const fn has_canonical_link_matches(&self) -> bool {
        !self.canonical_link_matches.is_empty()
    }

    pub(crate) const fn has_phantom_link_matches(&self) -> bool {
        !self.phantom_link_matches.is_empty()
    }

    /// A file whose frontmatter is absent or unreadable is reported and left alone: the
    /// tool never invents frontmatter, so rewriting the content would either drop the
    /// block the user wrote or produce a note the vault's linter never dated.
    pub(crate) const fn has_usable_frontmatter(&self) -> bool { self.front_matter.is_some() }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::collections::HashSet;
    use std::error::Error;
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;

    use chrono::Utc;
    use tempfile::TempDir;

    use super::MarkdownFile;
    use super::PersistReason;
    use crate::constants::DEFAULT_TIMEZONE;
    use crate::constants::ERROR_NOT_FOUND;
    use crate::constants::FRONTMATTER_DELIMITER_LINE_COUNT;
    use crate::constants::PERSIST_REQUIRES_FRONTMATTER;
    use crate::constants::YAML_CLOSING_DELIMITER_NEWLINE;
    use crate::constants::YAML_OPENING_DELIMITER;
    use crate::frontmatter::FrontMatter;
    use crate::test_support as test_utils;
    use crate::test_support::AliasExpectation;
    use crate::test_support::TestFileBuilder;
    use crate::wikilink::InvalidWikilinkReason;
    use crate::wikilink::Wikilink;

    #[test]
    fn test_parse_content_separation() {
        let temp_dir = TempDir::new().unwrap();

        let file_with_frontmatter = TestFileBuilder::new()
            .with_title("Test".to_string())
            .with_content("This is the actual content")
            .create(&temp_dir, "with_fm.md");

        let markdown_file = test_utils::get_test_markdown_file(file_with_frontmatter);
        assert_eq!(markdown_file.content.trim(), "This is the actual content");

        let file_no_frontmatter = TestFileBuilder::new()
            .with_content("Pure content\nNo frontmatter")
            .create(&temp_dir, "no_fm.md");

        let markdown_file = test_utils::get_test_markdown_file(file_no_frontmatter);
        assert_eq!(markdown_file.content.trim(), "Pure content\nNo frontmatter");

        let delimiter = YAML_OPENING_DELIMITER.trim_end();
        let content = format!("First line\n{delimiter}\nMiddle section\n{delimiter}\nLast section");
        let file_with_separators = TestFileBuilder::new()
            .with_title("Test".to_string())
            .with_content(content.clone())
            .create(&temp_dir, "with_separators.md");

        let markdown_file = test_utils::get_test_markdown_file(file_with_separators);
        assert_eq!(markdown_file.content.trim(), content);
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
            let markdown_file = MarkdownFile::new(file_path).unwrap();
            assert_eq!(
                markdown_file.frontmatter_line_count, *expected_frontmatter_lines,
                "Failed for content:\n{content}"
            );
        }
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

        assert!(markdown_file.persist_reasons.is_empty());

        markdown_file.mark_as_back_populated(DEFAULT_TIMEZONE)?;

        markdown_file.mark_image_reference_as_updated(DEFAULT_TIMEZONE)?;

        assert_eq!(markdown_file.persist_reasons.len(), 2);
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

        // `front_matter` receives the new modified date before `persist`.
        if let Some(front_matter) = &mut markdown_file.front_matter {
            let modified_date = test_utils::eastern_midnight(2024, 1, 2);
            front_matter.set_date_modified(modified_date, DEFAULT_TIMEZONE);
        }

        markdown_file.persist()?;

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

        if let Some(front_matter) = &mut markdown_file.front_matter {
            let modified_date = test_utils::eastern_midnight(2024, 1, 2);
            front_matter.set_date_modified(modified_date, DEFAULT_TIMEZONE);
        }

        markdown_file.persist()?;

        let updated_content = fs::read_to_string(&file_path)?;
        assert!(updated_content.contains("tags:\n- tag1\n- tag2"));
        assert!(updated_content.contains("[[2024-01-02]]"));

        Ok(())
    }

    #[test]
    fn test_disallow_persist_without_frontmatter() {
        let temp_dir = TempDir::new().unwrap();

        let file_path = TestFileBuilder::new()
            .with_content("Body text with no frontmatter block")
            .create(&temp_dir, "no_frontmatter.md");

        let markdown_file = test_utils::get_test_markdown_file(file_path);

        assert!(!markdown_file.has_usable_frontmatter());

        let result = markdown_file.persist();

        assert!(
            result.is_err(),
            "Expected an error, but persist completed successfully"
        );

        if let Err(err) = result {
            assert_eq!(
                err.to_string(),
                PERSIST_REQUIRES_FRONTMATTER,
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

        if let Some(front_matter) = &mut markdown_file.front_matter {
            front_matter.set_date_modified(
                test_utils::parse_datetime("2024-01-04 15:00:00"),
                DEFAULT_TIMEZONE,
            );
        }

        markdown_file.persist()?;

        let updated_content = fs::read_to_string(&file_path)?;
        assert!(updated_content.contains("Sample content for testing"));
        assert!(updated_content.contains("2024-01-01"));
        assert!(updated_content.contains("[[2024-01-04]]"));

        Ok(())
    }

    #[test]
    fn test_unparseable_frontmatter_is_left_alone() {
        let temp_dir = TempDir::new().unwrap();

        let file_path = TestFileBuilder::new()
            .with_custom_frontmatter("tags: [unclosed".to_string())
            .with_content("Some text that mentions a wikilink target")
            .create(&temp_dir, "unparseable_frontmatter.md");

        let mut markdown_file = test_utils::get_test_markdown_file(file_path);

        assert!(markdown_file.front_matter.is_none());
        assert!(!markdown_file.has_usable_frontmatter());

        let result = markdown_file.mark_as_back_populated(DEFAULT_TIMEZONE);

        assert!(result.is_err());
        assert!(markdown_file.front_matter.is_none());
        assert!(markdown_file.persist_reasons.is_empty());
    }

    #[test]
    fn test_markdown_file_with_invalid_wikilinks() {
        let temp_dir = TempDir::new().unwrap();

        let file_path = TestFileBuilder::new()
            .with_content(
                r"# Test File
[[Valid Link]]
[[invalid|link|extra]]
[[unmatched
[[]]"
                    .to_string(),
            )
            .create(&temp_dir, "test.md");

        let markdown_file = MarkdownFile::new(file_path).unwrap();
        let valid_wikilinks = markdown_file.wikilinks.valid;

        // `valid_wikilinks` includes the file name and inline wikilink.
        assert_eq!(valid_wikilinks.len(), 2); // file name and "Valid Link"
        assert!(
            valid_wikilinks
                .iter()
                .any(|w| w.display_text == "Valid Link")
        );

        // `markdown_file.wikilinks.invalid` contains malformed wikilinks.
        assert_eq!(markdown_file.wikilinks.invalid.len(), 3);

        let double_alias = markdown_file
            .wikilinks
            .invalid
            .iter()
            .find(|w| w.reason == InvalidWikilinkReason::DoubleAlias)
            .expect("Should have a double alias invalid wikilink");
        assert_eq!(double_alias.content, "[[invalid|link|extra]]");

        let unmatched = markdown_file
            .wikilinks
            .invalid
            .iter()
            .find(|w| w.reason == InvalidWikilinkReason::UnmatchedOpening)
            .expect("Should have an unmatched opening invalid wikilink");
        assert_eq!(unmatched.content, "[[unmatched");

        let empty = markdown_file
            .wikilinks
            .invalid
            .iter()
            .find(|w| w.reason == InvalidWikilinkReason::Empty)
            .expect("Should have an empty wikilink");
        assert_eq!(empty.content, "[[]]");
    }

    #[test]
    fn test_markdown_file_wikilink_collection() {
        let temp_dir = TempDir::new().unwrap();

        let file_path = TestFileBuilder::new()
            .with_aliases(vec!["Alias One".to_string(), "Second Alias".to_string()])
            .with_content(
                r"# Test Note

Here's a [[Simple Link]] and [[Target Page|Display Text]].
Also linking to [[Alias One]] which is defined in frontmatter."
                    .to_string(),
            )
            .create(&temp_dir, "test_note.md");

        let markdown_file = MarkdownFile::new(file_path).unwrap();
        let wikilinks = markdown_file.wikilinks.valid;

        // Collect unique target-display pairs
        let wikilink_pairs: HashSet<(String, String)> = wikilinks
            .iter()
            .map(|w| (w.target.clone(), w.display_text.clone()))
            .collect();

        assert!(
            wikilink_pairs.contains(&("test_note".to_string(), "test_note".to_string())),
            "Should contain filename-based wikilink"
        );
        assert!(
            wikilink_pairs.contains(&("test_note".to_string(), "Alias One".to_string())),
            "Should contain first alias from frontmatter"
        );
        assert!(
            wikilink_pairs.contains(&("test_note".to_string(), "Second Alias".to_string())),
            "Should contain second alias from frontmatter"
        );
        assert!(
            wikilink_pairs.contains(&("Simple Link".to_string(), "Simple Link".to_string())),
            "Should contain simple wikilink"
        );
        assert!(
            wikilink_pairs.contains(&("Target Page".to_string(), "Display Text".to_string())),
            "Should contain aliased display text"
        );
        assert!(
            wikilink_pairs.contains(&("Alias One".to_string(), "Alias One".to_string())),
            "Should contain content wikilink to Alias One"
        );

        // `extract_wikilinks_from_content` keeps the user-provided `Alias One` target until
        // missing-target validation handles it.
        assert_eq!(
            wikilink_pairs.len(),
            6,
            "Should have collected all unique wikilinks including content reference to Alias One"
        );
    }

    #[test]
    fn test_config_file_not_found() {
        let nonexistent_path = PathBuf::from("nonexistent/config.md");
        let result = MarkdownFile::new(nonexistent_path.clone());

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(&format!(
            "{}{}",
            ERROR_NOT_FOUND,
            nonexistent_path.display()
        )));
    }

    #[test]
    fn test_update_modified_uses_current_date() {
        let temp_dir = TempDir::new().unwrap();
        let base_date = test_utils::eastern_midnight(2024, 1, 15);

        let file_path = TestFileBuilder::new()
            .with_frontmatter_dates(
                Some(test_utils::frontmatter_date_wikilink(base_date)),
                Some(test_utils::frontmatter_date_wikilink(base_date)),
            )
            .create(&temp_dir, "test.md");

        let mut markdown_file = test_utils::get_test_markdown_file(file_path);

        // Use the actual mark_image_reference_as_updated method
        markdown_file
            .mark_image_reference_as_updated(DEFAULT_TIMEZONE)
            .unwrap();

        // `modified_date` stores the updated frontmatter date.
        let modified_date = markdown_file
            .front_matter
            .as_ref()
            .and_then(FrontMatter::date_modified)
            .expect("Should have a modified date");

        // `today` uses the same wikilink format as the frontmatter date.
        let today = test_utils::frontmatter_date_wikilink(Utc::now());

        assert_eq!(
            modified_date, &today,
            "Modified date should be today's date"
        );
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

        let markdown_file = MarkdownFile::new(file_path).unwrap();
        let extracted = markdown_file.process_wikilinks();
        let image_links = markdown_file.process_image_links();

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

        assert!(
            extracted.invalid.is_empty(),
            "Should not have invalid wikilinks"
        );

        assert!(image_links.is_empty(), "Should not have image links");
    }

    #[test]
    fn test_process_content_with_invalid() {
        let content = "Some [[good link]] and [[bad|link|extra]] here\n[[unmatched";

        let temp_dir = TempDir::new().unwrap();
        let file_path = TestFileBuilder::new()
            .with_content(content.to_string())
            .create(&temp_dir, "test.md");

        let markdown_file = MarkdownFile::new(file_path).unwrap();
        let extracted = markdown_file.process_wikilinks();
        let image_links = markdown_file.process_image_links();

        // `extracted.valid` contains non-image wikilinks.
        assert_contains_wikilink(&extracted.valid, "test", None, AliasExpectation::DirectLink);
        assert_contains_wikilink(
            &extracted.valid,
            "good link",
            None,
            AliasExpectation::DirectLink,
        );

        assert_eq!(
            extracted.invalid.len(),
            2,
            "Should have exactly two invalid wikilinks"
        );

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

        let unmatched = extracted
            .invalid
            .iter()
            .find(|w| w.reason == InvalidWikilinkReason::UnmatchedOpening)
            .expect("Should have an unmatched opening invalid wikilink");

        assert_eq!(unmatched.line_number, 2);
        assert_eq!(unmatched.line, "[[unmatched");
        assert_eq!(unmatched.content, "[[unmatched");

        assert!(image_links.is_empty(), "Should not have image links");
    }

    #[test]
    fn test_process_content_with_empty() {
        let content = "Test [[]] here\nAnd [[|]] there";

        let temp_dir = TempDir::new().unwrap();
        let file_path = TestFileBuilder::new()
            .with_content(content.to_string())
            .create(&temp_dir, "test.md");

        let markdown_file = MarkdownFile::new(file_path).unwrap();
        let extracted = markdown_file.process_wikilinks();
        let image_links = markdown_file.process_image_links();

        assert_eq!(
            extracted.invalid.len(),
            2,
            "Should have two invalid empty wikilinks"
        );

        let first_empty = &extracted.invalid[0];
        assert_eq!(first_empty.line_number, 1);
        assert_eq!(first_empty.line, "Test [[]] here");
        assert_eq!(first_empty.content, "[[]]");
        assert_eq!(first_empty.reason, InvalidWikilinkReason::Empty);

        let second_empty = &extracted.invalid[1];
        assert_eq!(second_empty.line_number, 2);
        assert_eq!(second_empty.line, "And [[|]] there");
        assert_eq!(second_empty.content, "[[|]]");
        assert_eq!(second_empty.reason, InvalidWikilinkReason::Empty);

        assert!(image_links.is_empty(), "Should not have image links");
    }

    #[test]
    fn test_process_content_with_images() {
        let content = "# Test\n![[image.png]]\nHere's a [[link]] and ![[another.jpg]]";

        let temp_dir = TempDir::new().unwrap();
        let file_path = TestFileBuilder::new()
            .with_content(content.to_string())
            .create(&temp_dir, "test.md");

        let markdown_file = MarkdownFile::new(file_path).unwrap();
        let extracted = markdown_file.process_wikilinks();
        let image_links = markdown_file.process_image_links();

        // `extracted.valid` contains file-title and inline wikilinks.
        assert_contains_wikilink(&extracted.valid, "test", None, AliasExpectation::DirectLink);
        assert_contains_wikilink(&extracted.valid, "link", None, AliasExpectation::DirectLink);

        // `image_links` contains embedded image wikilinks.
        assert_eq!(image_links.len(), 2, "Should have two image links");

        assert!(image_links.iter().any(|link| link.filename == "image.png"));
        assert!(
            image_links
                .iter()
                .any(|link| link.filename == "another.jpg")
        );
    }
}

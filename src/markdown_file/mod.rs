#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod alias_handling_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod back_populate_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod case_sensitivity_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod date_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod exclusion_zone_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod matching_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod parse_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod persist_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]
mod process_content_tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod table_handling_tests;

use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use aho_corasick::AhoCorasick;
use regex::Regex;

mod back_populate;
mod date_validation;
mod markdown_file_types;
mod match_helpers;
mod text_excluder;

pub use markdown_file_types::BackPopulateMatch;
pub use markdown_file_types::DateValidation;
pub use markdown_file_types::ImageLink;
pub use markdown_file_types::ImageLinkState;
pub use markdown_file_types::MatchContext;
pub use markdown_file_types::MatchType;
pub use markdown_file_types::PersistReason;
pub use markdown_file_types::ReplaceableContent;
pub use text_excluder::InlineCodeExcluder;

use self::markdown_file_types::BackPopulateMatches;
use self::markdown_file_types::DateCreatedFixValidation;
use self::markdown_file_types::ImageLinkTarget;
use self::markdown_file_types::ImageLinkType;
use self::markdown_file_types::ImageLinks;
use self::markdown_file_types::Wikilinks;
use self::text_excluder::CodeBlockExcluder;
use crate::constants::DEFAULT_TIMEZONE;
use crate::frontmatter::FrontMatter;
use crate::utils;
use crate::utils::IMAGE_REGEX;
use crate::validated_config::ValidatedConfig;
use crate::wikilink;
use crate::wikilink::ExtractedWikilinks;
use crate::wikilink::InvalidWikilink;
use crate::wikilink::Wikilink;
use crate::yaml_frontmatter;
use crate::yaml_frontmatter::YamlFrontMatter;
use crate::yaml_frontmatter::YamlFrontMatterError;

#[derive(Debug, Clone)]
pub(crate) struct MarkdownFile {
    pub(crate) content:                      String,
    pub(crate) date_created_fix:             DateCreatedFixValidation,
    pub(crate) date_validation_created:      DateValidation,
    pub(crate) date_validation_modified:     DateValidation,
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

impl MarkdownFile {
    pub(crate) fn new(
        path: PathBuf,
        operational_timezone: &str,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let full_content = utils::read_contents_from_file(&path)?;

        let yaml_result = yaml_frontmatter::find_yaml_section(&full_content);
        let frontmatter_line_count = match &yaml_result {
            Ok(Some((yaml_section, _))) => yaml_section.lines().count() + 2,
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

        let (date_validation_created, date_validation_modified) =
            date_validation::get_date_validations(
                frontmatter.as_ref(),
                &path,
                operational_timezone,
            )?;

        let date_created_fix = DateCreatedFixValidation::from_frontmatter(
            frontmatter.as_ref(),
            date_validation_created.file_system_date,
            operational_timezone,
        );

        let persist_reasons = date_validation::process_date_validations(
            &mut frontmatter,
            &date_validation_created,
            &date_validation_modified,
            &date_created_fix,
            operational_timezone,
        );

        let do_not_back_populate_regexes = frontmatter
            .as_ref()
            .and_then(FrontMatter::get_do_not_back_populate_regexes);

        let mut file_info = Self {
            content,
            date_created_fix,
            do_not_back_populate_regexes,
            date_validation_created,
            date_validation_modified,
            frontmatter,
            frontmatter_error,
            frontmatter_line_count,
            wikilinks: Wikilinks::default(),
            image_links: ImageLinks::default(),
            matches: BackPopulateMatches::default(),
            path,
            persist_reasons,
        };

        let extracted_wikilinks = file_info.process_wikilinks();
        let image_links = file_info.process_image_links();

        // Store results directly in `self`.
        file_info.wikilinks.invalid = extracted_wikilinks.invalid;
        file_info.wikilinks.valid = extracted_wikilinks.valid;
        file_info.image_links.links = image_links;

        Ok(file_info)
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
                    |yaml| format!("---\n{}\n---\n{}", yaml.trim(), self.content.trim()),
                )
            },
        )
    }

    #[allow(
        clippy::expect_used,
        reason = "persist is only called on files with frontmatter"
    )]
    pub(crate) fn persist(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        fs::write(&self.path, self.to_full_content())?;

        let frontmatter = self.frontmatter.as_ref().expect("Frontmatter is required");
        let modified_date = frontmatter
            .raw_date_modified
            .ok_or_else(|| "raw_date_modified must be set for persist".to_string())?;
        let created_date = frontmatter.raw_date_created;

        utils::set_file_dates(&self.path, created_date, modified_date, DEFAULT_TIMEZONE)?;

        Ok(())
    }

    fn ensure_frontmatter(&mut self, operational_timezone: &str) {
        if self.frontmatter.is_none() {
            let mut frontmatter = FrontMatter::default();
            frontmatter.set_date_created(
                self.date_validation_created.file_system_date,
                operational_timezone,
            );
            self.frontmatter = Some(frontmatter);
            self.frontmatter_error = None;
            self.persist_reasons.push(PersistReason::FrontmatterCreated);
        }
    }

    #[allow(
        clippy::expect_used,
        reason = "ensure_frontmatter guarantees frontmatter is present"
    )]
    pub(crate) fn mark_as_back_populated(&mut self, operational_timezone: &str) {
        self.ensure_frontmatter(operational_timezone);

        // Remove any `DateModifiedUpdated` reasons since we'll be setting the date to now.
        // This way we won't show extraneous results in `persist_reasons_report`.
        self.persist_reasons
            .retain(|reason| !matches!(reason, PersistReason::DateModifiedUpdated { .. }));

        self.frontmatter
            .as_mut()
            .expect("ensured above")
            .set_date_modified_now(operational_timezone);
        self.persist_reasons.push(PersistReason::BackPopulated);
    }

    #[allow(
        clippy::expect_used,
        reason = "ensure_frontmatter guarantees frontmatter is present"
    )]
    pub(crate) fn mark_image_reference_as_updated(&mut self, operational_timezone: &str) {
        self.ensure_frontmatter(operational_timezone);

        self.frontmatter
            .as_mut()
            .expect("ensured above")
            .set_date_modified_now(operational_timezone);
        self.persist_reasons
            .push(PersistReason::ImageReferencesModified);
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
                    let image_link = ImageLink::new(
                        raw_image_link.as_str().to_string(),
                        self.get_real_line_number(line_idx),
                        raw_image_link.start(),
                    );
                    match image_link.link_type {
                        ImageLinkType::Wikilink(_)
                        | ImageLinkType::MarkdownLink(ImageLinkTarget::Internal, _) => {
                            image_links.push(image_link);
                        },
                        ImageLinkType::MarkdownLink(..) => {},
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

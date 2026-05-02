#[cfg(test)]
mod tests;

mod back_populate;
mod date_validation;
mod image_link;
mod match_helpers;
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
use crate::frontmatter::FrontMatter;
use crate::utils;
use crate::utils::IMAGE_REGEX;
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
                    |yaml| format!("---\n{}\n---\n{}", yaml.trim(), self.content.trim()),
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

        utils::set_file_dates(&self.path, created_date, modified_date)?;

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

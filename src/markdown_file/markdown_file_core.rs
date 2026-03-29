use std::error::Error;
use std::fs;
use std::path::PathBuf;

use regex::Regex;

use super::date_validation;
use super::markdown_file_types::BackPopulateMatches;
use super::markdown_file_types::DateCreatedFixValidation;
use super::markdown_file_types::DateValidation;
use super::markdown_file_types::ImageLink;
use super::markdown_file_types::ImageLinkTarget;
use super::markdown_file_types::ImageLinkType;
use super::markdown_file_types::ImageLinks;
use super::markdown_file_types::PersistReason;
use super::markdown_file_types::Wikilinks;
use super::text_excluder::CodeBlockExcluder;
use crate::frontmatter::FrontMatter;
use crate::utils;
use crate::utils::IMAGE_REGEX;
use crate::wikilink;
use crate::wikilink::ExtractedWikilinks;
use crate::wikilink::InvalidWikilink;
use crate::wikilink::Wikilink;
use crate::yaml_frontmatter;
use crate::yaml_frontmatter::YamlFrontMatter;
use crate::yaml_frontmatter::YamlFrontMatterError;

#[derive(Debug, Clone)]
pub struct MarkdownFile {
    pub content:                      String,
    pub date_created_fix:             DateCreatedFixValidation,
    pub date_validation_created:      DateValidation,
    pub date_validation_modified:     DateValidation,
    pub do_not_back_populate_regexes: Option<Vec<Regex>>,
    pub frontmatter:                  Option<FrontMatter>,
    pub frontmatter_error:            Option<YamlFrontMatterError>,
    pub frontmatter_line_count:       usize,
    pub image_links:                  ImageLinks,
    pub wikilinks:                    Wikilinks,
    pub matches:                      BackPopulateMatches,
    pub path:                         PathBuf,
    pub persist_reasons:              Vec<PersistReason>,
}

impl MarkdownFile {
    pub fn new(
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
                    Ok(fm) => (Some(fm), after_yaml.to_string(), None),
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
            .and_then(crate::frontmatter::FrontMatter::get_do_not_back_populate_regexes);

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

        // Store results directly in self
        file_info.wikilinks.invalid = extracted_wikilinks.invalid;
        file_info.wikilinks.valid = extracted_wikilinks.valid;
        file_info.image_links.links = image_links;

        Ok(file_info)
    }

    // Add a method to reconstruct the full markdown content
    pub fn to_full_content(&self) -> String {
        self.frontmatter.as_ref().map_or_else(
            || self.content.clone(),
            |fm| {
                fm.to_yaml_str().map_or_else(
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
    pub fn persist(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Write the updated content to the file
        fs::write(&self.path, self.to_full_content())?;

        let frontmatter = self.frontmatter.as_ref().expect("Frontmatter is required");
        let modified_date = frontmatter
            .raw_date_modified
            .ok_or_else(|| "raw_date_modified must be set for persist".to_string())?;

        let created_date = frontmatter.raw_date_created;

        // Use `set_file_dates` for both macOS and non-macOS platforms
        utils::set_file_dates(&self.path, created_date, modified_date, "America/New_York")?;

        Ok(())
    }

    fn ensure_frontmatter(&mut self, operational_timezone: &str) {
        if self.frontmatter.is_none() {
            let mut fm = FrontMatter::default();
            fm.set_date_created(
                self.date_validation_created.file_system_date,
                operational_timezone,
            );
            self.frontmatter = Some(fm);
            self.frontmatter_error = None;
            self.persist_reasons.push(PersistReason::FrontmatterCreated);
        }
    }

    #[allow(
        clippy::expect_used,
        reason = "ensure_frontmatter guarantees frontmatter is present"
    )]
    pub fn mark_as_back_populated(&mut self, operational_timezone: &str) {
        self.ensure_frontmatter(operational_timezone);

        // Remove any `DateModifiedUpdated` reasons since we'll be setting the date to now
        // this way we won't show extraneous results in persist_reasons_report
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
    pub fn mark_image_reference_as_updated(&mut self, operational_timezone: &str) {
        self.ensure_frontmatter(operational_timezone);

        self.frontmatter
            .as_mut()
            .expect("ensured above")
            .set_date_modified_now(operational_timezone);
        self.persist_reasons
            .push(PersistReason::ImageReferencesModified);
    }

    pub(super) fn process_wikilinks(&self) -> ExtractedWikilinks {
        let mut result = ExtractedWikilinks::default();

        let aliases = self
            .frontmatter
            .as_ref()
            .and_then(|fm| fm.aliases().cloned());

        // Add filename-based wikilink
        let filename = self
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        let filename_wikilink = wikilink::create_filename_wikilink(filename);
        result.valid.push(filename_wikilink.clone());

        // Add aliases if present
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

        // Process content line by line for wikilinks
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

    // new only matches image patterns:
    // ![[image.ext]] or ![[image.ext|alt]] -> Embedded Wikilink
    // [[image.ext]] or [[image.ext|alt]] -> Link Only Wikilink
    // ![alt](image.ext) -> Embedded Markdown Internal
    // [alt](image.ext) -> Link Only Markdown Internal
    // ![alt](https://example.com/image.ext) -> Embedded Markdown External
    // [alt](https://example.com/image.ext) -> Link Only Markdown External
    pub(super) fn process_image_links(&self) -> Vec<ImageLink> {
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

    pub const fn get_real_line_number(&self, line_idx: usize) -> usize {
        self.frontmatter_line_count + line_idx + 1
    }

    pub const fn has_ambiguous_matches(&self) -> bool { !self.matches.ambiguous.is_empty() }

    pub const fn has_unambiguous_matches(&self) -> bool { !self.matches.unambiguous.is_empty() }
}

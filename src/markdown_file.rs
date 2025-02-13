#[cfg(test)]
mod alias_handling_tests;
#[cfg(test)]
pub mod back_populate_tests;
#[cfg(test)]
mod case_sensitivity_tests;
#[cfg(test)]
mod date_tests;
#[cfg(test)]
mod exclusion_zone_tests;
#[cfg(test)]
mod matching_tests;
#[cfg(test)]
mod parse_tests;
#[cfg(test)]
mod persist_tests;
#[cfg(test)]
mod process_content_tests;
#[cfg(test)]
mod table_handling_tests;

mod markdown_file_types;
mod text_excluder;

pub use markdown_file_types::*;
pub use text_excluder::{CodeBlockExcluder, InlineCodeExcluder};

use crate::constants::*;
use crate::frontmatter::FrontMatter;
use crate::utils::{IMAGE_REGEX, MARKDOWN_REGEX};
use crate::validated_config::ValidatedConfig;
use crate::wikilink;
use crate::wikilink::{ExtractedWikilinks, InvalidWikilink, ToWikilink, Wikilink};
use crate::yaml_frontmatter;
use crate::yaml_frontmatter::{YamlFrontMatter, YamlFrontMatterError};
use crate::{obsidian_repository, utils};

use aho_corasick::AhoCorasick;
use chrono::{DateTime, NaiveDate, Utc};
use itertools::Itertools;
use regex::Regex;
use std::error::Error;
use std::path::PathBuf;
use std::{fs, io};

#[derive(Debug, Clone)]
pub struct MarkdownFile {
    pub content: String,
    pub date_created_fix: DateCreatedFixValidation,
    pub date_validation_created: DateValidation,
    pub date_validation_modified: DateValidation,
    pub do_not_back_populate_regexes: Option<Vec<Regex>>,
    pub frontmatter: Option<FrontMatter>,
    pub frontmatter_error: Option<YamlFrontMatterError>,
    pub frontmatter_line_count: usize,
    pub image_links: ImageLinks,
    pub wikilinks: Wikilinks,
    pub matches: BackPopulateMatches,
    pub path: PathBuf,
    pub persist_reasons: Vec<PersistReason>,
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
            }
            Ok(None) => (None, full_content, Some(YamlFrontMatterError::Missing)),
            Err(e) => (None, full_content, Some(e)),
        };

        let (date_validation_created, date_validation_modified) =
            get_date_validations(&frontmatter, &path, operational_timezone)?;

        let date_created_fix = DateCreatedFixValidation::from_frontmatter(
            &frontmatter,
            date_validation_created.file_system_date,
            operational_timezone,
        );

        let persist_reasons = process_date_validations(
            &mut frontmatter,
            &date_validation_created,
            &date_validation_modified,
            &date_created_fix,
            operational_timezone,
        );

        let do_not_back_populate_regexes = frontmatter
            .as_ref()
            .and_then(|fm| fm.get_do_not_back_populate_regexes());

        let mut file_info = MarkdownFile {
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

        let extracted_wikilinks = file_info.process_wikilinks()?;
        let image_links = file_info.process_image_links();

        // Store results directly in self
        file_info.wikilinks.invalid = extracted_wikilinks.invalid;
        file_info.wikilinks.valid = extracted_wikilinks.valid;
        file_info.image_links.links = image_links;

        Ok(file_info)
    }

    // Add a method to reconstruct the full markdown content
    pub fn to_full_content(&self) -> String {
        if let Some(ref fm) = self.frontmatter {
            if let Ok(yaml) = fm.to_yaml_str() {
                format!("---\n{}\n---\n{}", yaml.trim(), self.content.trim())
            } else {
                self.content.clone()
            }
        } else {
            self.content.clone()
        }
    }

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

    pub fn mark_as_back_populated(&mut self, operational_timezone: &str) {
        let fm = self.frontmatter.as_mut().unwrap_or_else(|| {
            panic!(
                "Attempted to mark file '{}' as back populated without frontmatter",
                self.path.display()
            )
        });

        // Remove any DateModifiedUpdated reasons since we'll be setting the date to now
        // this way we won't show extraneous results in persist_reasons_report
        self.persist_reasons
            .retain(|reason| !matches!(reason, PersistReason::DateModifiedUpdated { .. }));

        fm.set_date_modified_now(operational_timezone);
        self.persist_reasons.push(PersistReason::BackPopulated);
    }

    pub fn mark_image_reference_as_updated(&mut self, operational_timezone: &str) {
        let fm = self
            .frontmatter
            .as_mut()
            .expect("Attempted to record image references change on a file without frontmatter");

        fm.set_date_modified_now(operational_timezone);
        self.persist_reasons
            .push(PersistReason::ImageReferencesModified);
    }

    pub(crate) fn process_file_for_back_populate_replacements(
        &mut self,
        sorted_wikilinks: &[&Wikilink],
        config: &ValidatedConfig,
        ac: &AhoCorasick,
    ) {
        let content = self.content.clone();
        let mut code_block_tracker = CodeBlockExcluder::new();

        for (line_idx, line) in content.lines().enumerate() {
            // Skip empty/whitespace lines early
            if line.trim().is_empty() {
                continue;
            }

            // Update state and skip if needed
            code_block_tracker.update(line);
            if code_block_tracker.is_in_code_block() {
                continue;
            }

            // Process the line and collect matches
            let matches = self.process_line_for_back_populate_replacements(
                line,
                line_idx,
                ac,
                sorted_wikilinks,
                config,
            );

            // Store matches instead of accumulating for return
            self.matches.unambiguous.extend(matches);
        }
    }

    fn process_wikilinks(&self) -> Result<ExtractedWikilinks, Box<dyn Error + Send + Sync>> {
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
                    target: filename_wikilink.target.clone(),
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

        Ok(result)
    }

    // new only matches image patterns:
    // ![[image.ext]] or ![[image.ext|alt]] -> Embedded Wikilink
    // [[image.ext]] or [[image.ext|alt]] -> Link Only Wikilink
    // ![alt](image.ext) -> Embedded Markdown Internal
    // [alt](image.ext) -> Link Only Markdown Internal
    // ![alt](https://example.com/image.ext) -> Embedded Markdown External
    // [alt](https://example.com/image.ext) -> Link Only Markdown External
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
                    match image_link.image_link_type {
                        ImageLinkType::Wikilink(_)
                        | ImageLinkType::MarkdownLink(ImageLinkTarget::Internal, _) => {
                            image_links.push(image_link)
                        }
                        _ => {}
                    }
                }
            }
        }

        image_links
    }

    fn process_line_for_back_populate_replacements(
        &self,
        line: &str,
        line_idx: usize,
        ac: &AhoCorasick,
        sorted_wikilinks: &[&Wikilink],
        config: &ValidatedConfig,
    ) -> Vec<BackPopulateMatch> {
        let mut matches = Vec::new();
        let exclusion_zones = self.collect_exclusion_zones(line, config);

        // Collect all valid matches
        for mat in ac.find_iter(line) {
            let wikilink = sorted_wikilinks[mat.pattern()];
            let starts_at = mat.start();
            let ends_at = mat.end();

            if range_overlaps(&exclusion_zones, starts_at, ends_at) {
                continue;
            }

            let matched_text = &line[starts_at..ends_at];
            if !is_word_boundary(line, starts_at, ends_at) {
                continue;
            }

            if self.should_create_match(line, starts_at, matched_text) {
                let mut replacement = if matched_text == wikilink.target {
                    wikilink.target.to_wikilink()
                } else {
                    wikilink.target.to_aliased_wikilink(matched_text)
                };

                let in_markdown_table = is_in_markdown_table(line, matched_text);
                if in_markdown_table {
                    replacement = replacement.replace('|', r"\|");
                }

                let relative_path =
                    obsidian_repository::format_relative_path(&self.path, config.obsidian_path());

                matches.push(BackPopulateMatch {
                    found_text: matched_text.to_string(),
                    line_number: self.get_real_line_number(line_idx),
                    line_text: line.to_string(),
                    position: starts_at,
                    in_markdown_table,
                    relative_path,
                    replacement,
                });
            }
        }

        matches
    }

    pub fn get_real_line_number(&self, line_idx: usize) -> usize {
        self.frontmatter_line_count + line_idx + 1
    }

    fn collect_exclusion_zones(&self, line: &str, config: &ValidatedConfig) -> Vec<(usize, usize)> {
        let mut exclusion_zones = Vec::new();

        // Add invalid wikilinks as exclusion zones
        for invalid_wikilink in &self.wikilinks.invalid {
            // Only add exclusion zone if this invalid wikilink is on the current line
            if invalid_wikilink.line == line {
                exclusion_zones.push(invalid_wikilink.span);
            }
        }

        let regex_sources = [
            config.do_not_back_populate_regexes(),
            self.do_not_back_populate_regexes.as_deref(),
        ];

        // Flatten the iterator to get a single iterator over regexes
        for do_not_back_populate_regexes in regex_sources.iter().flatten() {
            for regex in *do_not_back_populate_regexes {
                for mat in regex.find_iter(line) {
                    exclusion_zones.push((mat.start(), mat.end()));
                }
            }
        }

        // Add Markdown links as exclusion zones
        for mat in MARKDOWN_REGEX.find_iter(line) {
            exclusion_zones.push((mat.start(), mat.end()));
        }

        // they need to be ordered!
        exclusion_zones.sort_by_key(|&(start, _)| start);
        exclusion_zones
    }

    fn should_create_match(&self, line: &str, absolute_start: usize, matched_text: &str) -> bool {
        // Check if this is the text's own page or matches any frontmatter aliases
        if let Some(stem) = self.path.file_stem().and_then(|s| s.to_str()) {
            if stem.eq_ignore_ascii_case(matched_text) {
                return false;
            }

            // Check against frontmatter aliases
            if let Some(frontmatter) = &self.frontmatter {
                if let Some(aliases) = frontmatter.aliases() {
                    if aliases
                        .iter()
                        .any(|alias| alias.eq_ignore_ascii_case(matched_text))
                    {
                        return false;
                    }
                }
            }
        }

        !wikilink::is_within_wikilink(line, absolute_start)
    }

    pub fn has_ambiguous_matches(&self) -> bool {
        !self.matches.ambiguous.is_empty()
    }

    pub fn has_unambiguous_matches(&self) -> bool {
        !self.matches.unambiguous.is_empty()
    }
}

fn get_date_validations(
    frontmatter: &Option<FrontMatter>,
    path: &PathBuf,
    operational_timezone: &str,
) -> Result<(DateValidation, DateValidation), io::Error> {
    let metadata = fs::metadata(path)?;

    let dates = [
        (
            frontmatter
                .as_ref()
                .and_then(|fm| fm.date_created().cloned()),
            metadata
                .created()
                .map(|t| t.into())
                .unwrap_or_else(|_| Utc::now()),
        ),
        (
            frontmatter
                .as_ref()
                .and_then(|fm| fm.date_modified().cloned()),
            metadata
                .modified()
                .map(|t| t.into())
                .unwrap_or_else(|_| Utc::now()),
        ),
    ];

    // skip when the create date has a date_created_fix in place, we don't need to validate as it's moot
    Ok(dates
        .into_iter()
        .map(|(frontmatter_date, fs_date)| {
            let issue = get_date_validation_issue(
                frontmatter_date.as_ref(),
                &fs_date,
                operational_timezone,
            );
            DateValidation {
                frontmatter_date,
                file_system_date: fs_date,
                issue,
                operational_timezone: operational_timezone.to_string(),
            }
        })
        .collect_tuple()
        .unwrap())
}

fn get_date_validation_issue(
    date_opt: Option<&String>,
    fs_date: &DateTime<Utc>,
    operational_timezone: &str,
) -> Option<DateValidationIssue> {
    // Check if the date is missing
    let date_str = match date_opt {
        Some(s) => s,
        None => return Some(DateValidationIssue::Missing),
    };

    // Check if the date string is a valid wikilink
    if !wikilink::is_wikilink(Some(date_str)) {
        return Some(DateValidationIssue::InvalidWikilink);
    }

    let extracted_date = extract_date(date_str);

    // Validate the extracted date format
    if !is_valid_date(extracted_date) {
        return Some(DateValidationIssue::InvalidDateFormat);
    }

    // Parse the frontmatter date string into a NaiveDate
    let frontmatter_date = match NaiveDate::parse_from_str(extracted_date.trim(), "%Y-%m-%d") {
        Ok(date) => date,
        Err(_) => return Some(DateValidationIssue::InvalidDateFormat),
    };

    // Parse timezone string into a Tz
    let tz = match operational_timezone.parse::<chrono_tz::Tz>() {
        Ok(tz) => tz,
        Err(_) => return Some(DateValidationIssue::InvalidDateFormat),
    };

    // Convert UTC fs_date to the specified timezone
    let fs_date_local = fs_date.with_timezone(&tz);
    let fs_date_naive = fs_date_local.date_naive();

    // Compare the dates
    if frontmatter_date != fs_date_naive {
        return Some(DateValidationIssue::FileSystemMismatch);
    }

    // All validations passed
    None
}

// Extracts the date string from a possible wikilink format
fn extract_date(date_str: &str) -> &str {
    let date_str = date_str.trim();
    if wikilink::is_wikilink(Some(date_str)) {
        date_str
            .trim_start_matches(OPENING_WIKILINK)
            .trim_end_matches(CLOSING_WIKILINK)
            .trim()
    } else {
        date_str
    }
}

// Validates if a string is a valid YYYY-MM-DD date
fn is_valid_date(date_str: &str) -> bool {
    NaiveDate::parse_from_str(date_str.trim(), "%Y-%m-%d").is_ok()
}

fn process_date_validations(
    frontmatter: &mut Option<FrontMatter>,
    created_validation: &DateValidation,
    modified_validation: &DateValidation,
    date_created_fix_validation: &DateCreatedFixValidation,
    operational_timezone: &str,
) -> Vec<PersistReason> {
    let mut reasons = Vec::new();

    if let Some(ref mut fm) = frontmatter {
        let mut skip_date_created = false;

        if let Some(fix_date) = date_created_fix_validation.fix_date {
            skip_date_created = true;

            fm.set_date_created(fix_date, operational_timezone);
            fm.remove_date_created_fix();
            reasons.push(PersistReason::DateCreatedFixApplied);
        }

        // Update created date if there's an issue
        if let Some(ref issue) = created_validation.issue {
            if !skip_date_created {
                fm.set_date_created(created_validation.file_system_date, operational_timezone);
                reasons.push(PersistReason::DateCreatedUpdated {
                    reason: issue.clone(),
                });
            }
        }

        // Update modified date if there's an issue
        if let Some(ref issue) = modified_validation.issue {
            fm.set_date_modified(modified_validation.file_system_date, operational_timezone);
            reasons.push(PersistReason::DateModifiedUpdated {
                reason: issue.clone(),
            });
        }
    }

    reasons
}

fn is_word_boundary(line: &str, starts_at: usize, ends_at: usize) -> bool {
    // Helper to check if a char is a word character (\w in regex)
    fn is_word_char(ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_'
    }

    // Helper to check if string matches a contraction pattern ending in apostrophe t or T
    fn is_t_contraction(chars: &str) -> bool {
        let mut chars = chars.chars();
        match (chars.next(), chars.next()) {
            // Check for "'t" or "'t" (curly apostrophe)
            (Some('\''), Some('t') | Some('T')) | (Some('\u{2019}'), Some('t') | Some('T')) => true,
            _ => false,
        }
    }

    // Get chars before and after safely
    let before = line[..starts_at].chars().last();
    let after_chars = &line[ends_at..];

    // Check start boundary
    let start_is_boundary = starts_at == 0 || before.map_or(true, |ch| !is_word_char(ch));

    // Check end boundary
    // No need to check for possessives as they should be valid candidates for replacement
    let end_is_boundary = ends_at == line.len()
        || (!is_word_char(after_chars.chars().next().unwrap_or(' '))
            && !is_t_contraction(after_chars));

    start_is_boundary && end_is_boundary
}

fn range_overlaps(ranges: &[(usize, usize)], start: usize, end: usize) -> bool {
    ranges.iter().any(|&(r_start, r_end)| {
        (start >= r_start && start < r_end)
            || (end > r_start && end <= r_end)
            || (start <= r_start && end >= r_end)
    })
}

fn is_in_markdown_table(line: &str, matched_text: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|')
        && trimmed.ends_with('|')
        && trimmed.matches('|').count() > 2
        && trimmed.contains(matched_text)
}

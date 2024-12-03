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
mod table_handling_tests;

mod markdown_file_info_types;
pub use markdown_file_info_types::*;

use crate::frontmatter::FrontMatter;
use crate::utils::{read_contents_from_file, MARKDOWN_REGEX};
use crate::wikilink::is_wikilink;
use crate::wikilink::{InvalidWikilink, ToWikilink, Wikilink};
use crate::yaml_frontmatter::{find_yaml_section, YamlFrontMatter, YamlFrontMatterError};
use crate::{CLOSING_WIKILINK, OPENING_WIKILINK};

use crate::validated_config::ValidatedConfig;
use aho_corasick::AhoCorasick;
use chrono::{DateTime, NaiveDate, Utc};
use filetime::FileTime;
use itertools::Itertools;
use lazy_static::lazy_static;
use regex::Regex;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::{fs, io};

#[derive(Debug)]
pub struct MarkdownFileInfo {
    pub content: String,
    pub date_created_fix: DateCreatedFixValidation,
    pub date_validation_created: DateValidation,
    pub date_validation_modified: DateValidation,
    pub do_not_back_populate_regexes: Option<Vec<Regex>>,
    pub frontmatter: Option<FrontMatter>,
    pub frontmatter_error: Option<YamlFrontMatterError>,
    pub frontmatter_line_count: usize,
    pub image_links: Vec<ImageLink>,
    pub invalid_wikilinks: Vec<InvalidWikilink>,
    pub matches: BackPopulateMatches,
    pub path: PathBuf,
    pub persist_reasons: Vec<PersistReason>,
}

impl MarkdownFileInfo {
    pub fn new(
        path: PathBuf,
        operational_timezone: &str,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let full_content = read_contents_from_file(&path)?;

        let yaml_result = find_yaml_section(&full_content);
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

        let mut persist_reasons = process_date_validations(
            &mut frontmatter,
            &date_validation_created,
            &date_validation_modified,
        );

        let date_created_fix = DateCreatedFixValidation::from_frontmatter(
            &frontmatter,
            date_validation_created.file_system_date,
        );

        if let Some(ref mut fm) = frontmatter {
            if let Some(fix_date) = date_created_fix.fix_date {
                fm.set_date_created(fix_date);
                fm.remove_date_created_fix();
                persist_reasons.push(PersistReason::DateCreatedFixApplied);
            }
        }

        let do_not_back_populate_regexes = frontmatter
            .as_ref()
            .and_then(|fm| fm.get_do_not_back_populate_regexes());

        Ok(MarkdownFileInfo {
            content,
            date_created_fix,
            do_not_back_populate_regexes,
            date_validation_created,
            date_validation_modified,
            frontmatter,
            frontmatter_error,
            frontmatter_line_count,
            invalid_wikilinks: Vec::new(),
            image_links: Vec::new(),
            matches: BackPopulateMatches::default(),
            path,
            persist_reasons,
        })
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

        if let Some(created_date) = frontmatter.raw_date_created {
            filetime::set_file_times(
                &self.path,
                FileTime::from_system_time(created_date.into()),
                FileTime::from_system_time(modified_date.into()),
            )?;
        } else {
            filetime::set_file_mtime(&self.path, FileTime::from_system_time(modified_date.into()))?;
        }

        Ok(())
    }

    // Helper method to add invalid wikilinks
    pub fn add_invalid_wikilinks(&mut self, wikilinks: Vec<InvalidWikilink>) {
        self.invalid_wikilinks.extend(wikilinks);
    }

    pub fn mark_as_back_populated(&mut self) {
        let fm = self
            .frontmatter
            .as_mut()
            .expect("Attempted to mark file as back populated without frontmatter");
        fm.set_date_modified_now();
        self.persist_reasons.push(PersistReason::BackPopulated);
    }

    pub fn record_image_references_change(&mut self) {
        let fm = self
            .frontmatter
            .as_mut()
            .expect("Attempted to record image references change on a file without frontmatter");
        fm.set_date_modified_now();
        self.persist_reasons
            .push(PersistReason::ImageReferencesModified);
    }

    pub(crate) fn process_file(
        &mut self,
        sorted_wikilinks: &[&Wikilink],
        config: &ValidatedConfig,
        ac: &AhoCorasick,
    ) {
        let content = self.content.clone();
        let mut state = FileProcessingState::new();

        for (line_idx, line) in content.lines().enumerate() {
            // Skip empty/whitespace lines early
            if line.trim().is_empty() {
                continue;
            }

            // Update state and skip if needed
            state.update_for_line(line);
            if state.should_skip_line() {
                continue;
            }

            // Process the line and collect matches
            let matches = self.process_line(line, line_idx, ac, sorted_wikilinks, config);

            // Store matches instead of accumulating for return
            self.matches.unambiguous.extend(matches);
        }
    }

    fn process_line(
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

                let relative_path = format_relative_path(&self.path, config.obsidian_path());

                matches.push(BackPopulateMatch {
                    found_text: matched_text.to_string(),
                    frontmatter_line_count: self.frontmatter_line_count,
                    line_number: line_idx + 1,
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

    fn collect_exclusion_zones(&self, line: &str, config: &ValidatedConfig) -> Vec<(usize, usize)> {
        let mut exclusion_zones = Vec::new();

        // Add invalid wikilinks as exclusion zones
        for invalid_wikilink in &self.invalid_wikilinks {
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
        for regexes in regex_sources.iter().flatten() {
            for regex in *regexes {
                for mat in regex.find_iter(line) {
                    exclusion_zones.push((mat.start(), mat.end()));
                }
            }
        }

        // todo - we should use the ImageLinks once we add line number and start/end position
        //        to ImageLink struct then we can simply exclude everything without a redundant
        //        regex search right here
        // Add Markdown links as exclusion zones
        for mat in MARKDOWN_REGEX.find_iter(line) {
            exclusion_zones.push((mat.start(), mat.end()));
        }

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

        !is_within_wikilink(line, absolute_start)
    }
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
    if !is_wikilink(Some(date_str)) {
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
    let fs_date_ymd = fs_date_local.format("%Y-%m-%d").to_string();

    // Compare the dates
    if frontmatter_date.format("%Y-%m-%d").to_string() != fs_date_ymd {
        return Some(DateValidationIssue::FileSystemMismatch);
    }

    // All validations passed
    None
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

// Extracts the date string from a possible wikilink format
fn extract_date(date_str: &str) -> &str {
    let date_str = date_str.trim();
    if is_wikilink(Some(date_str)) {
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
) -> Vec<PersistReason> {
    let mut reasons = Vec::new();

    if let Some(ref mut fm) = frontmatter {
        // Update created date if there's an issue
        if let Some(ref issue) = created_validation.issue {
            fm.set_date_created(created_validation.file_system_date);
            reasons.push(PersistReason::DateCreatedUpdated {
                reason: issue.clone(),
            });
        }

        // Update modified date if there's an issue
        if let Some(ref issue) = modified_validation.issue {
            fm.set_date_modified(modified_validation.file_system_date);
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

fn is_within_wikilink(line: &str, byte_position: usize) -> bool {
    lazy_static! {
        static ref WIKILINK_FINDER: regex::Regex = regex::Regex::new(r"\[\[.*?\]\]").unwrap();
    }

    for mat in WIKILINK_FINDER.find_iter(line) {
        let content_start = mat.start() + 2; // Start of link content, after "[["
        let content_end = mat.end() - 2; // End of link content, before "\]\]"

        // Return true only if the byte_position falls within the link content
        if byte_position >= content_start && byte_position < content_end {
            return true;
        }
    }
    false
}

fn format_relative_path(path: &Path, base_path: &Path) -> String {
    path.strip_prefix(base_path)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn is_in_markdown_table(line: &str, matched_text: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|')
        && trimmed.ends_with('|')
        && trimmed.matches('|').count() > 2
        && trimmed.contains(matched_text)
}

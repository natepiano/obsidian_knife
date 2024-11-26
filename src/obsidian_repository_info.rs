#[cfg(test)]
mod alias_handling_tests;
#[cfg(test)]
mod ambiguous_matches_tests;
#[cfg(test)]
mod back_populate_tests;
#[cfg(test)]
mod case_sensitivity_tests;
#[cfg(test)]
mod exclusion_zone_tests;
#[cfg(test)]
mod file_process_limit_tests;
#[cfg(test)]
mod matching_tests;
#[cfg(test)]
mod persist_file_tests;
#[cfg(test)]
mod table_handling_tests;
#[cfg(test)]
mod update_modified_tests;

use crate::markdown_file_info::{BackPopulateMatch, MarkdownFileInfo};
use crate::markdown_files::MarkdownFiles;
use crate::scan::ImageInfo;
use crate::utils::{
    escape_brackets, escape_pipe, ColumnAlignment, ThreadSafeWriter, MARKDOWN_REGEX,
};
use crate::validated_config::ValidatedConfig;
use crate::wikilink_types::{InvalidWikilinkReason, ToWikilink, Wikilink};
use crate::LEVEL2;
use aho_corasick::AhoCorasick;
use itertools::Itertools;
use lazy_static::lazy_static;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct ObsidianRepositoryInfo {
    pub markdown_files: MarkdownFiles,
    pub image_map: HashMap<PathBuf, ImageInfo>,
    pub other_files: Vec<PathBuf>,
    pub wikilinks_ac: Option<AhoCorasick>,
    pub wikilinks_sorted: Vec<Wikilink>,
}

impl ObsidianRepositoryInfo {
    pub fn find_all_back_populate_matches(
        &mut self,
        config: &ValidatedConfig,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let ac = self
            .wikilinks_ac
            .as_ref()
            .expect("Wikilinks AC pattern should be initialized");
        let sorted_wikilinks: Vec<&Wikilink> = self.wikilinks_sorted.iter().collect();

        self.markdown_files
            .par_iter_mut()
            .for_each(|markdown_file_info| {
                if !cfg!(test) {
                    if let Some(filter) = config.back_populate_file_filter() {
                        if !markdown_file_info.path.ends_with(filter) {
                            return;
                        }
                    }
                }

                // todo - do you need to handle it with let _? is there a better way
                let _ = process_file(&sorted_wikilinks, config, markdown_file_info, ac);
            });

        Ok(())
    }

    pub fn persist(
        &mut self,
        config: &ValidatedConfig,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.markdown_files.persist_all(config.file_process_limit())
    }

    pub fn update_modified_dates_for_cleanup_images(&mut self, paths: &[PathBuf]) {
        self.markdown_files
            .update_modified_dates_for_cleanup_images(paths);
    }

    pub fn write_invalid_wikilinks_table(
        &self,
        writer: &ThreadSafeWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Collect all invalid wikilinks from all files
        let invalid_wikilinks = self
            .markdown_files
            .iter()
            .flat_map(|markdown_file_info| {
                markdown_file_info
                    .invalid_wikilinks
                    .iter()
                    .filter(|wikilink| {
                        !matches!(
                            wikilink.reason,
                            InvalidWikilinkReason::EmailAddress | InvalidWikilinkReason::Tag
                        )
                    })
                    .map(move |wikilink| (&markdown_file_info.path, wikilink))
            })
            .collect::<Vec<_>>()
            .into_iter()
            .sorted_by(|a, b| {
                let file_a = a.0.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let file_b = b.0.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                file_a
                    .cmp(file_b)
                    .then(a.1.line_number.cmp(&b.1.line_number))
            })
            .collect::<Vec<_>>();

        if invalid_wikilinks.is_empty() {
            return Ok(());
        }

        writer.writeln(LEVEL2, "invalid wikilinks")?;

        // Write header describing the count
        writer.writeln(
            "",
            &format!(
                "found {} invalid wikilinks in {} files\n",
                invalid_wikilinks.len(),
                invalid_wikilinks
                    .iter()
                    .map(|(p, _)| p)
                    .collect::<HashSet<_>>()
                    .len()
            ),
        )?;

        // Prepare headers and alignments for the table
        let headers = vec![
            "file name",
            "line",
            "line text",
            "invalid reason",
            "source text",
        ];

        let alignments = vec![
            ColumnAlignment::Left,
            ColumnAlignment::Right,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ];

        // Prepare rows
        let rows: Vec<Vec<String>> = invalid_wikilinks
            .iter()
            .map(|(file_path, invalid_wikilink)| {
                vec![
                    file_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_wikilink(),
                    invalid_wikilink.line_number.to_string(),
                    escape_pipe(&invalid_wikilink.line),
                    invalid_wikilink.reason.to_string(),
                    escape_brackets(&invalid_wikilink.content),
                ]
            })
            .collect();

        // Write the table
        writer.write_markdown_table(&headers, &rows, Some(&alignments))?;
        writer.writeln("", "\n---\n")?;

        Ok(())
    }
}

fn process_file(
    sorted_wikilinks: &[&Wikilink],
    config: &ValidatedConfig,
    markdown_file_info: &mut MarkdownFileInfo,
    ac: &AhoCorasick,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let content = markdown_file_info.content.clone();
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
        let matches = process_line(
            line,
            line_idx,
            ac,
            sorted_wikilinks,
            config,
            markdown_file_info,
        )?;

        // Store matches instead of accumulating for return
        markdown_file_info.matches.extend(matches);
    }

    Ok(())
}

fn process_line(
    line: &str,
    line_idx: usize,
    ac: &AhoCorasick,
    sorted_wikilinks: &[&Wikilink],
    config: &ValidatedConfig,
    markdown_file_info: &MarkdownFileInfo,
) -> Result<Vec<BackPopulateMatch>, Box<dyn Error + Send + Sync>> {
    let mut matches = Vec::new();
    let exclusion_zones = collect_exclusion_zones(line, config, markdown_file_info);

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

        if should_create_match(
            line,
            starts_at,
            matched_text,
            &markdown_file_info.path,
            markdown_file_info,
        ) {
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
                format_relative_path(&markdown_file_info.path, config.obsidian_path());

            matches.push(BackPopulateMatch {
                found_text: matched_text.to_string(),
                frontmatter_line_count: markdown_file_info.frontmatter_line_count,
                line_number: line_idx + 1,
                line_text: line.to_string(),
                position: starts_at,
                in_markdown_table,
                relative_path,
                replacement,
            });
        }
    }

    Ok(matches)
}

fn format_relative_path(path: &Path, base_path: &Path) -> String {
    path.strip_prefix(base_path)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn should_create_match(
    line: &str,
    absolute_start: usize,
    matched_text: &str,
    file_path: &Path,
    markdown_file_info: &MarkdownFileInfo,
) -> bool {
    // Check if this is the text's own page or matches any frontmatter aliases
    if let Some(stem) = file_path.file_stem().and_then(|s| s.to_str()) {
        if stem.eq_ignore_ascii_case(matched_text) {
            return false;
        }

        // Check against frontmatter aliases
        if let Some(frontmatter) = &markdown_file_info.frontmatter {
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

fn is_in_markdown_table(line: &str, matched_text: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|')
        && trimmed.ends_with('|')
        && trimmed.matches('|').count() > 2
        && trimmed.contains(matched_text)
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

fn collect_exclusion_zones(
    line: &str,
    config: &ValidatedConfig,
    markdown_file_info: &MarkdownFileInfo,
) -> Vec<(usize, usize)> {
    let mut exclusion_zones = Vec::new();

    // Add invalid wikilinks as exclusion zones
    for invalid_wikilink in &markdown_file_info.invalid_wikilinks {
        // Only add exclusion zone if this invalid wikilink is on the current line
        if invalid_wikilink.line == line {
            exclusion_zones.push(invalid_wikilink.span);
        }
    }

    let regex_sources = [
        config.do_not_back_populate_regexes(),
        markdown_file_info.do_not_back_populate_regexes.as_deref(),
    ];

    // Flatten the iterator to get a single iterator over regexes
    for regexes in regex_sources.iter().flatten() {
        for regex in *regexes {
            for mat in regex.find_iter(line) {
                exclusion_zones.push((mat.start(), mat.end()));
            }
        }
    }

    // Add Markdown links as exclusion zones
    for mat in MARKDOWN_REGEX.find_iter(line) {
        exclusion_zones.push((mat.start(), mat.end()));
    }

    exclusion_zones.sort_by_key(|&(start, _)| start);
    exclusion_zones
}

#[derive(Debug)]
struct FileProcessingState {
    in_frontmatter: bool,
    in_code_block: bool,
    frontmatter_delimiter_count: usize,
}

impl FileProcessingState {
    fn new() -> Self {
        Self {
            in_frontmatter: false,
            in_code_block: false,
            frontmatter_delimiter_count: 0,
        }
    }

    fn update_for_line(&mut self, line: &str) -> bool {
        let trimmed = line.trim();

        // Check frontmatter delimiter
        if trimmed == "---" {
            self.frontmatter_delimiter_count += 1;
            self.in_frontmatter = self.frontmatter_delimiter_count % 2 != 0;
            return true;
        }

        // Check code block delimiter if not in frontmatter
        if !self.in_frontmatter && trimmed.starts_with("```") {
            self.in_code_block = !self.in_code_block;
            return true;
        }

        // Return true if we should skip this line
        self.in_frontmatter || self.in_code_block
    }

    fn should_skip_line(&self) -> bool {
        self.in_frontmatter || self.in_code_block
    }
}

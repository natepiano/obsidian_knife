#[cfg(test)]
mod alias_handling_tests;
#[cfg(test)]
pub mod back_populate_tests;
#[cfg(test)]
mod case_sensitivity_tests;
#[cfg(test)]
mod exclusion_zone_tests;
#[cfg(test)]
mod matching_tests;
#[cfg(test)]
mod table_handling_tests;

use crate::markdown_file_info::{BackPopulateMatch, MarkdownFileInfo, PersistReason};
use crate::utils::{ColumnAlignment, ThreadSafeWriter, MARKDOWN_REGEX};
use crate::validated_config::ValidatedConfig;
use crate::wikilink::format_wikilink;
use crate::wikilink_types::{ToWikilink, Wikilink};
use crate::{LEVEL1, LEVEL3};
use lazy_static::lazy_static;

use aho_corasick::AhoCorasick;
use rayon::prelude::*;
use std::collections::HashSet;
use std::error::Error;
use std::io;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct MarkdownFiles {
    files: Vec<MarkdownFileInfo>, // Changed from Arc<Mutex<>>
}

impl Deref for MarkdownFiles {
    type Target = Vec<MarkdownFileInfo>;

    fn deref(&self) -> &Self::Target {
        &self.files
    }
}

impl DerefMut for MarkdownFiles {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.files
    }
}

// Add these implementations after the MarkdownFiles struct definition
impl Index<usize> for MarkdownFiles {
    type Output = MarkdownFileInfo;

    fn index(&self, index: usize) -> &Self::Output {
        &self.files[index]
    }
}

impl IndexMut<usize> for MarkdownFiles {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.files[index]
    }
}

impl MarkdownFiles {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    pub fn push(&mut self, file: MarkdownFileInfo) {
        // Note: now takes &mut self
        self.files.push(file);
    }

    pub fn iter(&self) -> impl Iterator<Item = &MarkdownFileInfo> {
        self.files.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut MarkdownFileInfo> {
        self.files.iter_mut()
    }

    pub fn par_iter(&self) -> impl ParallelIterator<Item = &MarkdownFileInfo> {
        self.files.par_iter()
    }

    pub fn process_files(
        &mut self,
        config: &ValidatedConfig,
        sorted_wikilinks: Vec<&Wikilink>,
        ac: &AhoCorasick,
    ) {
        self.par_iter_mut().for_each(|markdown_file_info| {
            if !cfg!(test) {
                if let Some(filter) = config.back_populate_file_filter() {
                    if !markdown_file_info.path.ends_with(filter) {
                        return;
                    }
                }
            }

            process_file(&sorted_wikilinks, config, markdown_file_info, ac);
        });
    }

    pub fn unambiguous_matches(&self) -> Vec<BackPopulateMatch> {
        self.iter()
            .flat_map(|file| file.matches.unambiguous.clone())
            .collect()
    }

    pub fn persist_all(
        &self,
        file_limit: Option<usize>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let files_to_persist: Vec<_> = self
            .files
            .iter()
            .filter(|file_info| {
                file_info
                    .frontmatter
                    .as_ref()
                    .map_or(false, |fm| fm.needs_persist())
            })
            .collect();

        let total_files = files_to_persist.len();
        let iter = files_to_persist.iter();
        let files_to_process = match file_limit {
            Some(limit) => iter.take(limit),
            None => iter.take(total_files), // Match the Take type from Some branch
        };

        for file_info in files_to_process {
            file_info.persist()?;
        }
        Ok(())
    }

    pub fn update_modified_dates_for_cleanup_images(&mut self, paths: &[PathBuf]) {
        let paths_set: HashSet<_> = paths.iter().collect();

        self.files
            .iter_mut()
            .filter(|file_info| paths_set.contains(&file_info.path))
            .for_each(|file_info| {
                file_info.record_image_references_change();
            });
    }

    pub fn report_frontmatter_issues(
        &self,
        writer: &ThreadSafeWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let files_with_errors: Vec<_> = self
            .files
            .iter()
            .filter_map(|info| info.frontmatter_error.as_ref().map(|err| (&info.path, err)))
            .collect();

        writer.writeln(LEVEL1, "frontmatter")?;

        if files_with_errors.is_empty() {
            return Ok(());
        }

        writer.writeln(
            "",
            &format!(
                "found {} files with frontmatter parsing errors",
                files_with_errors.len()
            ),
        )?;

        for (path, err) in files_with_errors {
            writer.writeln(LEVEL3, &format!("in file {}", format_wikilink(path)))?;
            writer.writeln("", &format!("{}", err))?;
            writer.writeln("", "")?;
        }

        Ok(())
    }

    pub fn write_persist_reasons_table(&self, writer: &ThreadSafeWriter) -> io::Result<()> {
        let mut rows: Vec<Vec<String>> = Vec::new();

        for file in &self.files {
            if !file.persist_reasons.is_empty() {
                let file_name = file
                    .path
                    .file_name()
                    .and_then(|f| f.to_str())
                    .map(|s| s.trim_end_matches(".md"))
                    .unwrap_or_default();

                let wikilink = format!("[[{}]]", file_name);

                // Count instances of BackPopulated and ImageReferencesModified
                let back_populate_count = file.matches.unambiguous.len();

                let image_refs_count = file
                    .persist_reasons
                    .iter()
                    .filter(|&r| matches!(r, PersistReason::ImageReferencesModified))
                    .count();

                // Generate rows for each persist reason
                for reason in &file.persist_reasons {
                    let (before, after, reason_info) = match reason {
                        PersistReason::DateCreatedUpdated { reason } => (
                            file.date_validation_created
                                .frontmatter_date
                                .clone()
                                .unwrap_or_default(),
                            format!(
                                "[[{}]]",
                                file.date_validation_created
                                    .file_system_date
                                    .format("%Y-%m-%d")
                            ),
                            reason.to_string(),
                        ),
                        PersistReason::DateModifiedUpdated { reason } => (
                            file.date_validation_modified
                                .frontmatter_date
                                .clone()
                                .unwrap_or_default(),
                            format!(
                                "[[{}]]",
                                file.date_validation_modified
                                    .file_system_date
                                    .format("%Y-%m-%d")
                            ),
                            reason.to_string(),
                        ),
                        PersistReason::DateCreatedFixApplied => (
                            file.date_created_fix
                                .date_string
                                .clone()
                                .unwrap_or_default(),
                            file.date_created_fix
                                .fix_date
                                .map(|d| format!("[[{}]]", d.format("%Y-%m-%d")))
                                .unwrap_or_default(),
                            String::new(),
                        ),
                        PersistReason::BackPopulated => (
                            String::new(),
                            String::new(),
                            format!("{} instances", back_populate_count),
                        ),
                        PersistReason::ImageReferencesModified => (
                            String::new(),
                            String::new(),
                            format!("{} instances", image_refs_count),
                        ),
                    };

                    rows.push(vec![
                        wikilink.clone(),
                        reason.to_string(),
                        reason_info,
                        before,
                        after,
                    ]);
                }
            }
        }

        if !rows.is_empty() {
            rows.sort_by(|a, b| {
                let file_cmp = a[0].to_lowercase().cmp(&b[0].to_lowercase());
                if file_cmp == std::cmp::Ordering::Equal {
                    a[1].cmp(&b[1])
                } else {
                    file_cmp
                }
            });

            writer.writeln(LEVEL1, "files to be updated")?;
            writer.writeln("", "")?;

            let headers = &["file", "persist reason", "info", "before", "after"];
            let alignments = &[
                ColumnAlignment::Left,
                ColumnAlignment::Left,
                ColumnAlignment::Left,
                ColumnAlignment::Left,
                ColumnAlignment::Left,
            ];

            for (i, chunk) in rows.chunks(500).enumerate() {
                if i > 0 {
                    writer.writeln("", "")?;
                }
                writer.write_markdown_table(headers, chunk, Some(alignments))?;
            }
        }

        Ok(())
    }
}

fn process_file(
    sorted_wikilinks: &[&Wikilink],
    config: &ValidatedConfig,
    markdown_file_info: &mut MarkdownFileInfo,
    ac: &AhoCorasick,
) {
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
        );

        // Store matches instead of accumulating for return
        markdown_file_info.matches.unambiguous.extend(matches);
    }
}

fn process_line(
    line: &str,
    line_idx: usize,
    ac: &AhoCorasick,
    sorted_wikilinks: &[&Wikilink],
    config: &ValidatedConfig,
    markdown_file_info: &MarkdownFileInfo,
) -> Vec<BackPopulateMatch> {
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

    matches
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

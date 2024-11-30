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
use crate::{
    format_back_populate_header, pluralize_occurrence_in_files, Timer, BACK_POPULATE_COUNT_PREFIX,
    BACK_POPULATE_COUNT_SUFFIX, BACK_POPULATE_TABLE_HEADER_SUFFIX, COL_OCCURRENCES,
    COL_SOURCE_TEXT, COL_TEXT, COL_WILL_REPLACE_WITH, LEVEL2, LEVEL3, LEVEL4, MATCHES_AMBIGUOUS,
    MATCHES_UNAMBIGUOUS,
};
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
    pub fn identify_ambiguous_matches(&mut self) {
        // Create target and display_text maps as before...
        let mut target_map: HashMap<String, String> = HashMap::new();
        for wikilink in &self.wikilinks_sorted {
            let lower_target = wikilink.target.to_lowercase();
            if !target_map.contains_key(&lower_target)
                || wikilink.target.to_lowercase() == wikilink.target
            {
                target_map.insert(lower_target.clone(), wikilink.target.clone());
            }
        }

        let mut display_text_map: HashMap<String, HashSet<String>> = HashMap::new();
        for wikilink in &self.wikilinks_sorted {
            let lower_display_text = wikilink.display_text.to_lowercase();
            let lower_target = wikilink.target.to_lowercase();
            if let Some(canonical_target) = target_map.get(&lower_target) {
                display_text_map
                    .entry(lower_display_text.clone())
                    .or_default()
                    .insert(canonical_target.clone());
            }
        }

        // Process each file's matches
        for markdown_file in &mut self.markdown_files.iter_mut() {
            // Create a map to group matches by their lowercased found_text within this file
            let mut matches_by_text: HashMap<String, Vec<BackPopulateMatch>> = HashMap::new();

            // Drain matches from the file into our temporary map
            let file_matches = std::mem::take(&mut markdown_file.matches.unambiguous);
            for match_info in file_matches {
                let lower_found_text = match_info.found_text.to_lowercase();
                matches_by_text
                    .entry(lower_found_text)
                    .or_default()
                    .push(match_info);
            }

            // Process each group of matches
            for (found_text_lower, text_matches) in matches_by_text {
                if let Some(targets) = display_text_map.get(&found_text_lower) {
                    if targets.len() > 1 {
                        // This is an ambiguous match
                        // Add to the file's ambiguous collection
                        markdown_file.matches.ambiguous.extend(text_matches.clone());
                    } else {
                        // Unambiguous matches go back into the markdown_file
                        markdown_file.matches.unambiguous.extend(text_matches);
                    }
                } else {
                    // Handle unclassified matches
                    println!(
                        "[WARNING] Found unclassified matches for '{}' in file '{}'",
                        found_text_lower,
                        markdown_file.path.display()
                    );
                    markdown_file.matches.unambiguous.extend(text_matches);
                }
            }
        }
    }

    pub fn find_all_back_populate_matches(
        &mut self,
        config: &ValidatedConfig,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let _timer = Timer::new("find_all_back_populate_matches");

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

    pub fn apply_back_populate_changes(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Only process files that have matches
        // matches have been pruned to only unambiguous matches
        for markdown_file in self.markdown_files.iter_mut() {
            if markdown_file.matches.unambiguous.is_empty() {
                continue;
            }

            // Sort matches by line number and position (reverse position for same line)
            let mut sorted_matches = markdown_file.matches.unambiguous.clone();
            sorted_matches.sort_by_key(|m| (m.line_number, std::cmp::Reverse(m.position)));

            let mut updated_content = String::new();
            let mut current_line_num = 1;

            // Process line by line
            for (line_idx, line) in markdown_file.content.lines().enumerate() {
                if current_line_num != line_idx + 1 {
                    updated_content.push_str(line);
                    updated_content.push('\n');
                    continue;
                }

                // Collect matches for the current line
                let line_matches: Vec<&BackPopulateMatch> = sorted_matches
                    .iter()
                    .filter(|m| m.line_number == current_line_num)
                    .collect();

                // Apply matches in reverse order if there are any
                let mut updated_line = line.to_string();
                if !line_matches.is_empty() {
                    updated_line =
                        apply_line_replacements(line, &line_matches, &markdown_file.path);
                }

                updated_content.push_str(&updated_line);
                updated_content.push('\n');
                current_line_num += 1;
            }

            // Final validation check
            if updated_content.contains("[[[")
                || updated_content.contains("]]]")
                || updated_content.matches("[[").count() != updated_content.matches("]]").count()
            {
                eprintln!(
                    "Unintended pattern detected in file '{}'.\nContent has mismatched or unexpected nesting.\nFull content:\n{}",
                    markdown_file.path.display(),
                    updated_content.escape_debug()
                );
                panic!(
                    "Unintended nesting or malformed brackets detected in file '{}'. Please check the content above for any hidden or misplaced patterns.",
                    markdown_file.path.display(),
                );
            }

            // Update the content and mark file as modified
            markdown_file.content = updated_content.trim_end().to_string();
            markdown_file.mark_as_back_populated();
        }

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

    pub fn write_ambiguous_matches_table(
        &self,
        writer: &ThreadSafeWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Skip if no files have ambiguous matches
        let has_ambiguous = self
            .markdown_files
            .iter()
            .any(|file| !file.matches.ambiguous.is_empty());

        if !has_ambiguous {
            return Ok(());
        }

        writer.writeln(LEVEL2, MATCHES_AMBIGUOUS)?;

        // Create a map to group ambiguous matches by their display text (case-insensitive)
        let mut matches_by_text: HashMap<String, (HashSet<String>, Vec<BackPopulateMatch>)> =
            HashMap::new();

        // First pass: collect all matches and their targets
        for markdown_file in self.markdown_files.iter() {
            for match_info in &markdown_file.matches.ambiguous {
                let key = match_info.found_text.to_lowercase();
                let entry = matches_by_text
                    .entry(key)
                    .or_insert((HashSet::new(), Vec::new()));
                entry.1.push(match_info.clone());
            }
        }

        // Second pass: collect targets for each found text
        for wikilink in &self.wikilinks_sorted {
            if let Some(entry) = matches_by_text.get_mut(&wikilink.display_text.to_lowercase()) {
                entry.0.insert(wikilink.target.clone());
            }
        }

        // Convert to sorted vec for consistent output
        let mut sorted_matches: Vec<_> = matches_by_text.into_iter().collect();
        sorted_matches.sort_by(|(a, _), (b, _)| a.cmp(b));

        // Write out each group of matches
        for (display_text, (targets, matches)) in sorted_matches {
            writer.writeln(
                LEVEL3,
                &format!("\"{}\" matches {} targets:", display_text, targets.len(),),
            )?;

            // Write out all possible targets
            let mut sorted_targets: Vec<_> = targets.into_iter().collect();
            sorted_targets.sort();
            for target in sorted_targets {
                writer.writeln(
                    "",
                    &format!("- \\[\\[{}|{}]]", target.to_wikilink(), display_text),
                )?;
            }

            // Reuse existing table writing code for the matches
            write_back_populate_table(writer, &matches, false, 0)?;
        }

        Ok(())
    }
}

fn apply_line_replacements(
    line: &str,
    line_matches: &[&BackPopulateMatch],
    file_path: &PathBuf,
) -> String {
    let mut updated_line = line.to_string();

    // Sort matches in descending order by `position`
    let mut sorted_matches = line_matches.to_vec();
    sorted_matches.sort_by_key(|m| std::cmp::Reverse(m.position));

    // Apply replacements in sorted (reverse) order
    for match_info in sorted_matches {
        let start = match_info.position;
        let end = start + match_info.found_text.len();

        // Check for UTF-8 boundary issues
        if !updated_line.is_char_boundary(start) || !updated_line.is_char_boundary(end) {
            eprintln!(
                "Error: Invalid UTF-8 boundary in file '{:?}', line {}.\n\
                Match position: {} to {}.\nLine content:\n{}\nFound text: '{}'\n",
                file_path, match_info.line_number, start, end, updated_line, match_info.found_text
            );
            panic!("Invalid UTF-8 boundary detected. Check positions and text encoding.");
        }

        // Perform the replacement
        updated_line.replace_range(start..end, &match_info.replacement);

        // Validation check after each replacement
        if updated_line.contains("[[[") || updated_line.contains("]]]") {
            eprintln!(
                "\nWarning: Potential nested pattern detected after replacement in file '{:?}', line {}.\n\
                Current line:\n{}\n",
                file_path, match_info.line_number, updated_line
            );
        }
    }

    updated_line
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
        markdown_file_info.matches.unambiguous.extend(matches);
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

pub fn write_back_populate_table(
    writer: &ThreadSafeWriter,
    matches: &[BackPopulateMatch],
    is_unambiguous_match: bool,
    wikilinks_count: usize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if is_unambiguous_match {
        writer.writeln(LEVEL2, MATCHES_UNAMBIGUOUS)?;
        writer.writeln(
            "",
            &format!(
                "{} {} {}",
                BACK_POPULATE_COUNT_PREFIX, wikilinks_count, BACK_POPULATE_COUNT_SUFFIX
            ),
        )?;
    }

    // Step 1: Group matches by found_text (case-insensitive) using a HashMap
    let mut matches_by_text: HashMap<String, Vec<&BackPopulateMatch>> = HashMap::new();
    for m in matches {
        let key = m.found_text.to_lowercase();
        matches_by_text.entry(key).or_default().push(m);
    }

    // Step 2: Get display text for each group (use first occurrence's case)
    let mut display_text_map: HashMap<String, String> = HashMap::new();
    for m in matches {
        let key = m.found_text.to_lowercase();
        display_text_map
            .entry(key)
            .or_insert_with(|| m.found_text.clone());
    }

    if is_unambiguous_match {
        // Count unique files across all matches
        let unique_files: HashSet<String> =
            matches.iter().map(|m| m.relative_path.clone()).collect();
        writer.writeln(
            "",
            &format!(
                "{} {}",
                format_back_populate_header(matches.len(), unique_files.len()),
                BACK_POPULATE_TABLE_HEADER_SUFFIX,
            ),
        )?;
    }

    // Headers for the tables
    let headers: Vec<&str> = if is_unambiguous_match {
        vec![
            "file name",
            "line",
            COL_TEXT,
            COL_OCCURRENCES,
            COL_WILL_REPLACE_WITH,
            COL_SOURCE_TEXT,
        ]
    } else {
        vec!["file name", "line", COL_TEXT, COL_OCCURRENCES]
    };

    // Step 3: Collect and sort the keys
    let mut sorted_found_texts: Vec<String> = matches_by_text.keys().cloned().collect();
    sorted_found_texts.sort();

    // Step 4: Iterate over the sorted keys
    for found_text_key in sorted_found_texts {
        let text_matches = &matches_by_text[&found_text_key];
        let display_text = &display_text_map[&found_text_key];
        let total_occurrences = text_matches.len();
        let file_paths: HashSet<String> = text_matches
            .iter()
            .map(|m| m.relative_path.clone())
            .collect();

        let level_string = if is_unambiguous_match { LEVEL3 } else { LEVEL4 };

        writer.writeln(
            level_string,
            &format!(
                "found: \"{}\" ({})",
                display_text,
                pluralize_occurrence_in_files(total_occurrences, file_paths.len())
            ),
        )?;

        // Sort matches by file path and line number
        let mut sorted_matches = text_matches.to_vec();
        sorted_matches.sort_by(|a, b| {
            let file_a = Path::new(&a.relative_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let file_b = Path::new(&b.relative_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            // First compare by file name (case-insensitive)
            let file_cmp = file_a.to_lowercase().cmp(&file_b.to_lowercase());
            if file_cmp != std::cmp::Ordering::Equal {
                return file_cmp;
            }

            // Then by line number within the same file
            a.line_number.cmp(&b.line_number)
        });

        // Consolidate matches
        let consolidated = consolidate_matches(&sorted_matches);

        // Prepare rows
        let mut table_rows = Vec::new();

        for m in consolidated {
            let file_path = Path::new(&m.file_path);
            let file_stem = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

            // Create a row for each line, maintaining the consolidation of occurrences
            for line_info in m.line_info {
                let highlighted_line = highlight_matches(
                    &line_info.line_text,
                    &line_info.positions,
                    display_text.len(),
                );

                let mut row = vec![
                    file_stem.to_wikilink(),
                    line_info.line_number.to_string(),
                    escape_pipe(&highlighted_line),
                    line_info.positions.len().to_string(),
                ];

                // Only add replacement columns for unambiguous matches
                if is_unambiguous_match {
                    let replacement = if m.in_markdown_table {
                        m.replacement.clone()
                    } else {
                        escape_pipe(&m.replacement)
                    };
                    row.push(replacement.clone());
                    row.push(escape_brackets(&replacement));
                }

                table_rows.push(row);
            }
        }

        // Write the table with appropriate column alignments
        let alignments = if is_unambiguous_match {
            vec![
                ColumnAlignment::Left,
                ColumnAlignment::Right,
                ColumnAlignment::Left,
                ColumnAlignment::Center,
                ColumnAlignment::Left,
                ColumnAlignment::Left,
            ]
        } else {
            vec![
                ColumnAlignment::Left,
                ColumnAlignment::Right,
                ColumnAlignment::Left,
                ColumnAlignment::Center,
            ]
        };

        writer.write_markdown_table(&headers, &table_rows, Some(&alignments))?;
        writer.writeln("", "\n---")?;
    }

    Ok(())
}

// Helper function to highlight all instances of a pattern in text
fn highlight_matches(text: &str, positions: &[usize], match_length: usize) -> String {
    let mut result = String::with_capacity(text.len() * 2);
    let mut last_end = 0;

    // Sort positions to ensure we process them in order
    let mut sorted_positions = positions.to_vec();
    sorted_positions.sort_unstable();

    for &start in sorted_positions.iter() {
        let end = start + match_length;

        // Validate UTF-8 boundaries
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            eprintln!(
                "Invalid UTF-8 boundary detected at position {} or {}",
                start, end
            );
            return text.to_string();
        }

        // Add text before the match
        result.push_str(&text[last_end..start]);

        // Add the highlighted match
        result.push_str("<span style=\"color: red;\">");
        result.push_str(&text[start..end]);
        result.push_str("</span>");

        last_end = end;
    }

    // Add any remaining text after the last match
    result.push_str(&text[last_end..]);
    result
}

#[derive(Debug, Clone)]
struct ConsolidatedMatch {
    file_path: String,
    line_info: Vec<LineInfo>, // Sorted vector of line information
    replacement: String,
    in_markdown_table: bool,
}

#[derive(Debug, Clone)]
struct LineInfo {
    line_number: usize,
    line_text: String,
    positions: Vec<usize>, // Multiple positions for same line
}

fn consolidate_matches(matches: &[&BackPopulateMatch]) -> Vec<ConsolidatedMatch> {
    // First, group by file path and line number
    let mut line_map: HashMap<(String, usize), LineInfo> = HashMap::new();
    let mut file_info: HashMap<String, (String, bool)> = HashMap::new(); // Tracks replacement and table status per file

    // Group matches by file and line
    for match_info in matches {
        let key = (match_info.relative_path.clone(), match_info.line_number);

        // Update or create line info
        let line_info = line_map.entry(key).or_insert(LineInfo {
            // line_number: match_info.line_number,
            line_number: match_info.line_number + match_info.frontmatter_line_count,
            line_text: match_info.line_text.clone(),
            positions: Vec::new(),
        });
        line_info.positions.push(match_info.position);

        // Track file-level information
        file_info.insert(
            match_info.relative_path.clone(),
            (match_info.replacement.clone(), match_info.in_markdown_table),
        );
    }

    // Convert to consolidated matches, sorting lines within each file
    let mut result = Vec::new();
    for (file_path, (replacement, in_markdown_table)) in file_info {
        let mut file_lines: Vec<LineInfo> = line_map
            .iter()
            .filter(|((path, _), _)| path == &file_path)
            .map(|((_, _), line_info)| line_info.clone())
            .collect();

        // Sort lines by line number
        file_lines.sort_by_key(|line| line.line_number);

        result.push(ConsolidatedMatch {
            file_path,
            line_info: file_lines,
            replacement,
            in_markdown_table,
        });
    }

    // Sort consolidated matches by file path
    result.sort_by(|a, b| {
        let file_a = Path::new(&a.file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let file_b = Path::new(&b.file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        file_a.cmp(file_b)
    });

    result
}

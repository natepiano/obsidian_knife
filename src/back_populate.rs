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
mod file_processing_state_and_config_tests;
#[cfg(test)]
mod matching_tests;
#[cfg(test)]
mod table_handling_tests;

use crate::config::ValidatedConfig;
use crate::constants::*;
use crate::deterministic_file_search::DeterministicSearch;
use crate::markdown_file_info::MarkdownFileInfo;
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::utils::Timer;
use crate::utils::MARKDOWN_REGEX;
use crate::utils::{ColumnAlignment, ThreadSafeWriter};
use crate::wikilink_types::{InvalidWikilinkReason, ToWikilink, Wikilink};
use aho_corasick::AhoCorasick;
use itertools::Itertools;
use lazy_static::lazy_static;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct BackPopulateMatch {
    found_text: String,
    full_path: PathBuf,
    in_markdown_table: bool,
    line_number: usize,
    line_text: String,
    position: usize,
    relative_path: String,
    replacement: String,
}

#[derive(Debug)]
struct AmbiguousMatch {
    display_text: String,
    targets: Vec<String>,
    matches: Vec<BackPopulateMatch>,
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

pub fn process_back_populate(
    config: &ValidatedConfig,
    obsidian_repository_info: &mut ObsidianRepositoryInfo,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL1, BACK_POPULATE_COUNT_PREFIX)?;
    let _timer = Timer::new("process_back_populate");

    // Write invalid wikilinks table first
    write_invalid_wikilinks_table(writer, obsidian_repository_info)?;

    let matches = find_all_back_populate_matches(config, obsidian_repository_info)?;
    if let Some(filter) = config.back_populate_file_filter() {
        writer.writeln(
            "",
            &format!(
                "{} {}\n{}\n",
                BACK_POPULATE_FILE_FILTER_PREFIX,
                filter.to_wikilink(),
                BACK_POPULATE_FILE_FILTER_SUFFIX
            ),
        )?;
    }

    if matches.is_empty() {
        return Ok(());
    }

    // Split matches into ambiguous and unambiguous
    let (ambiguous_matches, unambiguous_matches) =
        identify_ambiguous_matches(&matches, &obsidian_repository_info.wikilinks_sorted);

    // Write ambiguous matches first if any exist
    write_ambiguous_matches(writer, &ambiguous_matches)?;

    // Only process unambiguous matches
    if !unambiguous_matches.is_empty() {
        write_back_populate_table(
            writer,
            &unambiguous_matches,
            true,
            obsidian_repository_info.wikilinks_sorted.len(),
        )?;

        update_date_modified(obsidian_repository_info, &unambiguous_matches);

        //  apply_back_populate_changes(config, &unambiguous_matches)?;
    }

    Ok(())
}

fn update_date_modified(
    obsidian_repository_info: &mut ObsidianRepositoryInfo,
    unambiguous_matches: &Vec<BackPopulateMatch>,
) {
    // Collect distinct paths from unambiguous matches
    let distinct_paths: Vec<PathBuf> = unambiguous_matches
        .iter()
        .map(|m| m.full_path.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    // Update modified dates for all files that would be changed
    obsidian_repository_info.update_modified_dates(&distinct_paths);
}

fn find_all_back_populate_matches(
    config: &ValidatedConfig,
    obsidian_repository_info: &mut ObsidianRepositoryInfo,
) -> Result<Vec<BackPopulateMatch>, Box<dyn Error + Send + Sync>> {
    let searcher = DeterministicSearch::new(config.back_populate_file_count());

    let ac = obsidian_repository_info
        .wikilinks_ac
        .as_ref()
        .expect("Wikilinks AC pattern should be initialized");
    let sorted_wikilinks: Vec<&Wikilink> =
        obsidian_repository_info.wikilinks_sorted.iter().collect();

    let matches = searcher.search_with_info(
        &mut obsidian_repository_info.markdown_files,
        |markdown_file_info: &mut MarkdownFileInfo| {
            if !cfg!(test) {
                if let Some(filter) = config.back_populate_file_filter() {
                    if !markdown_file_info.path.ends_with(filter) {
                        return None;
                    }
                }
            }

            match process_file(&sorted_wikilinks, config, markdown_file_info, ac) {
                Ok(file_matches) if !file_matches.is_empty() => Some(file_matches),
                _ => None,
            }
        },
    );

    Ok(matches.into_iter().flatten().collect())
}

fn identify_ambiguous_matches(
    matches: &[BackPopulateMatch],
    wikilinks: &[Wikilink],
) -> (Vec<AmbiguousMatch>, Vec<BackPopulateMatch>) {
    // Create a case-insensitive map of targets to their canonical forms
    let mut target_map: HashMap<String, String> = HashMap::new();
    for wikilink in wikilinks {
        let lower_target = wikilink.target.to_lowercase();
        // If this is the first time we've seen this target (case-insensitive),
        // or if this version is an exact match for the lowercase version,
        // use this as the canonical form
        if !target_map.contains_key(&lower_target)
            || wikilink.target.to_lowercase() == wikilink.target
        {
            target_map.insert(lower_target.clone(), wikilink.target.clone());
        }
    }

    // Create a map of lowercased display_text to normalized targets
    let mut display_text_map: HashMap<String, HashSet<String>> = HashMap::new();
    for wikilink in wikilinks {
        let lower_display_text = wikilink.display_text.to_lowercase(); // Lowercase display_text
        let lower_target = wikilink.target.to_lowercase();
        // Use the canonical form of the target from our target_map
        if let Some(canonical_target) = target_map.get(&lower_target) {
            display_text_map
                .entry(lower_display_text.clone()) // Use lowercased display_text as key
                .or_default()
                .insert(canonical_target.clone());
        }
    }

    // Group matches by their lowercased found_text
    let mut matches_by_text: HashMap<String, Vec<BackPopulateMatch>> = HashMap::new();
    for match_info in matches {
        let lower_found_text = match_info.found_text.to_lowercase(); // Lowercase found_text

        matches_by_text
            .entry(lower_found_text) // Use lowercased found_text as key
            .or_default()
            .push(match_info.clone());
    }

    // Identify truly ambiguous matches and separate them
    let mut ambiguous_matches = Vec::new();
    let mut unambiguous_matches = Vec::new();
    let mut unclassified_matches = Vec::new();

    for (found_text_lower, text_matches) in matches_by_text {
        if let Some(targets) = display_text_map.get(&found_text_lower) {
            if targets.len() > 1 {
                // Only log ambiguous matches when there are multiple targets

                ambiguous_matches.push(AmbiguousMatch {
                    display_text: found_text_lower.clone(), // Use lowercased found_text
                    targets: targets.iter().cloned().collect(),
                    matches: text_matches.clone(),
                });
            } else {
                unambiguous_matches.extend(text_matches.clone());
            }
        } else {
            // Collect unclassified matches
            unclassified_matches.extend(text_matches.clone());
        }
    }

    // Log unclassified matches
    if !unclassified_matches.is_empty() {
        println!(
            "[WARNING] Found {} unclassified matches.",
            unclassified_matches.len()
        );
        for m in &unclassified_matches {
            println!(
                "[WARNING] Unclassified Match: '{}' in file '{}'",
                m.found_text, m.relative_path
            );
        }

        // Optionally, treat them as unambiguous - don't
        // let it fail if we have something unclassified
        // unambiguous_matches.extend(unclassified_matches);
    }

    // Calculate the total number of classified matches
    let total_classified = ambiguous_matches
        .iter()
        .map(|m| m.matches.len())
        .sum::<usize>()
        + unambiguous_matches.len();

    // Assert that the total matches classified equals the total matches passed in
    assert_eq!(
        total_classified,
        matches.len(),
        "Mismatch in match classification: total_classified={}, matches.len()={}",
        total_classified,
        matches.len()
    );

    // Sort ambiguous matches by display text for consistent output
    ambiguous_matches.sort_by(|a, b| a.display_text.cmp(&b.display_text));

    (ambiguous_matches, unambiguous_matches)
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

// fn process_file(
//     sorted_wikilinks: &[&Wikilink],
//     config: &ValidatedConfig,
//     markdown_file_info: &MarkdownFileInfo,
//     ac: &AhoCorasick,
// ) -> Result<Vec<BackPopulateMatch>, Box<dyn Error + Send + Sync>> {
//     let mut ac_matches = Vec::new();
//
//     let reader = BufReader::new(markdown_file_info.content.as_bytes());
//     let mut state = FileProcessingState::new();
//
//     for (line_idx, line) in reader.lines().enumerate() {
//         let line = line?;
//
//         // Skip empty or whitespace-only lines early
//         if line.trim().is_empty() {
//             continue;
//         }
//
//         // Update state and skip if needed
//         state.update_for_line(&line);
//         if state.should_skip_line() {
//             continue;
//         }
//
//         // Get AC matches (existing functionality)
//         let line_ac_matches = process_line(
//             line_idx,
//             &line,
//             ac,
//             sorted_wikilinks,
//             config,
//             markdown_file_info,
//         )?;
//
//         ac_matches.extend(line_ac_matches);
//     }
//
//     Ok(ac_matches)
// }
fn process_file(
    sorted_wikilinks: &[&Wikilink],
    config: &ValidatedConfig,
    markdown_file_info: &mut MarkdownFileInfo,
    ac: &AhoCorasick,
) -> Result<Vec<BackPopulateMatch>, Box<dyn Error + Send + Sync>> {
    let mut all_matches = Vec::new();
    let content = markdown_file_info.content.clone();
    let mut state = FileProcessingState::new();
    let mut updated_content = String::new();

    for (line_idx, line) in content.lines().enumerate() {
        let mut line_text = line.to_string();

        // Skip empty/whitespace lines early
        if line_text.trim().is_empty() {
            updated_content.push_str(&line_text);
            updated_content.push('\n');
            continue;
        }

        // Update state and skip if needed
        state.update_for_line(&line_text);
        if state.should_skip_line() {
            updated_content.push_str(&line_text);
            updated_content.push('\n');
            continue;
        }

        // Process the line and collect matches
        let matches = process_line(
            &mut line_text,
            line_idx,
            ac,
            sorted_wikilinks,
            config,
            markdown_file_info,
        )?;

        all_matches.extend(matches);
        updated_content.push_str(&line_text);
        updated_content.push('\n');
    }

    // Update the content if we found matches
    if !all_matches.is_empty() {
        markdown_file_info.content = updated_content;
    }

    Ok(all_matches)
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

// fn process_line(
//     line_idx: usize,
//     line: &str,
//     ac: &AhoCorasick,
//     sorted_wikilinks: &[&Wikilink],
//     config: &ValidatedConfig,
//     markdown_file_info: &MarkdownFileInfo,
// ) -> Result<Vec<BackPopulateMatch>, Box<dyn Error + Send + Sync>> {
//     let mut matches = Vec::new();
//
//     let exclusion_zones = collect_exclusion_zones(line, config, markdown_file_info);
//
//     for mat in ac.find_iter(line) {
//         // use the ac pattern - which returns the index that matches
//         // the index of how the ac was built from sorted_wikilinks in the first place
//         // so now we can extract the specific wikilink we need
//         let wikilink = sorted_wikilinks[mat.pattern()];
//         let starts_at = mat.start();
//         let ends_at = mat.end();
//
//         // Skip if in exclusion zone
//         if range_overlaps(&exclusion_zones, starts_at, ends_at) {
//             continue;
//         }
//
//         let matched_text = &line[starts_at..ends_at];
//
//         if !is_word_boundary(line, starts_at, ends_at) {
//             continue;
//         }
//
//         // Rest of the validation
//         if should_create_match(
//             line,
//             starts_at,
//             matched_text,
//             &markdown_file_info.path,
//             markdown_file_info,
//         ) {
//             let mut replacement = if matched_text == wikilink.target {
//                 wikilink.target.to_wikilink()
//             } else {
//                 // Use aliased format for case differences or actual aliases
//                 wikilink.target.to_aliased_wikilink(matched_text)
//             };
//
//             let in_markdown_table = is_in_markdown_table(line, matched_text);
//             if in_markdown_table {
//                 replacement = replacement.replace('|', r"\|");
//             }
//
//             let relative_path =
//                 format_relative_path(&markdown_file_info.path, config.obsidian_path());
//
//             matches.push(BackPopulateMatch {
//                 found_text: matched_text.to_string(),
//                 full_path: markdown_file_info.path.clone(),
//                 line_number: line_idx + 1,
//                 line_text: line.to_string(),
//                 position: starts_at,
//                 in_markdown_table,
//                 relative_path,
//                 replacement,
//             });
//         }
//     }
//
//     Ok(matches)
// }
fn process_line(
    line_text: &mut String,
    line_idx: usize,
    ac: &AhoCorasick,
    sorted_wikilinks: &[&Wikilink],
    config: &ValidatedConfig,
    markdown_file_info: &MarkdownFileInfo,
) -> Result<Vec<BackPopulateMatch>, Box<dyn Error + Send + Sync>> {
    let mut matches = Vec::new();
    let exclusion_zones = collect_exclusion_zones(line_text, config, markdown_file_info);
    let original_line = line_text.clone();

    // Collect all valid matches first
    for mat in ac.find_iter(&original_line) {
        let wikilink = sorted_wikilinks[mat.pattern()];
        let starts_at = mat.start();
        let ends_at = mat.end();

        if range_overlaps(&exclusion_zones, starts_at, ends_at) {
            continue;
        }

        let matched_text = &original_line[starts_at..ends_at];
        if !is_word_boundary(&original_line, starts_at, ends_at) {
            continue;
        }

        if should_create_match(
            &original_line,
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

            let in_markdown_table = is_in_markdown_table(&original_line, matched_text);
            if in_markdown_table {
                replacement = replacement.replace('|', r"\|");
            }

            let relative_path =
                format_relative_path(&markdown_file_info.path, config.obsidian_path());

            matches.push(BackPopulateMatch {
                found_text: matched_text.to_string(),
                full_path: markdown_file_info.path.clone(),
                line_number: line_idx + 1,
                line_text: original_line.clone(),
                position: starts_at,
                in_markdown_table,
                relative_path,
                replacement,
            });
        }
    }

    // Sort matches in reverse order by position and apply changes
    matches.sort_by_key(|m| std::cmp::Reverse(m.position));

    // Apply changes only if we have matches
    if !matches.is_empty() {
        let mut updated_line = original_line.clone();

        // Apply replacements in sorted (reverse) order
        for match_info in &matches {
            let start = match_info.position;
            let end = start + match_info.found_text.len();

            // Check for UTF-8 boundary issues
            if !updated_line.is_char_boundary(start) || !updated_line.is_char_boundary(end) {
                eprintln!(
                    "Error: Invalid UTF-8 boundary in file '{:?}', line {}.\n\
                   Match position: {} to {}.\nLine content:\n{}\nFound text: '{}'\n",
                    markdown_file_info.path,
                    match_info.line_number,
                    start,
                    end,
                    updated_line,
                    match_info.found_text
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
                    markdown_file_info.path, match_info.line_number, updated_line
                );
            }
        }

        // Final validation check
        if updated_line.matches("[[").count() != updated_line.matches("]]").count() {
            eprintln!(
                "Unmatched brackets detected in file '{}', line {}.\nContent: {}",
                markdown_file_info.path.display(),
                line_idx + 1,
                updated_line.escape_debug()
            );
            panic!("Unmatched brackets detected. Please check the content.");
        }

        // Only update the line if all validations pass
        *line_text = updated_line;
    }

    Ok(matches)
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
            line_number: match_info.line_number,
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

fn write_ambiguous_matches(
    writer: &ThreadSafeWriter,
    ambiguous_matches: &[AmbiguousMatch],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if ambiguous_matches.is_empty() {
        return Ok(());
    }

    writer.writeln(LEVEL2, MATCHES_AMBIGUOUS)?;

    for ambiguous_match in ambiguous_matches {
        writer.writeln(
            LEVEL3,
            &format!(
                "\"{}\" matches {} targets:",
                ambiguous_match.display_text,
                ambiguous_match.targets.len(),
            ),
        )?;

        // Write out all possible targets
        for target in &ambiguous_match.targets {
            writer.writeln(
                "",
                &format!(
                    "- \\[\\[{}|{}]]",
                    target.to_wikilink(),
                    ambiguous_match.display_text
                ),
            )?;
        }

        // Reuse existing table writing code for the matches
        write_back_populate_table(writer, &ambiguous_match.matches, false, 0)?;
    }

    Ok(())
}

fn write_invalid_wikilinks_table(
    writer: &ThreadSafeWriter,
    obsidian_repository_info: &ObsidianRepositoryInfo,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Collect all invalid wikilinks from all files
    let invalid_wikilinks = obsidian_repository_info
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

fn write_back_populate_table(
    writer: &ThreadSafeWriter,
    matches: &[BackPopulateMatch],
    is_unambiguous_match: bool,
    match_count: usize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if is_unambiguous_match {
        writer.writeln(LEVEL2, MATCHES_UNAMBIGUOUS)?;
        writer.writeln(
            "",
            &format!(
                "{} {} {}",
                BACK_POPULATE_COUNT_PREFIX, match_count, BACK_POPULATE_COUNT_SUFFIX
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

// Helper function to escape pipes in Markdown strings
fn escape_pipe(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() * 2);
    let chars: Vec<char> = text.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '|' {
            // Count the number of consecutive backslashes before '|'
            let mut backslash_count = 0;
            let mut j = i;
            while j > 0 && chars[j - 1] == '\\' {
                backslash_count += 1;
                j -= 1;
            }

            // If even number of backslashes, '|' is not escaped
            if backslash_count % 2 == 0 {
                escaped.push('\\');
            }
            escaped.push('|');
        } else {
            escaped.push(ch);
        }
        i += 1;
    }

    escaped
}

// Helper function to escape pipes and brackets for visual verification
fn escape_brackets(text: &str) -> String {
    text.replace('[', r"\[").replace(']', r"\]")
}

fn apply_back_populate_changes(
    config: &ValidatedConfig,
    matches: &[BackPopulateMatch],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if !config.apply_changes() {
        return Ok(());
    }

    let mut matches_by_file: BTreeMap<PathBuf, Vec<&BackPopulateMatch>> = BTreeMap::new();
    for match_info in matches {
        matches_by_file
            .entry(match_info.full_path.clone())
            .or_default()
            .push(match_info);
    }

    for (full_path, file_matches) in matches_by_file {
        let content = fs::read_to_string(&full_path)?;
        let mut updated_content = String::new();

        // Sort and group matches by line number
        let mut sorted_matches = file_matches;
        sorted_matches.sort_by_key(|m| (m.line_number, std::cmp::Reverse(m.position)));
        let mut current_line_num = 1;

        // Process line-by-line with line numbers and match positions checked
        for (line_index, line) in content.lines().enumerate() {
            if current_line_num != line_index + 1 {
                updated_content.push_str(line);
                updated_content.push('\n');
                continue;
            }

            // Collect matches for the current line
            let line_matches: Vec<&BackPopulateMatch> = sorted_matches
                .iter()
                .filter(|m| m.line_number == current_line_num)
                .cloned()
                .collect();

            // Apply matches in reverse order if there are any
            let mut updated_line = line.to_string();
            if !line_matches.is_empty() {
                updated_line = apply_line_replacements(line, &line_matches, &full_path);
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
                full_path.display(),
                updated_content.escape_debug() // use escape_debug for detailed inspection
            );
            panic!(
                "Unintended nesting or malformed brackets detected in file '{}'. Please check the content above for any hidden or misplaced patterns.",
                full_path.display(),
            );
        }

        fs::write(full_path, updated_content.trim_end())?;
    }

    Ok(())
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

fn format_relative_path(path: &Path, base_path: &Path) -> String {
    path.strip_prefix(base_path)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

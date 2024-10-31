use crate::constants::*;
use crate::deterministic_file_search::DeterministicSearch;
use crate::scan::{MarkdownFileInfo, ObsidianRepositoryInfo};
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::validated_config::ValidatedConfig;
use crate::wikilink::{CompiledWikilink, MARKDOWN_REGEX};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use lazy_static::lazy_static;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct BackPopulateMatch {
    file_path: String,
    line_number: usize,
    line_text: String,
    found_text: String,
    replacement: String,
    position: usize,
    in_markdown_table: bool,
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
    obsidian_repository_info: &ObsidianRepositoryInfo,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL1, BACK_POPULATE)?;

    println!("links to back populate: {}", obsidian_repository_info.wikilinks_sorted.len());

    let matches = find_all_back_populate_matches(
        config,
        obsidian_repository_info,
    )?;

    if matches.is_empty() {
        writer.writeln("", "no back population matches found")?;
        return Ok(());
    }

    write_back_populate_table(writer, &matches)?;
    apply_back_populate_changes(config, &matches)?;

    Ok(())
}

fn find_all_back_populate_matches(
    config: &ValidatedConfig,
    obsidian_repository_info: &ObsidianRepositoryInfo,
) -> Result<Vec<BackPopulateMatch>, Box<dyn Error + Send + Sync>> {
    let searcher = DeterministicSearch::new(config.back_populate_file_count());

    // Specify how often you want to log progress (e.g., every 100 files)
    let log_every = 100;

    let ac = obsidian_repository_info.wikilinks_ac.as_ref()
        .expect("Wikilinks AC pattern should be initialized");
    let sorted_wikilinks: Vec<&CompiledWikilink> = obsidian_repository_info.wikilinks_sorted.iter().collect();

    let matches = searcher.search_with_info(
        &obsidian_repository_info.markdown_files,
        |file_path, markdown_file_info| {
            // Filter to process only "amis et famille.md" unless in test
            // estatodo.md
            if !cfg!(test) && !file_path.ends_with("2022-01-18.md") {
               // return None;
            }

            // Process the file if it matches the filter
            match process_file(file_path, &sorted_wikilinks, config, markdown_file_info, ac) {
                Ok(file_matches) if !file_matches.is_empty() => Some(file_matches),
                _ => None,
            }
        },
        log_every, // Pass the logging interval
    );

    Ok(matches.into_iter().flatten().collect())
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct RawMatch {
    line_number: usize,
    position: usize,
    text: String,
    length: usize,
}

fn is_word_boundary(line: &str, starts_at: usize, ends_at: usize) -> bool {
    // Helper to check if a char is a word character (\w in regex)
    fn is_word_char(ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_'
    }

    // Check start boundary
    let start_is_boundary =
        starts_at == 0 || !is_word_char(line[..starts_at].chars().last().unwrap());

    // Check end boundary
    let end_is_boundary =
        ends_at == line.len() || !is_word_char(line[ends_at..].chars().next().unwrap());

    start_is_boundary && end_is_boundary
}

fn collect_ac_matches_with_word_boundaries(
    line: &str,
    line_idx: usize,
    ac: &AhoCorasick,
) -> Vec<RawMatch> {
    let mut matches = Vec::new();

    for mat in ac.find_iter(line) {
        let starts_at = mat.start();
        let ends_at = mat.end();
        let matched_text = &line[starts_at..ends_at];

        if is_word_boundary(line, starts_at, ends_at) {
            matches.push(RawMatch {
                line_number: line_idx + 1,
                position: starts_at,
                text: matched_text.to_string(),
                length: ends_at - starts_at,
            });
        }
    }

    matches.sort_by_key(|m| m.position);
    matches
}

fn collect_regex_matches_with_exclusions(
    line: &str,
    line_idx: usize,
    sorted_wikilinks: &[&CompiledWikilink],
) -> Vec<RawMatch> {
    let mut matches = Vec::new();
    let mut matched_positions = Vec::new();

    // Process each wikilink pattern
    for wikilink in sorted_wikilinks {
        for mat in wikilink.regex.find_iter(line) {
            let starts_at = mat.start();
            let ends_at = mat.end();

            // Skip if overlaps with previous match
            if range_overlaps(&matched_positions, starts_at, ends_at) {
                continue;
            }

            // Add to matched positions to prevent overlaps
            matched_positions.push((starts_at, ends_at));

            matches.push(RawMatch {
                line_number: line_idx + 1,
                position: starts_at,
                text: mat.as_str().to_string(),
                length: ends_at - starts_at,
            });
        }
    }

    matches.sort_by_key(|m| m.position);
    matches
}

fn process_file(
    file_path: &Path,
    sorted_wikilinks: &[&CompiledWikilink],
    config: &ValidatedConfig,
    markdown_file_info: &MarkdownFileInfo,
    ac: &AhoCorasick,
) -> Result<Vec<BackPopulateMatch>, Box<dyn Error + Send + Sync>> {
    let mut ac_matches = Vec::new();

    let content = fs::read_to_string(file_path)?;
    let reader = BufReader::new(content.as_bytes());
    let mut state = FileProcessingState::new();

    for (line_idx, line) in reader.lines().enumerate() {
        let line = line?;

        // Skip empty or whitespace-only lines early
        if line.trim().is_empty() {
            continue;
        }

        // Update state and skip if needed
        state.update_for_line(&line);
        if state.should_skip_line() {
            continue;
        }

        // Get AC matches (existing functionality)
        let line_ac_matches = process_line(
            line_idx,
            &line,
            file_path,
            ac,
            sorted_wikilinks,
            config,
            markdown_file_info,
        )?;

        ac_matches.extend(line_ac_matches);
    }

    Ok(ac_matches)
}

fn validate_raw_matches(
    regex_raw_matches: &[RawMatch],
    ac_raw_matches: &[RawMatch],
    file_path: &Path,
) {
    // ----------------------------------
    // Step 1: Define Files to Skip
    // ----------------------------------

    // Hard-coded list of file names to skip validation
    let files_to_skip: HashSet<&str> = [
        "bœuf.md",
        "Shaoxing wine.md",
        "sweet fermented sauce.md",
        "mint chutney.md",
        "miscellaneous quotes.md", // Add more file names as needed
    ]
    .iter()
    .cloned()
    .collect();

    // Extract the file name as &str
    let file_name = match file_path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name,
        None => {
            println!(
                "Unable to extract file name for path: {:?}. Skipping validation.",
                file_path
            );
            return;
        }
    };

    // Check if the current file is in the skip list
    if files_to_skip.contains(file_name) {
        return;
    }

    // ----------------------------------
    // Step 2: Define Exclusion Lists
    // ----------------------------------

    // Define exclusion lists for regex and AC matches
    let regex_exclusions: HashSet<&str> = ["Apple TV", "red braised beef with tofu \"bamboo\""]
        .iter()
        .cloned()
        .collect();

    let ac_exclusions: HashSet<&str> = [
        "Apple TV",
        "Apple TV+",
        "Disney+",
        "Paramount+",
        "red braised beef with tofu \"bamboo\"",
    ]
    .iter()
    .cloned()
    .collect();

    // ----------------------------------
    // Step 3: Filter Out Specific Matches
    // ----------------------------------

    // Filter out specific matches from regex results
    let filtered_regex_raw: Vec<&RawMatch> = regex_raw_matches
        .iter()
        .filter(|m| !regex_exclusions.contains(m.text.as_str()))
        .collect();

    // Filter out specific matches from AC results
    let filtered_ac_raw: Vec<&RawMatch> = ac_raw_matches
        .iter()
        .filter(|m| !ac_exclusions.contains(m.text.as_str()))
        .collect();

    // ----------------------------------
    // Step 4: Logging Filtered Match Counts
    // ----------------------------------

    println!(
        "{} - r:{} a:{} ",
        file_name,
        filtered_regex_raw.len(),
        filtered_ac_raw.len(),
    );

    // ----------------------------------
    // Step 5: Compare the Filtered Matches
    // ----------------------------------

    if filtered_regex_raw != filtered_ac_raw {
        println!("\nMatch mismatch found in {:?}!", file_path);

        // Find matches unique to regex
        let regex_only: Vec<_> = filtered_regex_raw
            .iter()
            .filter(|regex_match| !filtered_ac_raw.contains(regex_match))
            .collect();

        // Find matches unique to AC
        let ac_only: Vec<_> = filtered_ac_raw
            .iter()
            .filter(|ac_match| !filtered_regex_raw.contains(ac_match))
            .collect();

        // ----------------------------------
        // Step 6: Report Discrepancies
        // ----------------------------------

        if !regex_only.is_empty() {
            println!("\nMatches found by regex but not by AC:");
            for diff in regex_only {
                println!(
                    "Line {}: '{}' at pos {}",
                    diff.line_number, diff.text, diff.position
                );
            }
        }

        if !ac_only.is_empty() {
            println!("\nMatches found by AC but not by regex:");
            for diff in ac_only {
                println!(
                    "Line {}: '{}' at pos {}",
                    diff.line_number, diff.text, diff.position
                );
            }
        }

        // ----------------------------------
        // Step 7: Handle Mismatches
        // ----------------------------------

        panic!("Raw regex and AC matches don't match exactly! Check the output above for details.");
    }
}

fn range_overlaps(ranges: &[(usize, usize)], start: usize, end: usize) -> bool {
    ranges.iter().any(|&(r_start, r_end)| {
        (start >= r_start && start < r_end)
            || (end > r_start && end <= r_end)
            || (start <= r_start && end >= r_end)
    })
}

// fn collect_exclusion_zones(
//     line: &str,
//     config_patterns: Option<&[String]>,
//     file_patterns: Option<&[String]>,
// ) -> Vec<(usize, usize)> {
//     let mut exclusion_zones = Vec::new();
//
//     // Helper closure to process patterns
//     let mut add_zones = |patterns: &[String]| {
//         for pattern in patterns {
//             let exclusion_regex =
//                 regex::Regex::new(&format!(r"(?i){}", regex::escape(pattern))).unwrap();
//             for mat in exclusion_regex.find_iter(line) {
//                 exclusion_zones.push((mat.start(), mat.end()));
//             }
//         }
//     };
//
//     // Process global config patterns
//     if let Some(patterns) = config_patterns {
//         add_zones(patterns);
//     }
//
//     // Process file-specific patterns
//     if let Some(patterns) = file_patterns {
//         add_zones(patterns);
//     }
//
//     // Add Markdown links as exclusion zones
//     // this is simpler than trying to find each match and determine whether it is in a markdown
//     // this just eliminates them all from the start
//     for mat in MARKDOWN_REGEX.find_iter(line) {
//         exclusion_zones.push((mat.start(), mat.end()));
//     }
//
//     exclusion_zones
// }
fn collect_exclusion_zones(
    line: &str,
    config: &ValidatedConfig,
    file_patterns: Option<&[String]>,
) -> Vec<(usize, usize)> {
    let mut exclusion_zones = Vec::new();

    // Process patterns from config using the pre-built AC automaton
    if let Some(ac) = config.do_not_back_populate_ac() {
        for mat in ac.find_iter(line) {
            exclusion_zones.push((mat.start(), mat.end()));
        }
    }

    // Process file-specific patterns if they exist
    if let Some(patterns) = file_patterns {
        // Build AC automaton for file patterns
        if !patterns.is_empty() {
            if let Ok(file_ac) = AhoCorasickBuilder::new()
                .ascii_case_insensitive(true)
                .match_kind(MatchKind::LeftmostLongest)
                .build(patterns)
            {
                for mat in file_ac.find_iter(line) {
                    exclusion_zones.push((mat.start(), mat.end()));
                }
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


fn process_line(
    line_idx: usize,
    line: &str,
    file_path: &Path,
    ac: &aho_corasick::AhoCorasick,
    sorted_wikilinks: &[&CompiledWikilink],
    config: &ValidatedConfig,
    markdown_file_info: &MarkdownFileInfo,
) -> Result<Vec<BackPopulateMatch>, Box<dyn Error + Send + Sync>> {
    let mut matches = Vec::new();

    let exclusion_zones = collect_exclusion_zones(
        line,
        config,
        markdown_file_info.do_not_back_populate.as_deref(),
    );

    for mat in ac.find_iter(line) {
        // use the ac pattern - which returns the index that matches
        // the index of how the ac was built from sorted_wikilinks in the first place
        // so now we can extract the specific wikilink we need
        let wikilink = sorted_wikilinks[mat.pattern()];
        let starts_at = mat.start();
        let ends_at = mat.end();

        // Skip if in exclusion zone
        if range_overlaps(&exclusion_zones, starts_at, ends_at) {
            continue;
        }

        let matched_text = &line[starts_at..ends_at];

        // Add word boundary check
        if !is_word_boundary(line, starts_at, ends_at) {
            continue;
        }

        // Rest of the validation
        if should_create_match(line, starts_at, matched_text, file_path, markdown_file_info) {
            let mut replacement = if wikilink.wikilink.is_alias {
                format!("[[{}|{}]]", wikilink.wikilink.target, matched_text)
            } else if matched_text != wikilink.wikilink.target {
                format!("[[{}|{}]]", wikilink.wikilink.target, matched_text)
            } else {
                format!("[[{}]]", wikilink.wikilink.target)
            };

            let in_markdown_table = is_in_markdown_table(&line, &matched_text);
            if in_markdown_table {
                replacement = replacement.replace('|', r"\|");
            }

            matches.push(BackPopulateMatch {
                file_path: format_relative_path(file_path, config.obsidian_path()),
                line_number: line_idx + 1,
                line_text: line.to_string(),
                found_text: matched_text.to_string(),
                replacement,
                position: starts_at,
                in_markdown_table,
            });
        }
    }

    Ok(matches)
}



fn find_next_match(
    wikilink: &CompiledWikilink,
    line: &str,
    start_pos: usize,
    line_idx: usize,
    file_path: &Path,
    config: &ValidatedConfig,
    markdown_file_info: &MarkdownFileInfo,
) -> Result<Option<BackPopulateMatch>, Box<dyn Error + Send + Sync>> {
    if let Some(mat) = wikilink.regex.find(&line[start_pos..]) {
        let absolute_start = start_pos + mat.start();
        let absolute_end = start_pos + mat.end();
        let matched_text = &line[absolute_start..absolute_end];

        if should_create_match(
            line,
            absolute_start,
            matched_text,
            file_path,
            markdown_file_info,
        ) {
            let mut replacement = if wikilink.wikilink.is_alias {
                // For alias matches, preserve the case of the found text in the alias portion
                format!("[[{}|{}]]", wikilink.wikilink.target, matched_text)
            } else if matched_text != wikilink.wikilink.target {
                // For non-alias matches where case differs, create new alias
                format!("[[{}|{}]]", wikilink.wikilink.target, matched_text)
            } else {
                // Case matches exactly, use simple wikilink
                format!("[[{}]]", wikilink.wikilink.target)
            };

            let in_markdown_table = is_in_markdown_table(&line, &matched_text);
            if in_markdown_table {
                replacement = replacement.replace('|', r"\|");
            }

            let match_info = BackPopulateMatch {
                file_path: format_relative_path(file_path, config.obsidian_path()),
                line_number: line_idx + 1,
                line_text: line.to_string(),
                found_text: matched_text.to_string(),
                replacement,
                position: absolute_start,
                in_markdown_table,
            };

            return Ok(Some(match_info));
        }
    }
    Ok(None)
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
        let content_end = mat.end() - 2; // End of link content, before "]]"

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

#[derive(Debug)]
struct ConsolidatedMatch {
    line_number: usize,
    line_text: String,
    found_text: String,
    positions: Vec<usize>,
    replacement: String,
    in_markdown_table: bool,
}

fn consolidate_matches(matches: &[&BackPopulateMatch]) -> Vec<ConsolidatedMatch> {
    let mut consolidated: HashMap<(usize, String), ConsolidatedMatch> = HashMap::new();

    for match_info in matches {
        let key = (match_info.line_number, match_info.found_text.clone());

        if let Some(existing) = consolidated.get_mut(&key) {
            existing.positions.push(match_info.position);
        } else {
            consolidated.insert(
                key,
                ConsolidatedMatch {
                    line_number: match_info.line_number,
                    line_text: match_info.line_text.clone(),
                    found_text: match_info.found_text.clone(),
                    positions: vec![match_info.position],
                    replacement: match_info.replacement.clone(),
                    in_markdown_table: match_info.in_markdown_table,
                },
            );
        }
    }

    let mut result: Vec<ConsolidatedMatch> = consolidated.into_values().collect();
    result.sort_by_key(|m| (m.line_number, m.positions[0]));
    result
}

fn write_back_populate_table(
    writer: &ThreadSafeWriter,
    matches: &[BackPopulateMatch],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Group and sort matches by file path
    let mut matches_by_file: BTreeMap<String, Vec<&BackPopulateMatch>> = BTreeMap::new();
    for m in matches {
        let file_key = m.file_path.trim_end_matches(".md").to_string(); // Remove `.md`
        matches_by_file.entry(file_key).or_default().push(m);
    }

    writer.writeln(
        "",
        &format!(
            "found {} matches to back populate in {} files",
            matches.len(),
            matches_by_file.len()
        ),
    )?;

    for (file_path, file_matches) in &matches_by_file {
        // Write the file name as a header with change count
        writer.writeln(
            "",
            &format!("### [[{}]] - {}", file_path, file_matches.len()),
        )?;

        // Headers for each table
        let headers = &[
            "line",
            "current text",
            "found text",
            "occurrences",
            "will replace with",
            "escaped replacement",
        ];

        // Consolidate matches for this file
        let consolidated_matches = consolidate_matches(file_matches);

        // Collect rows for this file
        let rows: Vec<Vec<String>> = consolidated_matches
            .iter()
            .map(|m| {
                let replacement = if m.in_markdown_table {
                    m.replacement.clone()
                } else {
                    escape_pipe(&m.replacement)
                };

                // Create a copy of the line text and highlight all instances of found_text
                let highlighted_line = highlight_matches(&m.line_text, &m.found_text);

                vec![
                    m.line_number.to_string(),
                    escape_pipe(&highlighted_line),
                    m.found_text.clone(),
                    m.positions.len().to_string(),
                    replacement.clone(),
                    escape_brackets(&replacement),
                ]
            })
            .collect();

        writer.write_markdown_table(
            headers,
            &rows,
            Some(&[
                ColumnAlignment::Right,
                ColumnAlignment::Left,
                ColumnAlignment::Left,
                ColumnAlignment::Center,
                ColumnAlignment::Left,
                ColumnAlignment::Left,
            ]),
        )?;
    }

    Ok(())
}

// Helper function to highlight all instances of a pattern in text
fn highlight_matches(text: &str, pattern: &str) -> String {
    let mut result = String::new();
    let mut last_end = 0;

    // Create a case-insensitive regex pattern for the search
    let pattern_regex = match regex::Regex::new(&format!(r"(?i){}", regex::escape(pattern))) {
        Ok(regex) => regex,
        Err(e) => {
            eprintln!("Failed to create regex pattern: {}", e);
            return text.to_string(); // Return original text on regex creation failure
        }
    };

    // Iterate over all non-overlapping matches of the pattern in the text
    for mat in pattern_regex.find_iter(text) {
        let start = mat.start();
        let end = mat.end();

        // Check if the match starts within a wikilink
        if is_within_wikilink(text, start) {
            // If within a wikilink, skip highlighting and include the text as-is
            result.push_str(&text[last_end..end]);
        } else {
            // If not within a wikilink, highlight the match
            result.push_str(&text[last_end..start]); // Add text before the match
            result.push_str(&format!(
                "<span style=\"color: red;\">{}</span>",
                &text[start..end]
            ));
        }

        // Update the last_end to the end of the current match
        last_end = end;
    }

    // Append any remaining text after the last match
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

// Helper function to escape pipes and brackets for visual verification in `escaped replacement`
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

    let mut matches_by_file: BTreeMap<String, Vec<&BackPopulateMatch>> = BTreeMap::new();
    for match_info in matches {
        matches_by_file
            .entry(match_info.file_path.clone())
            .or_default()
            .push(match_info);
    }

    for (file_path, file_matches) in matches_by_file {
        let full_path = config.obsidian_path().join(&file_path);
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

            // Debug output: print line before replacement
            println!(
                "Processing file '{}', line {}. Original line content: '{}'",
                file_path, current_line_num, line
            );

            // Apply matches in reverse order if there are any
            let mut updated_line = line.to_string();
            if !line_matches.is_empty() {
                updated_line = apply_line_replacements(line, &line_matches, &file_path);
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
    file_path: &str,
) -> String {
    let mut updated_line = line.to_string();

    // Sort matches in descending order by `position`
    let mut sorted_matches = line_matches.to_vec();
    sorted_matches.sort_by_key(|m| std::cmp::Reverse(m.position));

    // Debug: Display sorted matches to confirm reverse processing order
    // println!("Matches to process for file '{}', line, in reverse order:", file_path);
    // for match_info in &sorted_matches {
    //     println!(
    //         "  Position: {}, Found text: '{}', Replacement: '{}'",
    //         match_info.position, match_info.found_text, match_info.replacement
    //     );
    // }

    // Apply replacements in sorted (reverse) order
    for match_info in sorted_matches {
        // println!(
        //     "\nProcessing match at position {} for '{}': replacing with '{}'",
        //     match_info.position, match_info.found_text, match_info.replacement
        // );

        let start = match_info.position;
        let end = start + match_info.found_text.len();

        // Check for UTF-8 boundary issues
        if !updated_line.is_char_boundary(start) || !updated_line.is_char_boundary(end) {
            eprintln!(
                "Error: Invalid UTF-8 boundary in file '{}', line {}.\n\
                Match position: {} to {}.\nLine content:\n{}\nFound text: '{}'\n",
                file_path, match_info.line_number, start, end, updated_line, match_info.found_text
            );
            panic!("Invalid UTF-8 boundary detected. Check positions and text encoding.");
        }

        // Debug information about slices involved in replacement
        //let before_slice = &updated_line[..start];
        //let after_slice = &updated_line[end..];
        // println!("Before match slice: '{}'", before_slice);
        // println!("Match slice: '{}'", &updated_line[start..end]);
        // println!("After match slice: '{}'", after_slice);

        // Perform the replacement
        updated_line.replace_range(start..end, &match_info.replacement);

        // Debug: Show line content after replacement
        // println!("Updated line after replacement: '{}'", updated_line);

        // Validation check after each replacement
        if updated_line.contains("[[[") || updated_line.contains("]]]") {
            eprintln!(
                "\nWarning: Potential nested pattern detected after replacement in file '{}', line {}.\n\
                Current line:\n{}\n",
                file_path, match_info.line_number, updated_line
            );
        }
    }

    // Final debug statement to show the fully updated line after all replacements
    // println!("Final updated line: '{}'", updated_line);

    updated_line
}

fn format_relative_path(path: &Path, base_path: &Path) -> String {
    path.strip_prefix(base_path)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontmatter;
    use crate::scan::MarkdownFileInfo;
    use crate::wikilink::{compile_wikilink, CompiledWikilink, Wikilink};
    use regex;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_environment() -> (TempDir, ValidatedConfig, ObsidianRepositoryInfo) {
        let temp_dir = TempDir::new().unwrap();

        let config = ValidatedConfig::new(
            false,                          // apply_changes
            None,                           // back_populate_file_count
            None,                           // do_not_back_populate
            None,                           // ignore_folders
            temp_dir.path().to_path_buf(),  // obsidian_path
            temp_dir.path().join("output"), // output_folder
        );

        let mut repo_info = ObsidianRepositoryInfo::default();

        // Create a wikilink for testing
        let wikilink = Wikilink {
            display_text: "Test Link".to_string(),
            target: "Test Link".to_string(),
            is_alias: false,
        };

        let compiled =
            CompiledWikilink::new(regex::Regex::new(r"(?i)\bTest Link\b").unwrap(), wikilink);

        repo_info.wikilinks_sorted.insert(compiled);
        repo_info.markdown_files = HashMap::new();

        (temp_dir, config, repo_info)
    }

    fn create_test_config(
        temp_dir: &TempDir,
        apply_changes: bool,
        do_not_back_populate: Option<Vec<String>>,
    ) -> ValidatedConfig {
        ValidatedConfig::new(
            apply_changes,
            None,                           // back_populate_file_count
            do_not_back_populate,           // do_not_back_populate
            None,                           // ignore_folders
            temp_dir.path().to_path_buf(),  // obsidian_path
            temp_dir.path().join("output"), // output_folder
        )
    }

    // Helper struct for test cases
    struct TestCase {
        content: &'static str,
        wikilink: Wikilink,
        expected_matches: Vec<(&'static str, &'static str)>,
        description: &'static str,
    }

    #[test]
    fn test_case_sensitivity_behavior() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        let config = ValidatedConfig::new(
            false,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        // Test cases combining previous separate tests
        let test_cases = vec![
            // Basic matching (non-table context)
            TestCase {
                content: "test link TEST LINK Test Link",
                wikilink: Wikilink {
                    display_text: "Test Link".to_string(),
                    target: "Test Link".to_string(),
                    is_alias: false,
                },
                expected_matches: vec![
                    ("test link", "[[Test Link|test link]]"),
                    ("TEST LINK", "[[Test Link|TEST LINK]]"),
                    ("Test Link", "[[Test Link]]"),
                ],
                description: "Basic case-insensitive matching",
            },
            // Alias preservation
            TestCase {
                content: "josh likes apples",
                wikilink: Wikilink {
                    display_text: "josh".to_string(),
                    target: "Joshua Strayhorn".to_string(),
                    is_alias: true,
                },
                expected_matches: vec![("josh", "[[Joshua Strayhorn|josh]]")],
                description: "Alias case preservation",
            },
            // Table context - notice we expect normal wikilink format which will be escaped later
            TestCase {
                content: "| Test Link | Another test link |",
                wikilink: Wikilink {
                    display_text: "test link".to_string(),
                    target: "Test Link".to_string(),
                    is_alias: false,
                },
                expected_matches: vec![
                    ("Test Link", "[[Test Link]]"), // This will get pipes escaped after
                    ("test link", "[[Test Link|test link]]"), // This will get pipes escaped after
                ],
                description: "Case handling in tables",
            },
        ];

        for case in test_cases {
            let mut file = File::create(&file_path).unwrap();
            write!(file, "{}", case.content).unwrap();

            let compiled = compile_wikilink(case.wikilink);
            let markdown_info = MarkdownFileInfo::new();

            let mut matches = Vec::new();
            process_line(
                0,
                case.content,
                &file_path,
                &[&compiled],
                &config,
                &mut matches,
                &markdown_info,
            )
            .unwrap();

            assert_eq!(
                matches.len(),
                case.expected_matches.len(),
                "Wrong number of matches for case: {}",
                case.description
            );

            for ((expected_text, expected_base_replacement), actual_match) in
                case.expected_matches.iter().zip(matches.iter())
            {
                assert_eq!(
                    actual_match.found_text, *expected_text,
                    "Wrong matched text for case: {}",
                    case.description
                );

                // For table matches, we expect the pipes to be escaped
                let expected_replacement = if actual_match.in_markdown_table {
                    expected_base_replacement.replace('|', r"\|")
                } else {
                    expected_base_replacement.to_string()
                };

                assert_eq!(
                    actual_match.replacement,
                    expected_replacement,
                    "Wrong replacement for case: {}\nExpected: {}\nActual: {}\nIn table: {}",
                    case.description,
                    expected_replacement,
                    actual_match.replacement,
                    actual_match.in_markdown_table
                );
            }
        }
    }

    #[test]
    fn test_is_within_wikilink_improved() {
        // Basic tests
        assert!(!is_within_wikilink("Test Link here", 0));
        assert!(is_within_wikilink("[[Test Link]]", 2));
        assert!(!is_within_wikilink("[[Other]] Test Link", 9));

        // Complex cases with multiple wikilinks
        let complex_line = "[[India]]?]] - set up a weeknight that [[Oleksiy Blavat|Oleksiy]] [[Zach Bowman|Zach]]";

        // Debug prints to understand positions
        // println!("Testing position within wikilink:");
        // println!("Line: {}", complex_line);
        // println!("Position 43 character: {}", &complex_line[43..44]);

        // Check various positions in complex_line
        assert!(is_within_wikilink(complex_line, 43)); // Position within "Oleksiy Blavat|Oleksiy"
        assert!(!is_within_wikilink(complex_line, 35)); // Position in "that "
        assert!(is_within_wikilink(complex_line, 2)); // Position within "India"

        // Test aliased wikilinks
        assert!(is_within_wikilink("[[Person|Name]]", 9)); // Within alias part
        assert!(is_within_wikilink("[[Person|Name]]", 3)); // Within target part

        // Test edge cases
        assert!(!is_within_wikilink("[single brackets]", 5));
        assert!(!is_within_wikilink("no brackets", 3));
        assert!(!is_within_wikilink("[[unclosed", 3));
        assert!(!is_within_wikilink("text]] [[", 2));
    }

    #[test]
    fn test_find_matches_with_existing_wikilinks() {
        let (temp_dir, config, mut repo_info) = create_test_environment();

        // Create a test file with mixed content
        let file_content =
            "[[Some Link]] and Test Link in same line\nTest Link [[Other Link]] Test Link mixed";

        let file_path = temp_dir.path().join("test.md");
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", file_content).unwrap();

        repo_info
            .markdown_files
            .insert(file_path, MarkdownFileInfo::new());

        // Convert HashSet to sorted Vec
        let sorted_wikilinks: Vec<&CompiledWikilink> = repo_info.wikilinks_sorted.iter().collect();

        let matches =
            find_all_back_populate_matches(&config, &repo_info.markdown_files, &sorted_wikilinks)
                .unwrap();

        assert_eq!(matches.len(), 3, "Should find three 'Test Link' instances");

        // First line has one match
        assert_eq!(
            matches.iter().filter(|m| m.line_number == 1).count(),
            1,
            "Should find one 'Test Link' on the first line"
        );

        // Second line has two matches
        assert_eq!(
            matches.iter().filter(|m| m.line_number == 2).count(),
            2,
            "Should find two 'Test Link' instances on the second line"
        );
    }

    #[test]
    fn test_find_matches_with_existing_wikilinks_debug() {
        let (temp_dir, config, mut repo_info) = create_test_environment();

        // Create a test file with mixed content
        let file_content =
            "[[Some Link]] and Test Link in same line\nTest Link [[Other Link]] Test Link mixed";
        let file_path = temp_dir.path().join("test.md");
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", file_content).unwrap();

        repo_info
            .markdown_files
            .insert(file_path, MarkdownFileInfo::new());

        // Convert HashSet to sorted Vec
        let sorted_wikilinks: Vec<&CompiledWikilink> = repo_info.wikilinks_sorted.iter().collect();

        let matches =
            find_all_back_populate_matches(&config, &repo_info.markdown_files, &sorted_wikilinks)
                .unwrap();

        assert_eq!(matches.len(), 3, "Should find three 'Test Link' instances");

        // First line has one match
        let first_line_matches: Vec<_> = matches.iter().filter(|m| m.line_number == 1).collect();

        assert_eq!(
            first_line_matches.len(),
            1,
            "Should find one 'Test Link' on the first line, found: {:?}",
            first_line_matches
                .iter()
                .map(|m| &m.found_text)
                .collect::<Vec<_>>()
        );
        // Second line has two matches
        let second_line_matches: Vec<_> = matches.iter().filter(|m| m.line_number == 2).collect();
        assert_eq!(
            second_line_matches.len(),
            2,
            "Should find two 'Test Link' instances on the second line, found: {:?}",
            second_line_matches
                .iter()
                .map(|m| &m.found_text)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_apply_changes() {
        let (temp_dir, config, mut repo_info) = create_test_environment();

        // Create a test file
        let file_path = temp_dir.path().join("test.md");
        let content = "Here is Test Link\nNo change here\nAnother Test Link";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        repo_info
            .markdown_files
            .insert(file_path.clone(), MarkdownFileInfo::new());

        // Convert HashSet to sorted Vec
        let sorted_wikilinks: Vec<&CompiledWikilink> = repo_info.wikilinks_sorted.iter().collect();

        // Find matches
        let matches =
            find_all_back_populate_matches(&config, &repo_info.markdown_files, &sorted_wikilinks)
                .unwrap();

        // Create a config that allows changes
        let config_with_changes = ValidatedConfig::new(
            true,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        // Apply changes
        apply_back_populate_changes(&config_with_changes, &matches).unwrap();

        // Verify changes
        let updated_content = fs::read_to_string(&file_path).unwrap();
        assert!(updated_content.contains("[[Test Link]]"));
        assert!(updated_content.contains("No change here"));
        assert_eq!(
            updated_content.matches("[[Test Link]]").count(),
            2,
            "Should have replaced both instances"
        );
    }

    #[test]
    fn test_overlapping_wikilink_matches() {
        let (temp_dir, config, mut repo_info) = create_test_environment();
        let file_path = temp_dir.path().join("test.md");

        // Create test content with potential overlapping matches
        let content = "[[Kyriana McCoy|Kyriana]] - Kyri and [[Kalina McCoy|Kali]]";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        repo_info
            .markdown_files
            .insert(file_path.clone(), MarkdownFileInfo::new());

        // Add the overlapping wikilinks to repo_info
        let wikilink1 = Wikilink {
            display_text: "Kyri".to_string(),
            target: "Kyri".to_string(),
            is_alias: false,
        };
        let wikilink2 = Wikilink {
            display_text: "Kyri".to_string(),
            target: "Kyriana McCoy".to_string(),
            is_alias: true,
        };

        let compiled1 =
            CompiledWikilink::new(regex::Regex::new(r"(?i)\bKyri\b").unwrap(), wikilink1);
        let compiled2 =
            CompiledWikilink::new(regex::Regex::new(r"(?i)\bKyri\b").unwrap(), wikilink2);

        repo_info.wikilinks_sorted.insert(compiled1);
        repo_info.wikilinks_sorted.insert(compiled2);

        // Convert HashSet to sorted Vec
        let sorted_wikilinks: Vec<&CompiledWikilink> = repo_info.wikilinks_sorted.iter().collect();

        let matches =
            find_all_back_populate_matches(&config, &repo_info.markdown_files, &sorted_wikilinks)
                .unwrap();

        // We should only get one match for "Kyri" at position 28
        assert_eq!(matches.len(), 1, "Expected exactly one match");
        assert_eq!(matches[0].position, 28, "Expected match at position 28");
    }

    #[test]
    fn test_process_line_with_mozzarella_exclusion() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let wikilink = Wikilink {
            display_text: "cheese".to_string(),
            target: "fromage".to_string(),
            is_alias: true,
        };

        let compiled =
            CompiledWikilink::new(regex::Regex::new(r"(?i)\bcheese\b").unwrap(), wikilink);

        let compiled_ref = &compiled;
        let sorted_wikilinks = &[compiled_ref][..];
        let mut matches = Vec::new();

        let config = create_test_config(
            &temp_dir,
            false,
            Some(vec!["[[mozzarella]] cheese".to_string()]),
        );

        let markdown_info = MarkdownFileInfo::new();

        // Test line with excluded pattern
        let line = "- 1 1/2 cup [[mozzarella]] cheese shredded";
        process_line(
            0,
            line,
            &file_path,
            sorted_wikilinks,
            &config,
            &mut matches,
            &markdown_info,
        )
        .unwrap();

        assert_eq!(matches.len(), 0, "Match should be excluded");

        // Test that other cheese references still match
        let line = "I love cheese on my pizza";
        matches.clear();
        process_line(
            0,
            line,
            &file_path,
            sorted_wikilinks,
            &config,
            &mut matches,
            &markdown_info,
        )
        .unwrap();

        assert_eq!(matches.len(), 1, "Match should be included");
        assert_eq!(matches[0].found_text, "cheese");
    }

    #[test]
    fn test_no_self_referential_back_population() {
        let (temp_dir, config, mut repo_info) = create_test_environment();

        // Create a wikilink for testing that includes an alias
        let wikilink = Wikilink {
            display_text: "Will".to_string(),
            target: "William.md".to_string(),
            is_alias: true,
        };

        let compiled = CompiledWikilink::new(regex::Regex::new(r"(?i)\bWill\b").unwrap(), wikilink);

        repo_info.wikilinks_sorted.clear();
        repo_info.wikilinks_sorted.insert(compiled);

        // Create a test file with its own name
        let content = "Will is mentioned here but should not be replaced";
        let file_path = temp_dir.path().join("Will.md");
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        repo_info
            .markdown_files
            .insert(file_path, MarkdownFileInfo::new());

        let sorted_wikilinks: Vec<&CompiledWikilink> = repo_info.wikilinks_sorted.iter().collect();
        let matches =
            find_all_back_populate_matches(&config, &repo_info.markdown_files, &sorted_wikilinks)
                .unwrap();

        assert_eq!(
            matches.len(),
            0,
            "Should not find matches on page's own name"
        );

        // Test with different file using same text
        let other_file_path = temp_dir.path().join("Other.md");
        let mut other_file = File::create(&other_file_path).unwrap();
        write!(other_file, "{}", content).unwrap();

        repo_info
            .markdown_files
            .insert(other_file_path, MarkdownFileInfo::new());

        let matches =
            find_all_back_populate_matches(&config, &repo_info.markdown_files, &sorted_wikilinks)
                .unwrap();

        assert_eq!(matches.len(), 1, "Should find match on other pages");
    }

    #[test]
    fn test_should_create_match_in_table() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let markdown_info = MarkdownFileInfo::new();

        // Test simple table cell match
        assert!(should_create_match(
            "| Test Link | description |",
            2,
            "Test Link",
            &file_path,
            &markdown_info,
        ));

        // Test match in table with existing wikilinks
        assert!(should_create_match(
            "| Test Link | [[Other]] |",
            2,
            "Test Link",
            &file_path,
            &markdown_info,
        ));
    }

    #[test]
    fn test_back_populate_table_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create test content with table
        let content = "# Test Table\n|Name|Description|\n|---|---|\n|Test Link|Sample text|\n";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        let matches = vec![BackPopulateMatch {
            file_path: "test.md".into(),
            line_number: 4,
            line_text: "|Test Link|Sample text|".into(),
            found_text: "Test Link".into(),
            replacement: "[[Test Link\\|Another Name]]".into(),
            position: 1,
            in_markdown_table: true,
        }];

        let config = ValidatedConfig::new(
            true,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        apply_back_populate_changes(&config, &matches).unwrap();

        let updated_content = fs::read_to_string(&file_path).unwrap();
        assert!(updated_content.contains("[[Test Link\\|Another Name]]|Sample text"));
    }

    #[test]
    fn test_back_populate_mixed_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create test content with both table and regular text
        let content = "# Mixed Content\n\
            Regular Test Link here\n\
            |Name|Description|\n\
            |---|---|\n\
            |Test Link|Sample|\n\
            More Test Link text";

        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        let matches = vec![
            BackPopulateMatch {
                file_path: "test.md".into(),
                line_number: 2,
                line_text: "Regular Test Link here".into(),
                found_text: "Test Link".into(),
                replacement: "[[Test Link]]".into(),
                position: 8,
                in_markdown_table: false,
            },
            BackPopulateMatch {
                file_path: "test.md".into(),
                line_number: 5,
                line_text: "|Test Link|Sample|".into(),
                found_text: "Test Link".into(),
                replacement: "[[Test Link\\|Display]]".into(),
                position: 1,
                in_markdown_table: true,
            },
        ];

        let config = ValidatedConfig::new(
            true,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        apply_back_populate_changes(&config, &matches).unwrap();

        let updated_content = fs::read_to_string(&file_path).unwrap();
        assert!(updated_content.contains("Regular [[Test Link]] here"));
        assert!(updated_content.contains("|[[Test Link\\|Display]]|Sample|"));
    }

    #[test]
    fn test_is_within_wikilink_byte_offsets() {
        // // Debug print function
        // fn print_char_positions(text: &str) {
        //     println!("\nAnalyzing text: {}", text);
        //     println!("Total bytes: {}", text.len());
        //     for (i, (byte_pos, ch)) in text.char_indices().enumerate() {
        //         println!("char '{}' at index {}: byte position {}", ch, i, byte_pos);
        //     }
        //
        //     // Find wikilink positions
        //     let wikilink_regex = regex::Regex::new(r"\[\[.*?]]").unwrap();
        //     if let Some(mat) = wikilink_regex.find(text) {
        //         println!("\nWikilink match: start={}, end={}", mat.start(), mat.end());
        //         println!("Matched text: '{}'", &text[mat.start()..mat.end()]);
        //         println!("Link content starts at: {}", mat.start() + 2);
        //         println!("Link content ends at: {}", mat.end() - 2);
        //     }
        // }

        let ascii_text = "before [[link]] after";
        let utf8_text = "привет [[ссылка]] текст";

        // print_char_positions(ascii_text);
        // print_char_positions(utf8_text);

        let cases = vec![
            // ASCII cases - fixed expectations
            (ascii_text, 7, false),  // First [ - should be FALSE (it's markup)
            (ascii_text, 8, false),  // Second [ - should be FALSE (it's markup)
            (ascii_text, 9, true),   // 'l' - should be TRUE (it's content)
            (ascii_text, 10, true),  // 'i' - should be TRUE (it's content)
            (ascii_text, 11, true),  // 'n' - should be TRUE (it's content)
            (ascii_text, 12, true),  // 'k' - should be TRUE (it's content)
            (ascii_text, 13, false), // First ] - should be FALSE (it's markup)
            (ascii_text, 14, false), // Second ] - should be FALSE (it's markup)
            // UTF-8 cases - fixed expectations
            (utf8_text, 13, false), // First [ - should be FALSE (it's markup)
            (utf8_text, 14, false), // Second [ - should be FALSE (it's markup)
            (utf8_text, 15, true),  // Inside link text (с) - should be TRUE (it's content)
            (utf8_text, 25, true),  // Inside link text (a) - should be TRUE (it's content)
            (utf8_text, 27, false), // First ] - should be FALSE (it's markup)
            (utf8_text, 28, false), // Second ] - should be FALSE (it's markup)
            (utf8_text, 12, false), // Space before [[ - should be FALSE (outside)
            (utf8_text, 29, false), // Space after ]] - should be FALSE (outside)
        ];

        for (text, pos, expected) in cases {
            let actual = is_within_wikilink(text, pos);
            assert_eq!(
                actual,
                expected,
                "Failed for text '{}' at position {} (char '{}')\nExpected: {}, Got: {}",
                text,
                pos,
                text.chars().nth(text[..pos].chars().count()).unwrap_or('?'),
                expected,
                actual
            );
        }
    }

    #[test]
    fn test_no_matches_for_frontmatter_aliases() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create test content with frontmatter containing aliases
        let content = r#"---
aliases:
  - "Johnny"
  - "John D"
---
# John Doe
Mentions of Johnny and John D should not be replaced.
But other Johns should be replaced."#;

        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        let wikilink = Wikilink {
            display_text: "Johnny".to_string(),
            target: "John Doe".to_string(),
            is_alias: true,
        };
        let compiled = compile_wikilink(wikilink);

        let mut markdown_info = MarkdownFileInfo::new();
        markdown_info.frontmatter = frontmatter::deserialize_frontmatter(content).ok();

        let config = ValidatedConfig::new(
            false,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        let match_result = find_next_match(
            &compiled,
            "Mentions of Johnny and John D",
            0,
            0,
            &file_path,
            &config,
            &markdown_info,
        )
        .unwrap();

        assert!(
            match_result.is_none(),
            "Should not match text that appears in frontmatter aliases"
        );
    }

    #[test]
    fn test_collect_exclusion_zones() {
        let line = "Test phrase and Another Phrase with EXTERNAL [link](https://test.com)";

        let config_patterns = Some(vec!["test phrase".to_string()]);
        let file_patterns = Some(vec!["another phrase".to_string()]);

        let zones =
            collect_exclusion_zones(line, config_patterns.as_deref(), file_patterns.as_deref());

        assert_eq!(zones.len(), 3); // Two patterns + external link

        // Check first zone contains "Test phrase"
        assert!(line[zones[0].0..zones[0].1]
            .to_lowercase()
            .contains("test phrase"));

        // Check second zone contains "Another Phrase"
        assert!(line[zones[1].0..zones[1].1]
            .to_lowercase()
            .contains("another phrase"));

        // Check last zone contains the external link
        assert!(line[zones[2].0..zones[2].1].contains("[link](https://test.com)"));
    }

    #[test]
    fn test_mixed_case_exclusions() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let wikilink = Wikilink {
            display_text: "test".to_string(),
            target: "target".to_string(),
            is_alias: true,
        };
        let compiled = compile_wikilink(wikilink);

        let mut markdown_info = MarkdownFileInfo::new();
        markdown_info.do_not_back_populate = Some(vec!["Test Phrase".to_string()]);

        let test_cases = vec![
            "TEST PHRASE here",
            "test phrase there",
            "Test Phrase somewhere",
        ];

        for line in test_cases {
            let mut matches = Vec::new();
            process_line(
                0,
                line,
                &file_path,
                &[&compiled],
                &ValidatedConfig::new(
                    false,
                    None,
                    None,
                    None,
                    file_path.clone(),
                    file_path.clone(),
                ),
                &mut matches,
                &markdown_info,
            )
            .unwrap();

            assert!(
                matches.is_empty(),
                "Should exclude matches regardless of case: {}",
                line
            );
        }
    }

    #[test]
    fn test_multi_word_pattern_exclusions() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let wikilink = Wikilink {
            display_text: "hill".to_string(),
            target: "target".to_string(),
            is_alias: true,
        };
        let compiled = compile_wikilink(wikilink);

        let mut markdown_info = MarkdownFileInfo::new();
        markdown_info.do_not_back_populate = Some(vec!["Federal Hill Baltimore".to_string()]);

        let mut matches = Vec::new();
        process_line(
            0,
            "Federal Hill Baltimore is nice",
            &file_path,
            &[&compiled],
            &ValidatedConfig::new(
                false,
                None,
                None,
                None,
                file_path.clone(),
                file_path.clone(),
            ),
            &mut matches,
            &markdown_info,
        )
        .unwrap();

        assert!(
            matches.is_empty(),
            "Should not match 'hill' within multi-word pattern"
        );
    }

    #[test]
    fn test_overlapping_exclusions() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let wikilink = Wikilink {
            display_text: "test".to_string(),
            target: "target".to_string(),
            is_alias: true,
        };
        let compiled = compile_wikilink(wikilink);

        let mut markdown_info = MarkdownFileInfo::new();
        markdown_info.do_not_back_populate = Some(vec!["file test".to_string()]);

        let config = ValidatedConfig::new(
            false,
            None,
            Some(vec!["test pattern".to_string()]),
            None,
            file_path.clone(),
            file_path.clone(),
        );

        let mut matches = Vec::new();
        process_line(
            0,
            "file test pattern here",
            &file_path,
            &[&compiled],
            &config,
            &mut matches,
            &markdown_info,
        )
        .unwrap();

        assert!(
            matches.is_empty(),
            "Should exclude matches in overlapping patterns"
        );
    }

    #[test]
    fn test_process_line_table_escaping() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create a test wikilink
        let wikilink = Wikilink {
            display_text: "Test Link".to_string(),
            target: "Target Page".to_string(),
            is_alias: true,
        };
        let compiled = compile_wikilink(wikilink);

        let config = ValidatedConfig::new(
            false,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        let markdown_info = MarkdownFileInfo::new();

        // Test cases with different table formats and positions
        let test_cases = vec![
            (
                "| Test Link | description |",
                "[[Target Page\\|Test Link]]",
                true,
                "Basic table cell",
            ),
            (
                "| prefix Test Link suffix | description |",
                "[[Target Page\\|Test Link]]",
                true,
                "Table cell with surrounding text",
            ),
            (
                "| column1 | Test Link |",
                "[[Target Page\\|Test Link]]",
                true,
                "Different column position",
            ),
            (
                "Test Link outside table",
                "[[Target Page|Test Link]]",
                false,
                "Outside table (no escaping)",
            ),
            (
                "| complex | Test Link | with | many | pipes |",
                "[[Target Page\\|Test Link]]",
                true,
                "Complex table with many columns",
            ),
        ];

        for (line, expected_replacement, should_be_in_table, case_desc) in test_cases {
            let mut matches = Vec::new();

            process_line(
                0,
                line,
                &file_path,
                &[&compiled],
                &config,
                &mut matches,
                &markdown_info,
            )
            .unwrap();

            assert!(
                !matches.is_empty(),
                "Should find match for case: {}",
                case_desc
            );
            let match_info = &matches[0];

            assert_eq!(
                match_info.replacement,
                expected_replacement,
                "Replacement text should {} be escaped for case: {}",
                if should_be_in_table { "" } else { "not" },
                case_desc
            );

            assert_eq!(
                match_info.in_markdown_table, should_be_in_table,
                "in_markdown_table flag should be correct for case: {}",
                case_desc
            );
        }
    }

    #[test]
    fn test_process_line_complex_table_escaping() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create multiple test wikilinks that might appear in tables
        let wikilinks = vec![
            (
                Wikilink {
                    display_text: "Test Link".to_string(),
                    target: "Target Page".to_string(),
                    is_alias: true,
                },
                "Test Link",
            ),
            (
                Wikilink {
                    display_text: "another link".to_string(), // Note: lowercase in wikilink
                    target: "Other Page".to_string(),
                    is_alias: false,
                },
                "Another Link",
            ),
        ];

        let compiled_wikilinks: Vec<CompiledWikilink> = wikilinks
            .into_iter()
            .map(|(w, _)| compile_wikilink(w))
            .collect();

        let wikilink_refs: Vec<&CompiledWikilink> = compiled_wikilinks.iter().collect();

        let config = ValidatedConfig::new(
            false,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        let markdown_info = MarkdownFileInfo::new();

        // Test complex table scenarios
        let test_cases = vec![
            (
                "| Test Link | Another Link | description |",
                vec![
                    "[[Target Page\\|Test Link]]",
                    "[[Other Page\\|Another Link]]", // Case differs, so it creates an alias
                ],
                "Multiple matches in one row",
            ),
            (
                "| Item | Test Link \\| split cell | notes |",
                vec!["[[Target Page\\|Test Link]]"],
                "Table with escaped pipe in cell",
            ),
            (
                "| Test Link | description | [[Existing|Wiki]] |",
                vec!["[[Target Page\\|Test Link]]"],
                "Table with existing wikilink",
            ),
        ];

        for (line, expected_replacements, case_desc) in test_cases {
            let mut matches = Vec::new();

            process_line(
                0,
                line,
                &file_path,
                &wikilink_refs,
                &config,
                &mut matches,
                &markdown_info,
            )
            .unwrap();

            assert_eq!(
                matches.len(),
                expected_replacements.len(),
                "Should find correct number of matches for case: {}",
                case_desc
            );

            for (match_info, expected_replacement) in
                matches.iter().zip(expected_replacements.iter())
            {
                assert_eq!(
                    match_info.replacement, *expected_replacement,
                    "Replacement text should be correctly escaped for case: {}",
                    case_desc
                );
                assert!(
                    match_info.in_markdown_table,
                    "Should be marked as in table for case: {}",
                    case_desc
                );
            }
        }
    }

    #[test]
    fn test_file_processing_state() {
        let mut state = FileProcessingState::new();

        // Test initial state
        assert!(!state.should_skip_line());

        // Test frontmatter
        state.update_for_line("---");
        assert!(state.should_skip_line());
        state.update_for_line("title: test");
        assert!(state.should_skip_line());
        state.update_for_line("---");
        assert!(!state.should_skip_line());

        // Test code block
        state.update_for_line("```rust");
        assert!(state.should_skip_line());
        state.update_for_line("code line");
        assert!(state.should_skip_line());
        state.update_for_line("```");
        assert!(!state.should_skip_line());
    }

    #[test]
    fn test_file_processing_state_complex() {
        let mut state = FileProcessingState::new();

        // Test frontmatter followed by code block
        assert!(!state.should_skip_line(), "Initial state should not skip");

        state.update_for_line("---");
        assert!(state.should_skip_line(), "Should skip in frontmatter");
        state.update_for_line("title: Test");
        assert!(state.should_skip_line(), "Should skip frontmatter content");
        state.update_for_line("---");
        assert!(
            !state.should_skip_line(),
            "Should not skip after frontmatter"
        );

        state.update_for_line("```rust");
        assert!(state.should_skip_line(), "Should skip in code block");
        state.update_for_line("let x = 42;");
        assert!(state.should_skip_line(), "Should skip code block content");
        state.update_for_line("```");
        assert!(
            !state.should_skip_line(),
            "Should not skip after code block"
        );
    }

    #[test]
    fn test_frontmatter_with_code_blocks() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let content = r#"---
title: Test Document
description: A test file
tags: [test, code]
---
Test Link here
```rust
Test Link in code block
```
Test Link after code block"#;

        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        let wikilink = Wikilink {
            display_text: "Test Link".to_string(),
            target: "Target".to_string(),
            is_alias: true,
        };
        let compiled = compile_wikilink(wikilink);

        let config = ValidatedConfig::new(
            false,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        let markdown_info = MarkdownFileInfo::new();
        let matches = process_file(&file_path, &[&compiled], &config, &markdown_info).unwrap();

        assert_eq!(
            matches.len(),
            2,
            "Should only match Test Links outside frontmatter and code blocks"
        );

        let line_numbers: Vec<_> = matches.iter().map(|m| m.line_number).collect();
        assert!(
            line_numbers.contains(&6),
            "Should match Test Link after frontmatter (line 6)"
        );
        assert!(
            line_numbers.contains(&10),
            "Should match Test Link after code block (line 10)"
        );
    }

    #[test]
    fn test_aho_corasick_matches_regex() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create test cases with potential overlapping matches
        let wikilinks = vec![
            Wikilink {
                display_text: "test".to_string(),
                target: "Test Page".to_string(),
                is_alias: true,
            },
            Wikilink {
                display_text: "test link".to_string(),
                target: "Test Link Page".to_string(),
                is_alias: true,
            },
        ];

        let compiled_wikilinks: Vec<CompiledWikilink> =
            wikilinks.into_iter().map(|w| compile_wikilink(w)).collect();

        let wikilink_refs: Vec<&CompiledWikilink> = compiled_wikilinks.iter().collect();

        // Build Aho-Corasick automaton
        let patterns: Vec<&str> = wikilink_refs
            .iter()
            .map(|w| w.wikilink.display_text.as_str())
            .collect();

        let ac = AhoCorasickBuilder::new()
            .ascii_case_insensitive(true)
            .match_kind(MatchKind::LeftmostLongest)
            .build(patterns)
            .expect("Failed to build Aho-Corasick automaton");

        let config = ValidatedConfig::new(
            false,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        let markdown_info = MarkdownFileInfo::new();

        // Test cases
        let test_cases = vec![
            "Here is a test link example",
            "TEST LINK and test here",
            "Multiple test test link patterns",
            "No matches here",
        ];

        for (i, line) in test_cases.iter().enumerate() {
            // Get matches using both methods
            let mut regex_matches = Vec::new();
            process_line(
                i,
                line,
                &file_path,
                &wikilink_refs,
                &config,
                &mut regex_matches,
                &markdown_info,
            )
            .unwrap();

            let ac_matches = process_line(
                i,
                line,
                &file_path,
                &ac,
                &wikilink_refs,
                &config,
                &markdown_info,
            )
            .unwrap();

            // Compare results
            assert_eq!(
                regex_matches.len(),
                ac_matches.len(),
                "Different number of matches for line: {}",
                line
            );

            for (regex_match, ac_match) in regex_matches.iter().zip(ac_matches.iter()) {
                assert_eq!(
                    regex_match.position, ac_match.position,
                    "Different match positions for line: {}",
                    line
                );
                assert_eq!(
                    regex_match.found_text, ac_match.found_text,
                    "Different matched text for line: {}",
                    line
                );
                assert_eq!(
                    regex_match.replacement, ac_match.replacement,
                    "Different replacements for line: {}",
                    line
                );
            }
        }
    }

    #[test]
    fn test_escape_pipe_with_unicode() {
        assert_eq!(
            escape_pipe("[[santé|medical scheduling]]"),
            "[[santé\\|medical scheduling]]"
        );
    }

    #[test]
    fn test_escape_pipe_with_existing_escapes() {
        assert_eq!(escape_pipe("a\\|b"), "a\\|b");
        assert_eq!(escape_pipe("a\\\\|b"), "a\\\\\\|b");
    }

    #[test]
    fn test_escape_pipe_multiple() {
        assert_eq!(
            escape_pipe("col1|col2|col3"),
            "col1\\|col2\\|col3"
        );
    }

    #[test]
    fn test_escape_pipe_with_complex_unicode() {
        assert_eq!(
            escape_pipe("[[café|☕]]|[[thé|🫖]]"),
            "[[café\\|☕]]\\|[[thé\\|🫖]]"
        );
    }

    #[test]
    fn test_collect_exclusion_zones_with_ac() {
        // Test with config patterns
        let config_patterns = vec!["test phrase".to_string(), "another test".to_string()];
        let config = ValidatedConfig::new(
            false,
            None,
            Some(config_patterns),
            None,
            PathBuf::new(),
            PathBuf::new(),
        );

        let file_patterns = Some(vec!["specific pattern".to_string()]);
        let line = "This is a test phrase with another test and a specific pattern";

        let zones = collect_exclusion_zones(line, config.do_not_back_populate_ac(), file_patterns.as_deref());

        // Verify zones are collected correctly
        assert!(!zones.is_empty());

        // Verify case insensitivity
        let line_upper = "This is a TEST PHRASE with ANOTHER TEST";
        let zones_upper = collect_exclusion_zones(
            line_upper,
            config.do_not_back_populate_ac(),
            None,
        );
        assert!(!zones_upper.is_empty());

        // Test with markdown links
        let line_with_links = "Text with [markdown](link) and test phrase";
        let zones_with_links = collect_exclusion_zones(
            line_with_links,
            config.do_not_back_populate_ac(),
            None,
        );
        assert!(zones_with_links.len() >= 2); // Should have both markdown link and pattern zones
    }

    #[test]
    fn test_exclusion_zone_merging() {
        let config_patterns = vec!["test".to_string(), "test pattern".to_string()];
        let config = ValidatedConfig::new(
            false,
            None,
            Some(config_patterns),
            None,
            PathBuf::new(),
            PathBuf::new(),
        );

        let line = "test pattern here";
        let zones = collect_exclusion_zones(line, config.do_not_back_populate_ac(), None);

        // Should merge overlapping "test" and "test pattern" into single zone
        assert_eq!(zones.len(), 1);
        assert_eq!(zones[0], (0, 12)); // Should cover "test pattern"
    }
}

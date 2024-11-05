use crate::constants::*;
use crate::deterministic_file_search::DeterministicSearch;
use crate::scan::{MarkdownFileInfo, ObsidianRepositoryInfo};
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::validated_config::ValidatedConfig;
use crate::wikilink::MARKDOWN_REGEX;
use crate::wikilink_types::{InvalidWikilink, ToWikilink, Wikilink};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use lazy_static::lazy_static;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Instant;

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
    obsidian_repository_info: &ObsidianRepositoryInfo,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(
        LEVEL1,
        &format!(
            "{} {} {}",
            BACK_POPULATE_SECTION_PREFIX,
            obsidian_repository_info.wikilinks_sorted.len(),
            BACK_POPULATE_SECTION_SUFFIX
        ),
    )?;
    let start = Instant::now();

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
        if ambiguous_matches.len() > 0 {
            writer.writeln(LEVEL2, MATCHES_UNAMBIGUOUS)?;
        }

        write_back_populate_table(writer, &unambiguous_matches, true)?;
        apply_back_populate_changes(config, &unambiguous_matches)?;
    }

    let duration = start.elapsed();
    let duration_string = &format!("{:.2}", duration.as_millis());
    println!("back populate took: {}ms", duration_string);

    Ok(())
}

fn find_all_back_populate_matches(
    config: &ValidatedConfig,
    obsidian_repository_info: &ObsidianRepositoryInfo,
) -> Result<Vec<BackPopulateMatch>, Box<dyn Error + Send + Sync>> {
    let searcher = DeterministicSearch::new(config.back_populate_file_count());

    let ac = obsidian_repository_info
        .wikilinks_ac
        .as_ref()
        .expect("Wikilinks AC pattern should be initialized");
    let sorted_wikilinks: Vec<&Wikilink> =
        obsidian_repository_info.wikilinks_sorted.iter().collect();

    let matches = searcher.search_with_info(
        &obsidian_repository_info.markdown_files,
        |file_path, markdown_file_info| {
            if !cfg!(test) {
                if let Some(filter) = config.back_populate_file_filter() {
                    if !file_path.ends_with(filter) {
                        return None;
                    }
                }
            }

            // Process the file if it matches the filter
            match process_file(file_path, &sorted_wikilinks, config, markdown_file_info, ac) {
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
            target_map.insert(lower_target, wikilink.target.clone());
        }
    }

    // Create a map of display_text to normalized targets
    let mut display_text_map: HashMap<String, HashSet<String>> = HashMap::new();
    for wikilink in wikilinks {
        let lower_target = wikilink.target.to_lowercase();
        // Use the canonical form of the target from our target_map
        if let Some(canonical_target) = target_map.get(&lower_target) {
            display_text_map
                .entry(wikilink.display_text.clone())
                .or_default()
                .insert(canonical_target.clone());
        }
    }

    // Group matches by their found_text
    let mut matches_by_text: HashMap<String, Vec<BackPopulateMatch>> = HashMap::new();
    for match_info in matches {
        matches_by_text
            .entry(match_info.found_text.clone())
            .or_default()
            .push(match_info.clone());
    }

    // Identify truly ambiguous matches and separate them
    let mut ambiguous_matches = Vec::new();
    let mut unambiguous_matches = Vec::new();

    for (found_text, text_matches) in matches_by_text {
        if let Some(targets) = display_text_map.get(&found_text) {
            // After normalizing case, if we still have multiple distinct targets,
            // then it's truly ambiguous
            if targets.len() > 1 {
                ambiguous_matches.push(AmbiguousMatch {
                    display_text: found_text,
                    targets: targets.iter().cloned().collect(),
                    matches: text_matches,
                });
            } else {
                unambiguous_matches.extend(text_matches);
            }
        }
    }

    // Sort ambiguous matches by display text for consistent output
    ambiguous_matches.sort_by(|a, b| a.display_text.cmp(&b.display_text));

    (ambiguous_matches, unambiguous_matches)
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

fn process_file(
    file_path: &Path,
    sorted_wikilinks: &[&Wikilink],
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
    ac: &AhoCorasick,
    sorted_wikilinks: &[&Wikilink],
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
            let mut replacement = if matched_text == wikilink.target {
                // Only use simple format if exact match (case-sensitive)
                format!("{}", wikilink.target.to_wikilink())
            } else {
                // Use aliased format for case differences or actual aliases
                //format!("[[{}|{}]]", wikilink.wikilink.target, matched_text)
                format!("{}", wikilink.target.to_aliased_wikilink(matched_text))
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
        let key = (match_info.file_path.clone(), match_info.line_number);

        // Update or create line info
        let line_info = line_map.entry(key).or_insert(LineInfo {
            line_number: match_info.line_number,
            line_text: match_info.line_text.clone(),
            positions: Vec::new(),
        });
        line_info.positions.push(match_info.position);

        // Track file-level information
        file_info.insert(
            match_info.file_path.clone(),
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
                "the text \"{}\" matches {} targets:",
                ambiguous_match.display_text,
                ambiguous_match.targets.len(),
            ),
        )?;

        // Write out all possible targets
        for target in &ambiguous_match.targets {
            writer.writeln("", &format!("- {}", target.to_wikilink()))?;
        }

        // Reuse existing table writing code for the matches
        write_back_populate_table(writer, &ambiguous_match.matches, false)?;
    }

    Ok(())
}

fn write_invalid_wikilinks_table(
    writer: &ThreadSafeWriter,
    obsidian_repository_info: &ObsidianRepositoryInfo,
) -> Result<(), Box<dyn Error + Send + Sync>> {

    // Collect all invalid wikilinks from all files
    let mut invalid_wikilinks: Vec<(&Path, &InvalidWikilink)> = Vec::new();
    for (file_path, file_info) in &obsidian_repository_info.markdown_files {
        for invalid_wikilink in &file_info.invalid_wikilinks {
            invalid_wikilinks.push((file_path, invalid_wikilink));
        }
    }

    // If no invalid wikilinks, return early
    if invalid_wikilinks.is_empty() {
        return Ok(());
    }

    // Sort by filename and then line number
    invalid_wikilinks.sort_by(|a, b| {
        let file_a = a.0.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let file_b = b.0.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let file_cmp = file_a.cmp(file_b);
        if file_cmp != std::cmp::Ordering::Equal {
            file_cmp
        } else {
            a.1.line_number.cmp(&b.1.line_number)
        }
    });

    writer.writeln(LEVEL2, "invalid wikilinks")?;

    // Write header describing the count
    writer.writeln(
        "",
        &format!(
            "found {} invalid wikilinks in {} files\n",
            invalid_wikilinks.len(),
            invalid_wikilinks.iter().map(|(p, _)| p).collect::<HashSet<_>>().len()
        ),
    )?;

    // Prepare headers and alignments for the table
    let headers = vec![
        "file name",
        "line",
        "line text",
        "reason",
        "invalid wikilink",
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
                escape_pipe(&invalid_wikilink.content),
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
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // First, group matches by found_text
    let mut matches_by_text: BTreeMap<String, Vec<&BackPopulateMatch>> = BTreeMap::new();
    for m in matches {
        matches_by_text
            .entry(m.found_text.clone())
            .or_default()
            .push(m);
    }

    if is_unambiguous_match {
        // Count unique files across all matches
        let unique_files: HashSet<String> = matches.iter().map(|m| m.file_path.clone()).collect();
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

    // Rest of the function remains the same...
    for (found_text, text_matches) in matches_by_text.iter() {
        let total_occurrences = text_matches.len();
        let file_paths: HashSet<String> = text_matches
            .iter()
            .map(|m| m.file_path.to_string())
            .collect();

        let level_string = if is_unambiguous_match { LEVEL3 } else { LEVEL4 };

        writer.writeln(
            level_string,
            &format!(
                "found text: \"{}\" ({})",
                found_text,
                pluralize_occurrence_in_files(total_occurrences, file_paths.len())
            ),
        )?;

        // Sort matches by file path and line number
        let mut sorted_matches = text_matches.to_vec();
        sorted_matches.sort_by(|a, b| {
            let file_a = Path::new(&a.file_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let file_b = Path::new(&b.file_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            // First compare by file name
            let file_cmp = file_a.cmp(file_b);
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
                let highlighted_line = highlight_matches(&line_info.line_text, found_text);

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
        writer.writeln("", "\n---\n")?;
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

    // Apply replacements in sorted (reverse) order
    for match_info in sorted_matches {
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

        // Perform the replacement
        updated_line.replace_range(start..end, &match_info.replacement);

        // Validation check after each replacement
        if updated_line.contains("[[[") || updated_line.contains("]]]") {
            eprintln!(
                "\nWarning: Potential nested pattern detected after replacement in file '{}', line {}.\n\
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::MarkdownFileInfo;
    use crate::wikilink_types::Wikilink;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // Common helper function to build Aho-Corasick automaton from CompiledWikilinks
    fn build_aho_corasick(wikilinks: &[Wikilink]) -> AhoCorasick {
        let patterns: Vec<&str> = wikilinks.iter().map(|w| w.display_text.as_str()).collect();

        AhoCorasickBuilder::new()
            .ascii_case_insensitive(true)
            .match_kind(MatchKind::LeftmostLongest)
            .build(&patterns)
            .expect("Failed to build Aho-Corasick automaton")
    }

    #[cfg(test)]
    fn create_test_environment(
        apply_changes: bool,
        do_not_back_populate: Option<Vec<String>>,
        wikilinks: Option<Vec<Wikilink>>,
    ) -> (TempDir, ValidatedConfig, ObsidianRepositoryInfo) {
        let temp_dir = TempDir::new().unwrap();

        let config = ValidatedConfig::new(
            apply_changes,
            None, // back_populate_file_count
            None, // back_populate_filter
            do_not_back_populate,
            None,                           // ignore_folders
            temp_dir.path().to_path_buf(),  // obsidian_path
            temp_dir.path().join("output"), // output_folder
        );

        // Initialize repository info with default values
        let mut repo_info = ObsidianRepositoryInfo::default();

        // If custom wikilinks are provided, use them
        if let Some(wikilinks) = wikilinks {
            repo_info.wikilinks_sorted = wikilinks
        } else {
            // Default wikilink
            let wikilink = Wikilink {
                display_text: "Test Link".to_string(),
                target: "Test Link".to_string(),
                is_alias: false,
                is_image: false,
            };
            repo_info.wikilinks_sorted = vec![wikilink];
        }

        // Build the Aho-Corasick automaton
        repo_info.wikilinks_ac = Some(build_aho_corasick(&repo_info.wikilinks_sorted));

        repo_info.markdown_files = HashMap::new();

        (temp_dir, config, repo_info)
    }

    fn create_markdown_test_file(
        temp_dir: &TempDir,
        file_name: &str,
        content: &str,
        repo_info: &mut ObsidianRepositoryInfo,
    ) -> PathBuf {
        let file_path = temp_dir.path().join(file_name);
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        repo_info
            .markdown_files
            .insert(file_path.clone(), MarkdownFileInfo::new());

        file_path
    }

    // Helper struct for test cases
    struct TestCase {
        content: &'static str,
        wikilink: Wikilink,
        expected_matches: Vec<(&'static str, &'static str)>,
        description: &'static str,
    }

    fn verify_match(
        actual_match: &BackPopulateMatch,
        expected_text: &str,
        expected_base_replacement: &str,
        case_description: &str,
    ) {
        assert_eq!(
            actual_match.found_text, expected_text,
            "Wrong matched text for case: {}",
            case_description
        );

        let expected_replacement = if actual_match.in_markdown_table {
            expected_base_replacement.replace('|', r"\|")
        } else {
            expected_base_replacement.to_string()
        };

        assert_eq!(
            actual_match.replacement,
            expected_replacement,
            "Wrong replacement for case: {}\nExpected: {}\nActual: {}\nIn table: {}",
            case_description,
            expected_replacement,
            actual_match.replacement,
            actual_match.in_markdown_table
        );
    }

    fn get_case_sensitivity_test_cases() -> Vec<TestCase> {
        vec![
            TestCase {
                content: "test link TEST LINK Test Link",
                wikilink: Wikilink {
                    display_text: "Test Link".to_string(),
                    target: "Test Link".to_string(),
                    is_alias: false,
                    is_image: false,
                },
                expected_matches: vec![
                    ("test link", "[[Test Link|test link]]"),
                    ("TEST LINK", "[[Test Link|TEST LINK]]"),
                    ("Test Link", "[[Test Link]]"), // Exact match
                ],
                description: "Basic case-insensitive matching",
            },
            TestCase {
                content: "josh likes apples",
                wikilink: Wikilink {
                    display_text: "josh".to_string(),
                    target: "Joshua Strayhorn".to_string(),
                    is_alias: true,
                    is_image: false,
                },
                expected_matches: vec![("josh", "[[Joshua Strayhorn|josh]]")],
                description: "Alias case preservation",
            },
            TestCase {
                content: "| Test Link | Another test link |",
                wikilink: Wikilink {
                    display_text: "Test Link".to_string(),
                    target: "Test Link".to_string(),
                    is_alias: false,
                    is_image: false,
                },
                expected_matches: vec![
                    ("Test Link", "[[Test Link]]"), // Exact match
                    ("test link", "[[Test Link|test link]]"),
                ],
                description: "Case handling in tables",
            },
        ]
    }

    #[test]
    fn test_config_creation() {
        // Basic usage with defaults
        let (_, basic_config, _) = create_test_environment(false, None, None);
        assert!(!basic_config.apply_changes());

        // With apply_changes set to true
        let (_, apply_config, _) = create_test_environment(true, None, None);
        assert!(apply_config.apply_changes());

        // With do_not_back_populate patterns
        let patterns = vec!["pattern1".to_string(), "pattern2".to_string()];
        let (_, pattern_config, _) = create_test_environment(false, Some(patterns.clone()), None);
        assert_eq!(
            pattern_config.do_not_back_populate(),
            Some(patterns.as_slice())
        );

        // With both parameters
        let (_, full_config, _) =
            create_test_environment(true, Some(vec!["pattern".to_string()]), None);
        assert!(full_config.apply_changes());
        assert!(full_config.do_not_back_populate().is_some());
    }

    #[test]
    fn test_case_sensitivity_behavior() {
        // Initialize test environment without specific wikilinks
        let (temp_dir, config, mut repo_info) = create_test_environment(false, None, None);

        for case in get_case_sensitivity_test_cases() {
            let file_path =
                create_markdown_test_file(&temp_dir, "test.md", case.content, &mut repo_info);

            // Create a custom wikilink and build AC automaton directly
            let wikilink = case.wikilink;
            let ac = build_aho_corasick(&[wikilink.clone()]);
            let markdown_info = MarkdownFileInfo::new();

            let matches = process_line(
                0,
                case.content,
                &file_path,
                &ac,
                &[&wikilink],
                &config,
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
                verify_match(
                    actual_match,
                    expected_text,
                    expected_base_replacement,
                    case.description,
                );
            }
        }
    }

    #[test]
    fn test_find_matches_with_existing_wikilinks() {
        // Create test environment with default settings
        let (temp_dir, config, mut repo_info) = create_test_environment(false, None, None);
        let content =
            "[[Some Link]] and Test Link in same line\nTest Link [[Other Link]] Test Link mixed";

        // Create the test Markdown file using the helper function
        create_markdown_test_file(&temp_dir, "test.md", content, &mut repo_info);

        // Find matches
        let matches = find_all_back_populate_matches(&config, &repo_info).unwrap();

        // We expect 3 matches for "Test Link" outside existing wikilinks
        assert_eq!(matches.len(), 3, "Mismatch in number of matches");

        // Verify that the matches are at the expected positions
        let expected_lines = vec![1, 2, 2];
        let actual_lines: Vec<usize> = matches.iter().map(|m| m.line_number).collect();
        assert_eq!(
            actual_lines, expected_lines,
            "Mismatch in line numbers of matches"
        );
    }

    #[test]
    fn test_apply_changes() {
        // Create test environment with apply_changes set to true
        let (temp_dir, config, mut repo_info) = create_test_environment(true, None, None);

        // Create a test Markdown file using the helper function
        let content = "Here is Test Link\nNo change here\nAnother Test Link";
        let file_path = create_markdown_test_file(&temp_dir, "test.md", content, &mut repo_info);

        // Find matches
        let matches = find_all_back_populate_matches(&config, &repo_info).unwrap();

        // Apply changes using the config from create_test_environment
        apply_back_populate_changes(&config, &matches).unwrap();

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
        let (temp_dir, config, mut repo_info) = create_test_environment(false, None, None);
        let content = "[[Kyriana McCoy|Kyriana]] - Kyri and [[Kalina McCoy|Kali]]";

        let file_path = create_markdown_test_file(&temp_dir, "test.md", content, &mut repo_info);

        repo_info
            .markdown_files
            .insert(file_path.clone(), MarkdownFileInfo::new());

        // Add the overlapping wikilinks to repo_info
        let wikilink1 = Wikilink {
            display_text: "Kyri".to_string(),
            target: "Kyri".to_string(),
            is_alias: false,
            is_image: false,
        };
        let wikilink2 = Wikilink {
            display_text: "Kyri".to_string(),
            target: "Kyriana McCoy".to_string(),
            is_alias: true,
            is_image: false,

        };

        // Clear and add to the sorted vec
        repo_info.wikilinks_sorted.clear();
        repo_info.wikilinks_sorted.push(wikilink1);
        repo_info.wikilinks_sorted.push(wikilink2);

        // Use the helper function to build the automaton
        repo_info.wikilinks_ac = Some(build_aho_corasick(&repo_info.wikilinks_sorted));

        let matches = find_all_back_populate_matches(&config, &repo_info).unwrap();

        // We should only get one match for "Kyri" at position 28
        assert_eq!(matches.len(), 1, "Expected exactly one match");
        assert_eq!(matches[0].position, 28, "Expected match at position 28");
    }

    #[test]
    fn test_process_line_with_mozzarella_exclusion() {
        // Set up the test environment with specific do_not_back_populate patterns
        let do_not_back_populate_patterns = vec!["[[mozzarella]] cheese".to_string()];
        let (temp_dir, config, _) =
            create_test_environment(false, Some(do_not_back_populate_patterns), None);

        let file_path = temp_dir.path().join("test.md");

        let wikilink = Wikilink {
            display_text: "cheese".to_string(),
            target: "fromage".to_string(),
            is_alias: true,
            is_image: false,

        };

        let ac = build_aho_corasick(&[wikilink.clone()]);
        let markdown_info = MarkdownFileInfo::new();

        // Test line with excluded pattern
        let line = "- 1 1/2 cup [[mozzarella]] cheese shredded";
        let matches = process_line(
            0,
            line,
            &file_path,
            &ac,
            &[&wikilink],
            &config,
            &markdown_info,
        )
        .unwrap();

        assert_eq!(matches.len(), 0, "Match should be excluded");

        // Test that other cheese references still match
        let line = "I love cheese on my pizza";
        let matches = process_line(
            0,
            line,
            &file_path,
            &ac,
            &[&wikilink],
            &config,
            &markdown_info,
        )
        .unwrap();

        assert_eq!(matches.len(), 1, "Match should be included");
        assert_eq!(matches[0].found_text, "cheese");
    }

    #[test]
    fn test_no_self_referential_back_population() {
        // Create test environment with apply_changes set to false
        let (temp_dir, config, mut repo_info) = create_test_environment(false, None, None);

        // Create a wikilink for testing that includes an alias
        let wikilink = Wikilink {
            display_text: "Will".to_string(),
            target: "William.md".to_string(),
            is_alias: true,
            is_image: false,

        };

        // Update repo_info with the custom wikilink
        repo_info.wikilinks_sorted.clear();
        repo_info.wikilinks_sorted.push(wikilink);
        repo_info.wikilinks_ac = Some(build_aho_corasick(&repo_info.wikilinks_sorted));

        // Create a test file with its own name using the helper function
        let content = "Will is mentioned here but should not be replaced";
        create_markdown_test_file(&temp_dir, "Will.md", content, &mut repo_info);

        // Find matches
        let matches = find_all_back_populate_matches(&config, &repo_info).unwrap();

        // Should not find matches in the file itself
        assert_eq!(
            matches.len(),
            0,
            "Should not find matches on page's own name"
        );

        // Create another file using the same content
        let other_file_path =
            create_markdown_test_file(&temp_dir, "Other.md", content, &mut repo_info);

        // Find matches again
        let matches = find_all_back_populate_matches(&config, &repo_info).unwrap();

        // Should find matches in other files
        assert_eq!(matches.len(), 1, "Should find match on other pages");
        assert_eq!(
            matches[0].file_path,
            format_relative_path(&other_file_path, config.obsidian_path()),
            "Match should be in 'Other.md'"
        );
    }

    #[test]
    fn test_should_create_match_in_table() {
        // Set up the test environment
        let (temp_dir, _, _) = create_test_environment(false, None, None);
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
    fn test_back_populate_content() {
        // Initialize environment with `apply_changes` set to true
        let (temp_dir, config, mut repo_info) = create_test_environment(true, None, None);

        // Define test cases with various content structures
        let test_cases = vec![
            (
                "# Test Table\n|Name|Description|\n|---|---|\n|Test Link|Sample text|\n",
                vec![BackPopulateMatch {
                    file_path: "test.md".into(),
                    line_number: 4,
                    line_text: "|Test Link|Sample text|".into(),
                    found_text: "Test Link".into(),
                    replacement: "[[Test Link\\|Another Name]]".into(),
                    position: 1,
                    in_markdown_table: true,
                }],
                "Table content replacement",
            ),
            (
                "# Mixed Content\n\
            Regular Test Link here\n\
            |Name|Description|\n\
            |---|---|\n\
            |Test Link|Sample|\n\
            More Test Link text",
                vec![
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
                ],
                "Mixed table and regular content replacement",
            ),
        ];

        for (content, matches, description) in test_cases {
            let file_path =
                create_markdown_test_file(&temp_dir, "test.md", content, &mut repo_info);

            // Apply back-populate changes
            apply_back_populate_changes(&config, &matches).unwrap();

            // Verify changes
            let updated_content = fs::read_to_string(&file_path).unwrap();
            for match_info in matches {
                assert!(
                    updated_content.contains(&match_info.replacement),
                    "Failed for: {}",
                    description
                );
            }
        }
    }

    #[test]
    fn test_no_matches_for_frontmatter_aliases() {
        let (temp_dir, config, mut repo_info) = create_test_environment(false, None, None);

        // Create a wikilink for testing that includes an alias
        let wikilink = Wikilink {
            display_text: "Will".to_string(),
            target: "William.md".to_string(),
            is_alias: true,
            is_image: false,

        };

        // Clear and add to the sorted vec
        repo_info.wikilinks_sorted.clear();
        repo_info.wikilinks_sorted.push(wikilink);

        // Use the helper function to build the automaton
        repo_info.wikilinks_ac = Some(build_aho_corasick(&repo_info.wikilinks_sorted));

        // Create a test file with its own name
        let content = "Will is mentioned here but should not be replaced";
        let file_path = temp_dir.path().join("Will.md");
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        repo_info
            .markdown_files
            .insert(file_path.clone(), MarkdownFileInfo::new());

        // Now, use the config returned from create_test_environment
        let matches = find_all_back_populate_matches(&config, &repo_info).unwrap();

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
            .insert(other_file_path.clone(), MarkdownFileInfo::new());

        let matches = find_all_back_populate_matches(&config, &repo_info).unwrap();

        assert_eq!(matches.len(), 1, "Should find match on other pages");
    }

    #[test]
    fn test_process_line_table_escaping_combined() {
        // Define multiple wikilinks
        let wikilinks = vec![
            Wikilink {
                display_text: "Test Link".to_string(),
                target: "Target Page".to_string(),
                is_alias: true,
                is_image: false,

            },
            Wikilink {
                display_text: "Another Link".to_string(),
                target: "Other Page".to_string(),
                is_alias: false,
                is_image: false,

            },
        ];
        // Initialize environment with custom wikilinks
        let (temp_dir, config, repo_info) =
            create_test_environment(false, None, Some(wikilinks.clone()));

        // Compile the wikilinks
        let sorted_wikilinks = &repo_info.wikilinks_sorted;

        let ac = build_aho_corasick(sorted_wikilinks);

        let markdown_info = MarkdownFileInfo::new();

        // Define test cases with different table formats and expected replacements
        let test_cases = vec![
            (
                "| Test Link | Another Link | description |",
                vec![
                    "[[Target Page\\|Test Link]]",
                    "[[Other Page\\|Another Link]]",
                ],
                "Multiple matches in one row",
            ),
            (
                "| prefix Test Link suffix | Another Link |",
                vec![
                    "[[Target Page\\|Test Link]]",
                    "[[Other Page\\|Another Link]]",
                ],
                "Table cells with surrounding text",
            ),
            (
                "| column1 | Test Link | Another Link |",
                vec![
                    "[[Target Page\\|Test Link]]",
                    "[[Other Page\\|Another Link]]",
                ],
                "Different column positions",
            ),
            (
                "| Test Link | description | Another Link |",
                vec![
                    "[[Target Page\\|Test Link]]",
                    "[[Other Page\\|Another Link]]",
                ],
                "Multiple replacements in different columns",
            ),
        ];

        // Create references to the compiled wikilinks
        let wikilink_refs: Vec<&Wikilink> = sorted_wikilinks.iter().collect();
        for (line, expected_replacements, description) in test_cases {
            let matches = process_line(
                0,
                line,
                &temp_dir.path().join("test.md"),
                &ac,
                &wikilink_refs,
                &config,
                &markdown_info,
            )
            .unwrap();

            assert_eq!(
                matches.len(),
                expected_replacements.len(),
                "Incorrect number of replacements for: {}",
                description
            );

            for (match_info, expected) in matches.iter().zip(expected_replacements.iter()) {
                assert_eq!(
                    match_info.replacement, *expected,
                    "Incorrect replacement for: {}",
                    description
                );
                assert!(
                    match_info.in_markdown_table,
                    "Should be marked as in table for: {}",
                    description
                );
            }
        }
    }

    #[test]
    fn test_escape_pipe() {
        let test_cases = vec![
            (
                "[[santé|medical scheduling]]",
                "[[santé\\|medical scheduling]]",
            ),
            ("a\\|b", "a\\|b"),
            ("a\\\\|b", "a\\\\\\|b"),
            ("col1|col2|col3", "col1\\|col2\\|col3"),
            ("[[café|☕]]|[[thé|🫖]]", "[[café\\|☕]]\\|[[thé\\|🫖]]"),
        ];

        for (input, expected) in test_cases {
            assert_eq!(
                escape_pipe(input),
                expected,
                "Failed to escape pipe in input: {}",
                input
            );
        }
    }

    #[test]
    fn test_is_within_wikilink() {
        let test_cases = vec![
            // ASCII cases
            ("before [[link]] after", 7, false),
            ("before [[link]] after", 8, false),
            ("before [[link]] after", 9, true),
            ("before [[link]] after", 10, true),
            ("before [[link]] after", 11, true),
            ("before [[link]] after", 12, true),
            ("before [[link]] after", 13, false),
            ("before [[link]] after", 14, false),
            // Unicode cases
            ("привет [[ссылка]] текст", 13, false),
            ("привет [[ссылка]] текст", 14, false),
            ("привет [[ссылка]] текст", 15, true),
            ("привет [[ссылка]] текст", 25, true),
            ("привет [[ссылка]] текст", 27, false),
            ("привет [[ссылка]] текст", 28, false),
            ("привет [[ссылка]] текст", 12, false),
            ("привет [[ссылка]] текст", 29, false),
        ];

        for (text, pos, expected) in test_cases {
            assert_eq!(
                is_within_wikilink(text, pos),
                expected,
                "Failed for text '{}' at position {}",
                text,
                pos
            );
        }
    }

    #[test]
    fn test_file_processing_state() {
        let mut state = FileProcessingState::new();

        // Initial state
        assert!(!state.should_skip_line(), "Initial state should not skip");

        // Frontmatter
        state.update_for_line("---");
        assert!(state.should_skip_line(), "Should skip in frontmatter");
        state.update_for_line("title: Test");
        assert!(state.should_skip_line(), "Should skip frontmatter content");
        state.update_for_line("---");
        assert!(
            !state.should_skip_line(),
            "Should not skip after frontmatter"
        );

        // Code block
        state.update_for_line("```rust");
        assert!(state.should_skip_line(), "Should skip in code block");
        state.update_for_line("let x = 42;");
        assert!(state.should_skip_line(), "Should skip code block content");
        state.update_for_line("```");
        assert!(
            !state.should_skip_line(),
            "Should not skip after code block"
        );

        // Combined frontmatter and code block
        state.update_for_line("---");
        assert!(state.should_skip_line(), "Should skip in frontmatter again");
        state.update_for_line("description: complex");
        assert!(state.should_skip_line(), "Should skip frontmatter content");
        state.update_for_line("---");
        assert!(
            !state.should_skip_line(),
            "Should not skip after frontmatter"
        );

        state.update_for_line("```");
        assert!(
            state.should_skip_line(),
            "Should skip in another code block"
        );
        state.update_for_line("print('Hello')");
        assert!(state.should_skip_line(), "Should skip code block content");
        state.update_for_line("```");
        assert!(
            !state.should_skip_line(),
            "Should not skip after code block"
        );
    }

    #[test]
    fn test_alias_priority() {
        // Initialize test environment with specific wikilinks
        let wikilinks = vec![
            // Define an alias relationship: "tomatoes" is an alias for "tomato"
            Wikilink {
                display_text: "tomatoes".to_string(),
                target: "tomato".to_string(),
                is_alias: true,
                is_image: false,

            },
            // Also include a direct "tomatoes" wikilink that should not be used
            Wikilink {
                display_text: "tomatoes".to_string(),
                target: "tomatoes".to_string(),
                is_alias: false,
                is_image: false,

            },
        ];

        let (temp_dir, config, mut repo_info) =
            create_test_environment(false, None, Some(wikilinks));

        // Create a test file that contains the word "tomatoes"
        let content = "I love tomatoes in my salad";
        create_markdown_test_file(&temp_dir, "salad.md", content, &mut repo_info);

        // Find matches
        let matches = find_all_back_populate_matches(&config, &repo_info).unwrap();

        // Verify we got exactly one match
        assert_eq!(matches.len(), 1, "Should find exactly one match");

        // Verify the match uses the alias form
        let match_info = &matches[0];
        assert_eq!(match_info.found_text, "tomatoes");
        assert_eq!(
            match_info.replacement, "[[tomato|tomatoes]]",
            "Should use the alias form [[tomato|tomatoes]] instead of [[tomatoes]]"
        );
    }

    #[test]
    fn test_identify_ambiguous_matches() {
        // Create test wikilinks
        let wikilinks = vec![
            Wikilink {
                display_text: "Ed".to_string(),
                target: "Ed Barnes".to_string(),
                is_alias: true,
                is_image: false,

            },
            Wikilink {
                display_text: "Ed".to_string(),
                target: "Ed Stanfield".to_string(),
                is_alias: true,
                is_image: false,

            },
            Wikilink {
                display_text: "Unique".to_string(),
                target: "Unique Target".to_string(),
                is_alias: false,
                is_image: false,

            },
        ];

        // Create test matches
        let matches = vec![
            BackPopulateMatch {
                file_path: "test1.md".to_string(),
                line_number: 1,
                line_text: "Ed wrote this".to_string(),
                found_text: "Ed".to_string(),
                replacement: "[[Ed Barnes|Ed]]".to_string(),
                position: 0,
                in_markdown_table: false,
            },
            BackPopulateMatch {
                file_path: "test2.md".to_string(),
                line_number: 1,
                line_text: "Unique wrote this".to_string(),
                found_text: "Unique".to_string(),
                replacement: "[[Unique Target]]".to_string(),
                position: 0,
                in_markdown_table: false,
            },
        ];

        let (ambiguous, unambiguous) = identify_ambiguous_matches(&matches, &wikilinks);

        // Check ambiguous matches
        assert_eq!(ambiguous.len(), 1, "Should have one ambiguous match group");
        assert_eq!(ambiguous[0].display_text, "Ed");
        assert_eq!(ambiguous[0].targets.len(), 2);
        assert!(ambiguous[0].targets.contains(&"Ed Barnes".to_string()));
        assert!(ambiguous[0].targets.contains(&"Ed Stanfield".to_string()));

        // Check unambiguous matches
        assert_eq!(unambiguous.len(), 1, "Should have one unambiguous match");
        assert_eq!(unambiguous[0].found_text, "Unique");
    }

    #[test]
    fn test_case_insensitive_targets() {
        // Create test wikilinks with case variations
        let wikilinks = vec![
            Wikilink {
                display_text: "Amazon".to_string(),
                target: "Amazon".to_string(),
                is_alias: false,
                is_image: false,

            },
            Wikilink {
                display_text: "amazon".to_string(),
                target: "amazon".to_string(),
                is_alias: false,
                is_image: false,

            },
        ];

        // Create test matches
        let matches = vec![
            BackPopulateMatch {
                file_path: "test1.md".to_string(),
                line_number: 1,
                line_text: "- [[Amazon]]".to_string(),
                found_text: "Amazon".to_string(),
                replacement: "[[Amazon]]".to_string(),
                position: 0,
                in_markdown_table: false,
            },
            BackPopulateMatch {
                file_path: "test1.md".to_string(),
                line_number: 2,
                line_text: "- [[amazon]]".to_string(),
                found_text: "amazon".to_string(),
                replacement: "[[amazon]]".to_string(),
                position: 0,
                in_markdown_table: false,
            },
        ];

        let (ambiguous, unambiguous) = identify_ambiguous_matches(&matches, &wikilinks);

        // Should treat case variations of the same target as the same file
        assert_eq!(
            ambiguous.len(),
            0,
            "Case variations of the same target should not be ambiguous"
        );
        assert_eq!(
            unambiguous.len(),
            2,
            "Both matches should be considered unambiguous"
        );
    }

    #[test]
    fn test_truly_ambiguous_targets() {
        // Create test wikilinks with actually different targets
        let wikilinks = vec![
            Wikilink {
                display_text: "Amazon".to_string(),
                target: "Amazon (company)".to_string(),
                is_alias: true,
                is_image: false,

            },
            Wikilink {
                display_text: "Amazon".to_string(),
                target: "Amazon (river)".to_string(),
                is_alias: true,
                is_image: false,

            },
        ];

        let matches = vec![BackPopulateMatch {
            file_path: "test1.md".to_string(),
            line_number: 1,
            line_text: "Amazon is huge".to_string(),
            found_text: "Amazon".to_string(),
            replacement: "[[Amazon (company)|Amazon]]".to_string(),
            position: 0,
            in_markdown_table: false,
        }];

        let (ambiguous, unambiguous) = identify_ambiguous_matches(&matches, &wikilinks);

        assert_eq!(
            ambiguous.len(),
            1,
            "Different targets should be identified as ambiguous"
        );
        assert_eq!(
            unambiguous.len(),
            0,
            "No matches should be considered unambiguous"
        );
        assert_eq!(ambiguous[0].targets.len(), 2);
    }

    #[test]
    fn test_mixed_case_and_truly_ambiguous() {
        let wikilinks = vec![
            // Case variations of one target
            Wikilink {
                display_text: "AWS".to_string(),
                target: "AWS".to_string(),
                is_alias: false,
                is_image: false,

            },
            Wikilink {
                display_text: "aws".to_string(),
                target: "aws".to_string(),
                is_alias: false,
                is_image: false,

            },
            // Truly different targets
            Wikilink {
                display_text: "Amazon".to_string(),
                target: "Amazon (company)".to_string(),
                is_alias: true,
                is_image: false,

            },
            Wikilink {
                display_text: "Amazon".to_string(),
                target: "Amazon (river)".to_string(),
                is_alias: true,
                is_image: false,

            },
        ];

        let matches = vec![
            BackPopulateMatch {
                file_path: "test1.md".to_string(),
                line_number: 1,
                line_text: "AWS and aws are the same".to_string(),
                found_text: "AWS".to_string(),
                replacement: "[[AWS]]".to_string(),
                position: 0,
                in_markdown_table: false,
            },
            BackPopulateMatch {
                file_path: "test1.md".to_string(),
                line_number: 2,
                line_text: "Amazon is ambiguous".to_string(),
                found_text: "Amazon".to_string(),
                replacement: "[[Amazon (company)|Amazon]]".to_string(),
                position: 0,
                in_markdown_table: false,
            },
        ];

        let (ambiguous, unambiguous) = identify_ambiguous_matches(&matches, &wikilinks);

        assert_eq!(
            ambiguous.len(),
            1,
            "Should only identify truly different targets as ambiguous"
        );
        assert_eq!(
            unambiguous.len(),
            1,
            "Case variations should be identified as unambiguous"
        );
    }
}

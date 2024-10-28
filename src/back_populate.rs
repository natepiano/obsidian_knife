use crate::constants::*;
use crate::deterministic_file_search::DeterministicSearch;
use crate::scan::ObsidianRepositoryInfo;
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::validated_config::ValidatedConfig;
use crate::wikilink::{CompiledWikilink, EXTERNAL_MARKDOWN_REGEX};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone)]
struct BackPopulateMatch {
    file_path: String,
    line_number: usize,
    line_text: String,
    found_text: String,
    replacement: String,
    position: usize,
}

#[derive(Debug)]
struct FrontmatterState {
    in_frontmatter: bool,
    delimiter_count: usize,
}

impl FrontmatterState {
    fn new() -> Self {
        Self {
            in_frontmatter: false,
            delimiter_count: 0,
        }
    }

    fn update_for_line(&mut self, line: &str) -> bool {
        if line.trim() == "---" {
            self.delimiter_count += 1;
            self.in_frontmatter = self.delimiter_count % 2 != 0;
            true
        } else {
            false
        }
    }
}

pub fn process_back_populate(
    config: &ValidatedConfig,
    obsidian_repository_info: &ObsidianRepositoryInfo,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL1, BACK_POPULATE)?;

    // Sort wikilinks by length (longest first)
    let mut sorted_wikilinks: Vec<&CompiledWikilink> =
        obsidian_repository_info.all_wikilinks.iter().collect();
    sorted_wikilinks.sort_by(|a, b| {
        // 1. Sort by display text length (longest first)
        b.wikilink
            .display_text
            .len()
            .cmp(&a.wikilink.display_text.len())
            // 2. Then by target length if display lengths are equal
            .then_with(|| b.wikilink.target.len().cmp(&a.wikilink.target.len()))
            // 3. Finally by display text lexicographically to stabilize order
            .then_with(|| b.wikilink.display_text.cmp(&a.wikilink.display_text))
    });

    println!("links to back populate: {} ", sorted_wikilinks.len());

    let matches =
        find_all_back_populate_matches(config, obsidian_repository_info, &sorted_wikilinks)?;

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
    collected_files: &ObsidianRepositoryInfo,
    sorted_wikilinks: &[&CompiledWikilink],
) -> Result<Vec<BackPopulateMatch>, Box<dyn Error + Send + Sync>> {
    let searcher = DeterministicSearch::new(config.back_populate_file_count());

    let wikilinks = sorted_wikilinks.to_vec();

    let matches =
        searcher.search_with_info(
            &collected_files.markdown_files,
            |file_path, _| match process_file(file_path, &wikilinks, config) {
                Ok(file_matches) if !file_matches.is_empty() => Some(file_matches),
                _ => None,
            },
        );

    Ok(matches.into_iter().flatten().collect())
}

fn process_file(
    file_path: &Path,
    sorted_wikilinks: &[&CompiledWikilink],
    config: &ValidatedConfig,
) -> Result<Vec<BackPopulateMatch>, Box<dyn Error + Send + Sync>> {
    let mut matches = Vec::new(); // Local vector to collect matches
    let content = fs::read_to_string(file_path)?;
    let reader = BufReader::new(content.as_bytes());
    let mut frontmatter = FrontmatterState::new();

    for (line_idx, line) in reader.lines().enumerate() {
        let line = line?;

        // Skip empty or whitespace-only lines early
        if line.trim().is_empty() {
            continue;
        }

        if frontmatter.update_for_line(&line) || frontmatter.in_frontmatter {
            continue;
        }

        let mut processed_line = String::new();
        let mut last_pos = 0;

        // Regex to match Markdown links `[text](url)`
        for mat in EXTERNAL_MARKDOWN_REGEX.find_iter(&line) {
            let non_link_text = &line[last_pos..mat.start()];
            // Process non-link text for matches
            process_line(
                line_idx,
                non_link_text,
                file_path,
                &content,
                sorted_wikilinks,
                config,
                &mut matches, // Use local matches vector here
            )?;

            // Add non-link text and link back to processed line
            processed_line.push_str(non_link_text);
            processed_line.push_str(mat.as_str());
            last_pos = mat.end();
        }

        // Process any remaining non-link text after the last link
        let remaining_text = &line[last_pos..];
        process_line(
            line_idx,
            remaining_text,
            file_path,
            &content,
            sorted_wikilinks,
            config,
            &mut matches,
        )?;
        processed_line.push_str(remaining_text);
    }

    Ok(matches) // Return local matches collected in this file
}

fn process_line(
    line_idx: usize,
    line: &str,
    file_path: &Path,
    full_content: &str,
    sorted_wikilinks: &[&CompiledWikilink],
    config: &ValidatedConfig,
    matches: &mut Vec<BackPopulateMatch>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut matched_positions = Vec::new();

    for wikilink in sorted_wikilinks {
        let mut search_start = 0;

        // Use regex matching and respect the sorted order for specificity
        while search_start < line.len() {
            if let Some(match_info) = find_next_match(
                wikilink,
                line,
                search_start,
                line_idx,
                file_path,
                full_content,
                config,
            )? {
                let starts_at = match_info.position;
                let ends_at = starts_at + match_info.found_text.len();

                // Check for overlapping matches
                let overlaps = matched_positions.iter().any(|&(start, end)| {
                    (starts_at >= start && starts_at < end)
                        || (ends_at > start && ends_at <= end)
                        || (starts_at <= start && ends_at >= end)
                });

                // Check for exclusion patterns - now case-insensitive
                let should_exclude = if let Some(exclusion_patterns) = config.do_not_back_populate()
                {
                    exclusion_patterns.iter().any(|pattern| {
                        if let Some(pattern_start) =
                            line.to_lowercase().find(&pattern.to_lowercase())
                        {
                            let pattern_end = pattern_start + pattern.len();
                            starts_at >= pattern_start && starts_at < pattern_end
                        } else {
                            false
                        }
                    })
                } else {
                    false
                };

                // Skip this match if it overlaps or should be excluded
                if overlaps || should_exclude {
                    search_start = ends_at;
                    continue;
                }

                // Debug statement to show the match information
                println!(
                    "Match found in '{}' line {} position {}",
                    match_info.file_path, match_info.line_number, match_info.position
                );
                println!(" line text:{}", match_info.line_text);
                println!(
                    "  found: '{}' replace with: '{}'",
                    match_info.found_text, match_info.replacement
                );
                println!("  Wikilink: {} - {:?}", wikilink, wikilink.wikilink);
                println!();

                // Store the matched positions and add the match
                matched_positions.push((starts_at, ends_at));
                search_start = ends_at;
                matches.push(match_info.clone());
            } else {
                break;
            }
        }
    }

    Ok(())
}

fn find_next_match(
    wikilink: &CompiledWikilink,
    line: &str,
    start_pos: usize,
    line_idx: usize,
    file_path: &Path,
    full_content: &str,
    config: &ValidatedConfig,
) -> Result<Option<BackPopulateMatch>, Box<dyn Error + Send + Sync>> {
    if let Some(mat) = wikilink.regex.find(&line[start_pos..]) {
        let absolute_start = start_pos + mat.start();
        let absolute_end = start_pos + mat.end();
        let matched_text = &line[absolute_start..absolute_end];

        if should_create_match(
            line,
            absolute_start,
            matched_text,
            line_idx,
            full_content,
            file_path,
        ) {
            let replacement = if wikilink.wikilink.is_alias {
                // For alias matches, preserve the case of the found text in the alias portion
                format!("[[{}|{}]]", wikilink.wikilink.target, matched_text)
            } else if matched_text != wikilink.wikilink.target {
                // For non-alias matches where case differs, create new alias
                format!("[[{}|{}]]", wikilink.wikilink.target, matched_text)
            } else {
                // Case matches exactly, use simple wikilink
                format!("[[{}]]", wikilink.wikilink.target)
            };

            let match_info = BackPopulateMatch {
                file_path: format_relative_path(file_path, config.obsidian_path()),
                line_number: line_idx + 1,
                line_text: line.to_string(),
                found_text: matched_text.to_string(),
                replacement,
                position: absolute_start,
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
    line_idx: usize,
    full_content: &str,
    file_path: &Path,
) -> bool {
    // First check if this is the text's own page
    if let Some(stem) = file_path.file_stem().and_then(|s| s.to_str()) {
        if stem.eq_ignore_ascii_case(matched_text) {
            return false;
        }
    }

    !is_within_wikilink(line, absolute_start)
        && !is_in_code_block(full_content, line_idx)
        && !is_within_external_link(line, absolute_start)
}

fn is_within_external_link(line: &str, position: usize) -> bool {
    // Look for Markdown links, e.g., [text](url)
    let before = &line[..position];
    let after = &line[position..];

    // Check if the text is inside a Markdown link by looking for "[" before and "](" after
    if let Some(open_bracket) = before.rfind('[') {
        if let Some(close_bracket) = after.find("](") {
            return position > open_bracket && position < open_bracket + close_bracket + 2;
        }
    }

    false
}

fn is_within_wikilink(line: &str, position: usize) -> bool {
    // Find all wikilink pairs in the line
    let mut current_pos = 0;
    while let Some(start) = line[current_pos..].find("[[") {
        let start_pos = current_pos + start;
        if let Some(end) = line[start_pos..].find("]]") {
            let end_pos = start_pos + end + 2; // +2 to include the closing brackets

            // Check if our position falls within this wikilink
            if position >= start_pos && position < end_pos {
                return true;
            }

            current_pos = end_pos;
        } else {
            break; // No more closing brackets
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

fn is_in_code_block(content: &str, current_line: usize) -> bool {
    let lines: Vec<&str> = content.lines().collect();
    let mut in_code_block = false;
    let mut triple_ticks = 0;

    for (idx, line) in lines.iter().take(current_line + 1).enumerate() {
        if line.trim().starts_with("```") {
            triple_ticks += 1;
            in_code_block = triple_ticks % 2 != 0;
        }
        if idx == current_line {
            return in_code_block;
        }
    }
    false
}

fn write_back_populate_table(
    writer: &ThreadSafeWriter,
    matches: &[BackPopulateMatch],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(
        "",
        &format!("found {} matches to back populate", matches.len()),
    )?;

    // Group and sort matches by file path
    let mut matches_by_file: BTreeMap<String, Vec<&BackPopulateMatch>> = BTreeMap::new();
    for m in matches {
        let file_key = m.file_path.trim_end_matches(".md").to_string(); // Remove `.md`
        matches_by_file.entry(file_key).or_default().push(m);
    }

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
            "will replace with",
            "escaped replacement",
        ];

        // Collect rows for this file, with escaped brackets and pipe in the `escaped replacement` column
        let rows: Vec<Vec<String>> = file_matches
            .iter()
            .map(|m| {
                vec![
                    m.line_number.to_string(),
                    escape_pipe(&m.line_text),
                    escape_pipe(&m.found_text),
                    escape_pipe(&m.replacement), // Escape for Markdown table
                    escape_brackets_and_pipe(&m.replacement), // Escaped replacement with brackets
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
                ColumnAlignment::Left,
                ColumnAlignment::Left,
            ]),
        )?;
    }

    Ok(())
}

// Helper function to escape pipes in Markdown strings
fn escape_pipe(text: &str) -> String {
    text.replace('|', r"\|")
}

// Helper function to escape pipes and brackets for visual verification in `escaped replacement`
fn escape_brackets_and_pipe(text: &str) -> String {
    text.replace('[', r"\[")
        .replace(']', r"\]")
        .replace('|', r"\|")
}

fn apply_back_populate_changes(
    config: &ValidatedConfig,
    matches: &[BackPopulateMatch],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if !config.apply_changes() {
        return Ok(());
    }

    for match_info in matches {
        let full_path = config.obsidian_path().join(&match_info.file_path);
        let content = fs::read_to_string(&full_path)?;

        let updated_content = content
            .lines()
            .enumerate()
            .map(|(idx, line)| {
                if idx + 1 == match_info.line_number {
                    let replacement = if is_in_markdown_table(line, &match_info.found_text) {
                        // For table cells, escape the | in the replacement wikilink
                        match_info.replacement.replace('|', "\\|")
                    } else {
                        match_info.replacement.clone()
                    };
                    line.replace(&match_info.found_text, &replacement)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        fs::write(full_path, updated_content)?;
    }

    Ok(())
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
            None,                           // ignore_rendered_text
            temp_dir.path().to_path_buf(),  // obsidian_path
            temp_dir.path().join("output"), // output_folder
            None,                           // simplify_wikilinks
        );

        let mut repo_info = ObsidianRepositoryInfo::default();

        // Create a wikilink for testing
        let wikilink = Wikilink {
            display_text: "Test Link".to_string(),
            target: "Test Link".to_string(),
            is_alias: false,
        };

        let compiled = CompiledWikilink::new(
            regex::Regex::new(r"(?i)\bTest Link\b").unwrap(),
            wikilink,
        );

        repo_info.all_wikilinks.insert(compiled);
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
            None,                  // back_populate_file_count
            do_not_back_populate,  // do_not_back_populate
            None,                  // ignore_folders
            None,                  // ignore_rendered_text
            temp_dir.path().to_path_buf(), // obsidian_path
            temp_dir.path().join("output"), // output_folder
            None,                  // simplify_wikilinks
        )
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
        println!("Testing position within wikilink:");
        println!("Line: {}", complex_line);
        println!("Position 43 character: {}", &complex_line[43..44]);

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
    fn test_wikilink_exact_positions() {
        let line = "[[Oleksiy Blavat|Oleksiy]]";
        // Test every position in the wikilink
        for i in 0..line.len() {
            assert!(
                is_within_wikilink(line, i),
                "Position {} should be within wikilink: '{}'",
                i,
                &line[i..i + 1]
            );
        }
    }

    #[test]
    fn test_find_back_populate_matches() {
        let (temp_dir, config, mut repo_info) = create_test_environment();

        // Create a test file with potential matches
        let file_content = "Here is Test Link without brackets\nThis [[Test Link]] is already linked\nAnother Test Link here";
        let file_path = temp_dir.path().join("test.md");
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", file_content).unwrap();

        repo_info
            .markdown_files
            .insert(file_path, MarkdownFileInfo::new());

        // Convert HashSet to sorted Vec
        let sorted_wikilinks: Vec<&CompiledWikilink> = repo_info.all_wikilinks.iter().collect();

        let matches =
            find_all_back_populate_matches(&config, &repo_info, &sorted_wikilinks).unwrap();

        assert_eq!(
            matches.len(),
            2,
            "Should find two matches needing back population"
        );
        assert_eq!(matches[0].line_number, 1);
        assert_eq!(matches[1].line_number, 3);
        assert!(matches.iter().all(|m| m.replacement == "[[Test Link]]"));
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
        let sorted_wikilinks: Vec<&CompiledWikilink> = repo_info.all_wikilinks.iter().collect();

        let matches =
            find_all_back_populate_matches(&config, &repo_info, &sorted_wikilinks).unwrap();

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
        let sorted_wikilinks: Vec<&CompiledWikilink> = repo_info.all_wikilinks.iter().collect();

        let matches =
            find_all_back_populate_matches(&config, &repo_info, &sorted_wikilinks).unwrap();

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
        let sorted_wikilinks: Vec<&CompiledWikilink> = repo_info.all_wikilinks.iter().collect();

        // Find matches
        let matches =
            find_all_back_populate_matches(&config, &repo_info, &sorted_wikilinks).unwrap();

        // Create a config that allows changes
        let config_with_changes = ValidatedConfig::new(
            true,
            None,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
            None,
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

        let compiled1 = CompiledWikilink::new(
            regex::Regex::new(r"(?i)\bKyri\b").unwrap(),
            wikilink1,
        );
        let compiled2 = CompiledWikilink::new(
            regex::Regex::new(r"(?i)\bKyri\b").unwrap(),
            wikilink2,
        );

        repo_info.all_wikilinks.insert(compiled1);
        repo_info.all_wikilinks.insert(compiled2);

        // Convert HashSet to sorted Vec
        let sorted_wikilinks: Vec<&CompiledWikilink> = repo_info.all_wikilinks.iter().collect();

        let matches =
            find_all_back_populate_matches(&config, &repo_info, &sorted_wikilinks).unwrap();

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

        let compiled = CompiledWikilink::new(
            regex::Regex::new(r"(?i)\bcheese\b").unwrap(),
            wikilink,
        );

        let compiled_ref = &compiled;
        let sorted_wikilinks = &[compiled_ref][..];
        let mut matches = Vec::new();

        let config = create_test_config(
            &temp_dir,
            false,
            Some(vec!["[[mozzarella]] cheese".to_string()]),
        );

        // Test line with excluded pattern
        let line = "- 1 1/2 cup [[mozzarella]] cheese shredded";
        process_line(
            0,
            line,
            &file_path,
            line,
            sorted_wikilinks,
            &config,
            &mut matches,
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
            line,
            sorted_wikilinks,
            &config,
            &mut matches,
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

        let compiled = CompiledWikilink::new(

            regex::Regex::new(r"(?i)\bWill\b").unwrap(),
            wikilink,
        );

        repo_info.all_wikilinks.clear();
        repo_info.all_wikilinks.insert(compiled);

        // Create a test file with its own name
        let content = "Will is mentioned here but should not be replaced";
        let file_path = temp_dir.path().join("Will.md");
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        repo_info
            .markdown_files
            .insert(file_path, MarkdownFileInfo::new());

        let sorted_wikilinks: Vec<&CompiledWikilink> = repo_info.all_wikilinks.iter().collect();
        let matches =
            find_all_back_populate_matches(&config, &repo_info, &sorted_wikilinks).unwrap();

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
            find_all_back_populate_matches(&config, &repo_info, &sorted_wikilinks).unwrap();

        assert_eq!(matches.len(), 1, "Should find match on other pages");
    }

    #[test]
    fn test_should_create_match_in_table() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Test simple table cell match
        assert!(should_create_match(
            "| Test Link | description |",
            2,
            "Test Link",
            0,
            "| Test Link | description |",
            &file_path
        ));

        // Test match in table with existing wikilinks
        assert!(should_create_match(
            "| Test Link | [[Other]] |",
            2,
            "Test Link",
            0,
            "| Test Link | [[Other]] |",
            &file_path
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
            line_text: "Test Link|Sample text".into(),
            found_text: "Test Link".into(),
            replacement: "[[Test Link|Another Name]]".into(),
            position: 0,
        }];

        let config = ValidatedConfig::new(
            true,
            None,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
            None,
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
            },
            BackPopulateMatch {
                file_path: "test.md".into(),
                line_number: 5,
                line_text: "|Test Link|Sample|".into(),
                found_text: "Test Link".into(),
                replacement: "[[Test Link|Display]]".into(),
                position: 1,
            },
        ];

        let config = ValidatedConfig::new(
            true,
            None,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
            None,
        );

        apply_back_populate_changes(&config, &matches).unwrap();

        let updated_content = fs::read_to_string(&file_path).unwrap();
        assert!(updated_content.contains("Regular [[Test Link]] here"));
        assert!(updated_content.contains("|[[Test Link\\|Display]]|Sample|"));
    }

    #[test]
    fn test_case_insensitive_matching() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        let content = "# Test\nThis test LINK here\nAlso test Link and TEST link";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        let wikilink = Wikilink {
            display_text: "Test Link".to_string(),
            target: "Test Link".to_string(),
            is_alias: false,
        };

        let compiled = compile_wikilink(wikilink);

        // Test various case combinations
        assert!(compiled.regex.is_match("test link"));
        assert!(compiled.regex.is_match("Test Link"));
        assert!(compiled.regex.is_match("TEST LINK"));
        assert!(compiled.regex.is_match("tEsT lInK"));
    }

    #[test]
    fn test_case_insensitive_boundary_matching() {
        let wikilink = Wikilink {
            display_text: "Test Link".to_string(),
            target: "Test Link".to_string(),
            is_alias: false,
        };
        let compiled = compile_wikilink(wikilink);

        // Should match
        assert!(compiled.regex.is_match("Here is test link."));
        assert!(compiled.regex.is_match("(TEST LINK)"));
        assert!(compiled.regex.is_match("test link;"));
        assert!(compiled.regex.is_match("[test link]"));

        // Should not match
        assert!(!compiled.regex.is_match("testlink"));
        assert!(!compiled.regex.is_match("atestlink"));
        assert!(!compiled.regex.is_match("testlinka"));
        assert!(!compiled.regex.is_match("my-test-link"));
    }

    #[test]
    fn test_case_insensitive_in_tables() {
        let wikilink = Wikilink {
            display_text: "Test Link".to_string(),
            target: "Test Link".to_string(),
            is_alias: false,
        };
        let compiled = compile_wikilink(wikilink);

        // Test table cell matches
        assert!(compiled.regex.is_match("| test link |"));
        assert!(compiled.regex.is_match("|TEST LINK|"));
        assert!(compiled
            .regex
            .is_match("| Test Link |description|"));

        // Test with escaped pipes
        assert!(compiled
            .regex
            .is_match("| test link \\| description |"));
    }
    #[test]
    fn test_case_preservation_with_alias() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        let content = "- send josh a clip";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // Create wikilink with alias
        let wikilink = Wikilink {
            display_text: "josh".to_string(),
            target: "Joshua Strayhorn".to_string(),
            is_alias: true,
        };
        let compiled = compile_wikilink(wikilink);

        let config = ValidatedConfig::new(
            false,
            None,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
            None,
        );

        // Test matching
        let match_result = find_next_match(&compiled, content, 0, 0, &file_path, content, &config)
            .unwrap()
            .unwrap();

        assert_eq!(match_result.replacement, "[[Joshua Strayhorn|josh]]");
    }

    #[test]
    fn test_case_mismatch_creates_alias() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        let content = "Configure Apple Home settings";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // Create wikilink without alias
        let wikilink = Wikilink {
            display_text: "apple home".to_string(),
            target: "apple home".to_string(),
            is_alias: false,
        };
        let compiled = compile_wikilink(wikilink);

        let config = ValidatedConfig::new(
            false,
            None,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
            None,
        );

        // Test matching
        let match_result = find_next_match(&compiled, content, 0, 0, &file_path, content, &config)
            .unwrap()
            .unwrap();

        assert_eq!(match_result.replacement, "[[apple home|Apple Home]]");
    }

    #[test]
    fn test_exact_case_match_no_alias() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        let content = "Configure apple home settings";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // Create wikilink without alias
        let wikilink = Wikilink {
            display_text: "apple home".to_string(),
            target: "apple home".to_string(),
            is_alias: false,
        };
        let compiled = compile_wikilink(wikilink);

        let config = ValidatedConfig::new(
            false,
            None,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
            None,
        );

        // Test matching
        let match_result = find_next_match(&compiled, content, 0, 0, &file_path, content, &config)
            .unwrap()
            .unwrap();

        assert_eq!(match_result.replacement, "[[apple home]]");
    }

    #[test]
    fn test_mixed_case_scenarios() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        let content = "Testing\n- josh likes apples\n- JOSH ate lunch\n- Josh went home";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        // Create wikilink with alias
        let wikilink = Wikilink {
            display_text: "josh".to_string(),
            target: "Joshua Strayhorn".to_string(),
            is_alias: true,
        };
        let compiled = compile_wikilink(wikilink);

        let config = ValidatedConfig::new(
            false,
            None,
            None,
            None,
            None,
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
            None,
        );

        // Test each line separately
        let lines: Vec<&str> = content.lines().collect();

        // Test "josh likes apples"
        let match1 = find_next_match(&compiled, lines[1], 0, 1, &file_path, content, &config)
            .unwrap()
            .unwrap();
        assert_eq!(match1.replacement, "[[Joshua Strayhorn|josh]]");

        // Test "JOSH ate lunch"
        let match2 = find_next_match(&compiled, lines[2], 0, 2, &file_path, content, &config)
            .unwrap()
            .unwrap();
        assert_eq!(match2.replacement, "[[Joshua Strayhorn|JOSH]]");

        // Test "Josh went home"
        let match3 = find_next_match(&compiled, lines[3], 0, 3, &file_path, content, &config)
            .unwrap()
            .unwrap();
        assert_eq!(match3.replacement, "[[Joshua Strayhorn|Josh]]");
    }

    #[test]
    fn test_case_insensitive_exclusion() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let wikilink = Wikilink {
            display_text: "cheese".to_string(),
            target: "fromage".to_string(),
            is_alias: true,
        };

        let compiled = CompiledWikilink::new(
            regex::Regex::new(r"(?i)\bcheese\b").unwrap(),
            wikilink,
        );

        let compiled_ref = &compiled;
        let sorted_wikilinks = &[compiled_ref][..];
        let mut matches = Vec::new();

        let config = create_test_config(
            &temp_dir,
            false,
            Some(vec!["[[mozzarella]] cheese".to_string()]),
        );

        // Test with different casings
        let test_cases = vec![
            "- 1 1/2 cup [[mozzarella]] cheese shredded",
            "- 1 1/2 cup [[Mozzarella]] Cheese shredded",
            "- 1 1/2 cup [[MOZZARELLA]] CHEESE shredded",
        ];

        for line in test_cases {
            matches.clear();
            process_line(
                0,
                line,
                &file_path,
                line,
                sorted_wikilinks,
                &config,
                &mut matches,
            )
            .unwrap();

            assert_eq!(
                matches.len(),
                0,
                "Match should be excluded regardless of case: {}",
                line
            );
        }

        // Test that other cheese references still match
        let line = "I love Cheese on my pizza";
        matches.clear();
        process_line(
            0,
            line,
            &file_path,
            line,
            sorted_wikilinks,
            &config,
            &mut matches,
        )
        .unwrap();

        assert_eq!(matches.len(), 1, "Match should be included");
        assert_eq!(matches[0].found_text, "Cheese");
    }
}

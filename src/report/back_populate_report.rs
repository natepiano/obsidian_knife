use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::Path;

use crate::constants::*;
use crate::markdown_file_info::{BackPopulateMatch, MarkdownFileInfo};
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::report::{
    escape_brackets, escape_pipe, highlight_matches, ReportDefinition, ReportWriter,
};
use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use crate::wikilink::ToWikilink;

struct BackPopulateTable {
    display_text: String,
    total_occurrences: usize,
    file_count: usize,
}

impl ReportDefinition for BackPopulateTable {
    type Item = BackPopulateMatch;

    fn headers(&self) -> Vec<&str> {
        vec![
            "file name",
            "line",
            TEXT,
            OCCURRENCES,
            WILL_REPLACE_WITH,
            SOURCE_TEXT,
        ]
    }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![
            ColumnAlignment::Left,
            ColumnAlignment::Right,
            ColumnAlignment::Left,
            ColumnAlignment::Center,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]
    }

    fn build_rows(&self, items: &[Self::Item], _: Option<&ValidatedConfig>) -> Vec<Vec<String>> {
        let consolidated = consolidate_matches(items);
        let mut table_rows = Vec::new();

        for m in consolidated {
            let file_path = Path::new(&m.file_path);
            let file_stem = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

            for line_info in m.line_info {
                let highlighted_line = highlight_matches(
                    &line_info.line_text,
                    &line_info.positions,
                    self.display_text.len(),
                );

                let replacement = if m.in_markdown_table {
                    m.replacement.clone()
                } else {
                    escape_pipe(&m.replacement)
                };

                table_rows.push(vec![
                    file_stem.to_wikilink(),
                    line_info.line_number.to_string(),
                    escape_pipe(&highlighted_line),
                    line_info.positions.len().to_string(),
                    replacement.clone(),
                    escape_brackets(&replacement),
                ]);
            }
        }

        table_rows
    }

    fn title(&self) -> Option<String> {
        let stats = DescriptionBuilder::new()
            .pluralize_with_count(Phrase::Time(self.total_occurrences))
            .text(IN)
            .pluralize_with_count(Phrase::Time(self.file_count))
            .build();

        let title = DescriptionBuilder::new()
            .text(FOUND)
            .no_space(COLON)
            .quoted_text(&self.display_text)
            .parenthetical_text(&stats)
            .build();

        Some(title)
    }

    fn description(&self, _items: &[Self::Item]) -> String {
        String::new()
    }

    fn level(&self) -> &'static str {
        LEVEL3
    }
}

impl ObsidianRepositoryInfo {
    pub fn write_back_populate_report(
        &self,
        files_to_persist: &[&MarkdownFileInfo],
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let matches = self.markdown_files.unambiguous_matches();

        // Skip if no matches
        if matches.is_empty() {
            return Ok(());
        }

        writer.writeln(LEVEL2, MATCHES_UNAMBIGUOUS)?;
        let header_message = DescriptionBuilder::new()
            .text(BACK_POPULATE)
            .number(self.wikilinks_sorted.len())
            .text(WIKILINKS)
            .build();
        writer.writeln("", &header_message)?;

        let unique_files: HashSet<String> =
            matches.iter().map(|m| m.relative_path.clone()).collect();

        let header_message = DescriptionBuilder::new()
            .pluralize_with_count(Phrase::Match(matches.len()))
            .text(IN)
            .pluralize_with_count(Phrase::File(unique_files.len()))
            .text(WILL_BE_BACK_POPULATED)
            .build();

        writer.writeln("", &header_message)?;

        // Group matches by display text (case-insensitive)
        let mut matches_by_text: HashMap<String, Vec<BackPopulateMatch>> = HashMap::new();
        for match_info in matches {
            let key = match_info.found_text.to_lowercase();
            matches_by_text
                .entry(key)
                .or_default()
                .push(match_info.clone());
        }

        // Sort keys for consistent output
        let mut sorted_keys: Vec<String> = matches_by_text.keys().cloned().collect();
        sorted_keys.sort();

        // Create and write a table for each group
        for key in sorted_keys {
            let group_matches = &matches_by_text[&key];
            let display_text = &group_matches[0].found_text;
            let total_occurrences = group_matches.len();
            let file_paths: HashSet<String> = group_matches
                .iter()
                .map(|m| m.relative_path.clone())
                .collect();

            let table = BackPopulateTable {
                display_text: display_text.clone(),
                total_occurrences,
                file_count: file_paths.len(),
            };

            let report = ReportWriter::new(group_matches.clone());
            report.write(&table, writer)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ConsolidatedMatch {
    file_path: String,
    line_info: Vec<LineInfo>,
    replacement: String,
    in_markdown_table: bool,
}

#[derive(Debug, Clone)]
struct LineInfo {
    line_number: usize,
    line_text: String,
    positions: Vec<usize>,
}

fn consolidate_matches(matches: &[BackPopulateMatch]) -> Vec<ConsolidatedMatch> {
    let mut line_map: HashMap<(String, usize), LineInfo> = HashMap::new();
    let mut file_info: HashMap<String, (String, bool)> = HashMap::new();

    for match_info in matches {
        let key = (match_info.relative_path.clone(), match_info.line_number);

        let line_info = line_map.entry(key).or_insert(LineInfo {
            line_number: match_info.line_number + match_info.frontmatter_line_count,
            line_text: match_info.line_text.clone(),
            positions: Vec::new(),
        });
        line_info.positions.push(match_info.position);

        file_info.insert(
            match_info.relative_path.clone(),
            (match_info.replacement.clone(), match_info.in_markdown_table),
        );
    }

    let mut result = Vec::new();
    for (file_path, (replacement, in_markdown_table)) in file_info {
        let mut file_lines: Vec<LineInfo> = line_map
            .iter()
            .filter(|((path, _), _)| path == &file_path)
            .map(|((_, _), line_info)| line_info.clone())
            .collect();

        file_lines.sort_by_key(|line| line.line_number);

        result.push(ConsolidatedMatch {
            file_path,
            line_info: file_lines,
            replacement,
            in_markdown_table,
        });
    }

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

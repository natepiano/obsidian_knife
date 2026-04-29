use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::ffi::OsStr;
use std::path::Path;

use super::constants::TABLE_HEADER_FILE_NAME;
use super::constants::TABLE_HEADER_LINE;
use super::orchestration;
use super::report_writer::ReportDefinition;
use super::report_writer::ReportWriter;
use crate::constants::BACK_POPULATE;
use crate::constants::COLON;
use crate::constants::FOUND;
use crate::constants::IN;
use crate::constants::LEVEL2;
use crate::constants::LEVEL3;
use crate::constants::MATCHES;
use crate::constants::OCCURRENCES;
use crate::constants::SOURCE_TEXT;
use crate::constants::TEXT;
use crate::constants::WIKILINKS;
use crate::constants::WILL_BE_BACK_POPULATED;
use crate::constants::WILL_REPLACE_WITH;
use crate::description_builder::DescriptionBuilder;
use crate::description_builder::Phrase;
use crate::markdown_file::BackPopulateMatch;
use crate::markdown_file::MatchContext;
use crate::obsidian_repository::ObsidianRepository;
use crate::utils;
use crate::utils::ColumnAlignment;
use crate::utils::OutputFileWriter;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::ToWikilink;

struct BackPopulateTable {
    display_text:      String,
    total_occurrences: usize,
    file_count:        usize,
}

impl ReportDefinition for BackPopulateTable {
    type Item = BackPopulateMatch;

    fn headers(&self) -> Vec<&str> {
        vec![
            TABLE_HEADER_FILE_NAME,
            TABLE_HEADER_LINE,
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

    fn build_rows(
        &self,
        items: &[Self::Item],
        _: Option<&ValidatedConfig>,
    ) -> anyhow::Result<Vec<Vec<String>>> {
        let consolidated = consolidate_matches(items);
        let mut table_rows = Vec::new();

        for entry in consolidated {
            let file_path = Path::new(&entry.file_path);
            let file_stem = file_path.file_stem().and_then(OsStr::to_str).unwrap_or("");

            for line_info in entry.line_info {
                let highlighted_line = orchestration::highlight_matches(
                    &line_info.line_text,
                    &line_info.positions,
                    self.display_text.len(),
                );

                let replacement = if entry.match_context == MatchContext::MarkdownTable {
                    entry.replacement.clone()
                } else {
                    utils::escape_pipe(&entry.replacement)
                };

                table_rows.push(vec![
                    file_stem.to_wikilink(),
                    line_info.line_number.to_string(),
                    utils::escape_pipe(&highlighted_line),
                    line_info.positions.len().to_string(),
                    replacement.clone(),
                    utils::escape_brackets(&replacement),
                ]);
            }
        }

        Ok(table_rows)
    }

    fn title(&self) -> Option<String> {
        let stats = DescriptionBuilder::new()
            .pluralize_with_count(Phrase::Time(self.total_occurrences))
            .text(IN)
            .pluralize_with_count(Phrase::File(self.file_count))
            .build();

        let title = DescriptionBuilder::new()
            .text(FOUND)
            .no_space(COLON)
            .quoted_text(&self.display_text)
            .parenthetical_text(&stats)
            .build();

        Some(title)
    }

    fn description(&self, _: &[Self::Item]) -> String { String::new() }

    fn level(&self) -> &'static str { LEVEL3 }
}

impl ObsidianRepository {
    pub(super) fn write_back_populate_report(
        &self,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let matches = self.markdown_files.files_to_persist().unambiguous_matches();

        writer.writeln(LEVEL2, MATCHES)?;
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
    file_path:     String,
    line_info:     Vec<LineInfo>,
    replacement:   String,
    match_context: MatchContext,
}

#[derive(Debug, Clone)]
struct LineInfo {
    line_number: usize,
    line_text:   String,
    positions:   Vec<usize>,
}

fn consolidate_matches(matches: &[BackPopulateMatch]) -> Vec<ConsolidatedMatch> {
    let mut line_map: HashMap<(String, usize), LineInfo> = HashMap::new();
    let mut file_info: HashMap<String, (String, MatchContext)> = HashMap::new();

    for match_info in matches {
        let key = (match_info.relative_path.clone(), match_info.line_number);

        let line_info = line_map.entry(key).or_insert_with(|| LineInfo {
            line_number: match_info.line_number,
            line_text:   match_info.line_text.clone(),
            positions:   Vec::new(),
        });
        line_info.positions.push(match_info.position);

        file_info.insert(
            match_info.relative_path.clone(),
            (
                match_info.replacement.clone(),
                match_info.match_context.clone(),
            ),
        );
    }

    let mut result = Vec::new();
    for (file_path, (replacement, match_context)) in file_info {
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
            match_context,
        });
    }

    result.sort_by(|a, b| {
        let file_a = Path::new(&a.file_path)
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("");
        let file_b = Path::new(&b.file_path)
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("");
        file_a.cmp(file_b)
    });

    result
}

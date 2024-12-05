use crate::constants::*;
use crate::markdown_file_info::BackPopulateMatch;
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::report::{escape_pipe, highlight_matches, ReportDefinition, ReportWriter};
use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use crate::wikilink::ToWikilink;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::Path;

struct AmbiguousMatchesTable {
    display_text: String,
    targets: HashSet<String>,
    sorted_targets: Vec<String>,
}

impl ReportDefinition for AmbiguousMatchesTable {
    type Item = BackPopulateMatch;

    fn headers(&self) -> Vec<&str> {
        vec!["file name", "line", TEXT, OCCURRENCES]
    }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![
            ColumnAlignment::Left,
            ColumnAlignment::Right,
            ColumnAlignment::Left,
            ColumnAlignment::Center,
        ]
    }

    fn build_rows(
        &self,
        items: &[Self::Item],
        _config: Option<&ValidatedConfig>,
    ) -> Vec<Vec<String>> {
        // Group matches by file path and line number for consolidation
        let mut line_map: HashMap<(String, usize), (String, Vec<usize>)> = HashMap::new();

        // Group matches by file and line
        for match_info in items {
            let key = (
                match_info.relative_path.clone(),
                match_info.line_number + match_info.frontmatter_line_count,
            );

            let entry = line_map
                .entry(key)
                .or_insert((match_info.line_text.clone(), Vec::new()));
            entry.1.push(match_info.position);
        }

        // Convert to sorted rows
        let mut rows = Vec::new();
        for ((file_path, line_number), (line_text, positions)) in line_map {
            let file_path = Path::new(&file_path);
            let file_stem = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();

            let highlighted_line =
                highlight_matches(&line_text, &positions, self.display_text.len());

            rows.push(vec![
                file_stem.to_string(),
                line_number.to_string(),
                escape_pipe(&highlighted_line),
                positions.len().to_string(),
            ]);
        }

        // Sort rows by file name and line number
        rows.sort_by(|a, b| {
            let file_cmp = a[0].to_lowercase().cmp(&b[0].to_lowercase());
            if file_cmp != std::cmp::Ordering::Equal {
                return file_cmp;
            }
            a[1].parse::<usize>()
                .unwrap_or(0)
                .cmp(&b[1].parse::<usize>().unwrap_or(0))
        });

        rows
    }

    fn title(&self) -> Option<String> {
        Some(format!(
            "\"{}\" matches {} targets:",
            self.display_text,
            self.targets.len()
        ));
        Some(
            DescriptionBuilder::new()
                .quoted_text(&self.display_text)
                .text(MATCHES)
                .pluralize_with_count(Phrase::Target(self.targets.len()))
                .no_space(COLON)
                .build(),
        )
    }

    fn description(&self, items: &[Self::Item]) -> String {
        let mut result = String::new();

        // Write out targets first
        for target in &self.sorted_targets {
            result.push_str(&format!(
                "- \\[\\[{}|{}]]\n",
                target.to_wikilink(),
                self.display_text
            ));
        }

        // Add original description
        let unique_files: HashSet<String> = items.iter().map(|m| m.relative_path.clone()).collect();

        let stats = DescriptionBuilder::new()
            .pluralize_with_count(Phrase::Time(items.len()))
            .text(IN)
            .pluralize_with_count(Phrase::File(unique_files.len()))
            .build();

        let stats_message = DescriptionBuilder::new()
            .text(LEVEL4)
            .text(FOUND)
            .no_space(COLON)
            .quoted_text(&self.display_text)
            .parenthetical_text(&stats)
            .build();

        result.push_str(&stats_message);

        result
    }

    fn level(&self) -> &'static str {
        LEVEL3
    }
}

impl ObsidianRepositoryInfo {
    pub fn write_ambiguous_matches_report(
        &self,
        writer: &OutputFileWriter,
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
        let mut matches_by_text: HashMap<String, Vec<BackPopulateMatch>> = HashMap::new();

        // First pass: collect all matches
        for markdown_file in self.markdown_files.iter() {
            for match_info in &markdown_file.matches.ambiguous {
                let key = match_info.found_text.to_lowercase();
                matches_by_text
                    .entry(key)
                    .or_default()
                    .push(match_info.clone());
            }
        }

        // Second pass: collect targets for each found text
        let mut targets_by_text: HashMap<String, HashSet<String>> = HashMap::new();
        for wikilink in &self.wikilinks_sorted {
            if let Some(matches) = matches_by_text.get(&wikilink.display_text.to_lowercase()) {
                targets_by_text
                    .entry(matches[0].found_text.clone())
                    .or_default()
                    .insert(wikilink.target.clone());
            }
        }

        // Sort the keys for consistent output
        let mut sorted_keys: Vec<_> = matches_by_text.keys().cloned().collect();
        sorted_keys.sort();

        // Write a table for each group of matches
        for key in sorted_keys {
            let matches = matches_by_text.get(&key).unwrap();
            let display_text = &matches[0].found_text;
            let targets = targets_by_text
                .get(display_text)
                .unwrap_or(&HashSet::new())
                .clone();

            // collect out all possible targets to display in the description
            let mut sorted_targets: Vec<String> = targets.iter().map(|s| s.to_string()).collect();
            sorted_targets.sort();

            let table = AmbiguousMatchesTable {
                display_text: display_text.clone(),
                targets,
                sorted_targets,
            };

            let report = ReportWriter::new(matches.clone());
            report.write(&table, writer)?;
        }

        Ok(())
    }
}

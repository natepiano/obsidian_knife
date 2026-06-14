use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::ffi::OsStr;
use std::fmt::Write;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Result as AnyhowResult;

use super::constants::TABLE_HEADER_FILE_NAME;
use super::constants::TABLE_HEADER_LINE;
use super::support;
use super::writer::ReportDefinition;
use super::writer::ReportWriter;
use crate::constants::COLON;
use crate::constants::FOUND;
use crate::constants::IN;
use crate::constants::LEVEL1;
use crate::constants::LEVEL2;
use crate::constants::LEVEL3;
use crate::constants::MATCHES;
use crate::constants::MATCHES_AMBIGUOUS;
use crate::constants::OCCURRENCES;
use crate::constants::OPENING_WIKILINK;
use crate::constants::REFERENCES_TO;
use crate::constants::TEXT;
use crate::constants::YOU_HAVE_TO_FIX_THESE_YOURSELF;
use crate::description_builder::DescriptionBuilder;
use crate::markdown_file::BackPopulateMatch;
use crate::obsidian_repository::ObsidianRepository;
use crate::output_file_writer::ColumnAlignment;
use crate::output_file_writer::OutputFileWriter;
use crate::phrase::Phrase;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::ToWikilink;

struct AmbiguousMatchesTable {
    display_text:   String,
    targets:        HashSet<String>,
    sorted_targets: Vec<String>,
}

impl ReportDefinition for AmbiguousMatchesTable {
    type Item = BackPopulateMatch;

    fn headers(&self) -> Vec<&str> {
        vec![TABLE_HEADER_FILE_NAME, TABLE_HEADER_LINE, TEXT, OCCURRENCES]
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
        _: Option<&ValidatedConfig>,
    ) -> AnyhowResult<Vec<Vec<String>>> {
        // `line_map` consolidates `BackPopulateMatch` values by path and line.
        let mut line_map: HashMap<(String, usize), (String, Vec<usize>)> = HashMap::new();

        // line_map groups BackPopulateMatch positions by relative path and line number.
        for match_info in items {
            let key = (match_info.relative_path.clone(), match_info.line_number);

            let entry = line_map
                .entry(key)
                .or_insert_with(|| (match_info.line_text.clone(), Vec::new()));
            entry.1.push(match_info.position);
        }

        let mut rows = Vec::new();
        for ((file_path, line_number), (line_text, positions)) in line_map {
            let file_path = Path::new(&file_path);
            let file_stem = file_path
                .file_stem()
                .and_then(OsStr::to_str)
                .unwrap_or_default();

            let highlighted_line =
                support::highlight_matches(&line_text, &positions, self.display_text.len());

            rows.push(vec![
                file_stem.to_wikilink(),
                line_number.to_string(),
                support::escape_pipe(&highlighted_line),
                positions.len().to_string(),
            ]);
        }

        // Rows sort by wikilink target, then source line number.
        rows.sort_by(|a, b| {
            let file_cmp = a[0].to_lowercase().cmp(&b[0].to_lowercase());
            if file_cmp == Ordering::Equal {
                a[1].parse::<usize>()
                    .unwrap_or(0)
                    .cmp(&b[1].parse::<usize>().unwrap_or(0))
            } else {
                file_cmp
            }
        });

        Ok(rows)
    }

    fn title(&self) -> Option<String> {
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

        // `sorted_targets` form the target list before the source-line tables.
        for target in &self.sorted_targets {
            let _ = writeln!(
                result,
                "- \\[\\[{}|{}]]",
                target.to_wikilink(),
                self.display_text
            );
        }

        // DescriptionBuilder summarizes the source file count for this display text.
        let unique_files: HashSet<String> = items.iter().map(|m| m.relative_path.clone()).collect();

        let stats = DescriptionBuilder::new()
            .pluralize_with_count(Phrase::Time(items.len()))
            .text(IN)
            .pluralize_with_count(Phrase::File(unique_files.len()))
            .build();

        let stats_message = DescriptionBuilder::new()
            .text(LEVEL3)
            .text(FOUND)
            .no_space(COLON)
            .quoted_text(&self.display_text)
            .parenthetical_text(&stats)
            .text_with_newline("")
            .no_space(YOU_HAVE_TO_FIX_THESE_YOURSELF)
            .build();

        result.push_str(&stats_message);

        result
    }

    fn level(&self) -> &'static str { LEVEL2 }
}

struct TargetLinesTable {
    target_text: String,
}

#[derive(Clone)]
struct TargetLine {
    file_path: PathBuf,
    number:    usize,
    text:      String,
}

impl ReportDefinition for TargetLinesTable {
    type Item = TargetLine;

    fn headers(&self) -> Vec<&str> { vec![TABLE_HEADER_FILE_NAME, TABLE_HEADER_LINE, TEXT] }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![
            ColumnAlignment::Left,
            ColumnAlignment::Right,
            ColumnAlignment::Left,
        ]
    }

    fn build_rows(
        &self,
        items: &[Self::Item],
        _: Option<&ValidatedConfig>,
    ) -> AnyhowResult<Vec<Vec<String>>> {
        let mut rows: Vec<Vec<String>> = items
            .iter()
            .map(|item| {
                let file_stem = item
                    .file_path
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .unwrap_or_default();

                vec![
                    file_stem.to_wikilink(),
                    item.number.to_string(),
                    support::escape_pipe(&item.text),
                ]
            })
            .collect();

        rows.sort_by(|a, b| {
            let file_cmp = a[0].to_lowercase().cmp(&b[0].to_lowercase());
            if file_cmp == Ordering::Equal {
                a[1].parse::<usize>()
                    .unwrap_or(0)
                    .cmp(&b[1].parse::<usize>().unwrap_or(0))
            } else {
                file_cmp
            }
        });

        Ok(rows)
    }

    fn title(&self) -> Option<String> {
        Some(
            DescriptionBuilder::new()
                .text(REFERENCES_TO)
                .text(&self.target_text.to_wikilink())
                .build(),
        )
    }

    fn description(&self, items: &[Self::Item]) -> String {
        let unique_files: HashSet<&PathBuf> = items.iter().map(|r| &r.file_path).collect();

        DescriptionBuilder::new()
            .text(FOUND)
            .pluralize_with_count(Phrase::Reference(items.len()))
            .text(IN)
            .pluralize_with_count(Phrase::File(unique_files.len()))
            .build()
    }

    fn level(&self) -> &'static str { LEVEL3 }
}

impl ObsidianRepository {
    pub(super) fn write_ambiguous_matches_report(
        &self,
        output_file_writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        output_file_writer.writeln(LEVEL1, MATCHES_AMBIGUOUS)?;

        // matches_by_text groups BackPopulateMatch values by lowercase display text.
        let mut matches_by_text: HashMap<String, Vec<BackPopulateMatch>> = HashMap::new();

        for markdown_file in &self.markdown_files
        /* .files_to_persist() */
        {
            for match_info in &markdown_file.back_populate_matches.ambiguous {
                let key = match_info.found_text.to_lowercase();
                matches_by_text
                    .entry(key)
                    .or_default()
                    .push(match_info.clone());
            }
        }

        // targets_by_text maps found text to candidate Wikilink targets.
        let mut targets_by_text: HashMap<String, HashSet<String>> = HashMap::new();
        for wikilink in &self.wikilinks_sorted {
            if let Some(matches) = matches_by_text.get(&wikilink.display_text.to_lowercase()) {
                targets_by_text
                    .entry(matches[0].found_text.clone())
                    .or_default()
                    .insert(wikilink.target.clone());
            }
        }

        // sorted_keys gives report sections deterministic order.
        let mut sorted_keys: Vec<_> = matches_by_text.keys().cloned().collect();
        sorted_keys.sort();

        // AmbiguousMatchesTable renders one section per grouped display text.
        for key in sorted_keys {
            let Some(matches) = matches_by_text.get(&key) else {
                continue;
            };
            let display_text = &matches[0].found_text;
            let default_targets = HashSet::new();
            let targets = targets_by_text
                .get(display_text)
                .unwrap_or(&default_targets)
                .clone();

            // `sorted_targets` lists every target shown in the description.
            let mut sorted_targets: Vec<String> = targets.iter().cloned().collect();
            sorted_targets.sort();

            let ambiguous_matches_table = AmbiguousMatchesTable {
                display_text: display_text.clone(),
                targets,
                sorted_targets: sorted_targets.clone(),
            };

            let report_writer = ReportWriter::new(matches.clone());
            report_writer.write(&ambiguous_matches_table, output_file_writer)?;

            // `TargetLinesTable` renders source lines for each candidate target.
            for target in &sorted_targets {
                let lines = self.collect_target_lines(target);
                let target_lines_table = TargetLinesTable {
                    target_text: target.clone(),
                };
                let report_writer = ReportWriter::new(lines);
                report_writer.write(&target_lines_table, output_file_writer)?;
            }
        }

        Ok(())
    }

    fn collect_target_lines(&self, target: &str) -> Vec<TargetLine> {
        let target_lower = target.to_lowercase();

        self.markdown_files
            .iter()
            .filter(|file| {
                file.wikilinks
                    .valid
                    .iter()
                    .any(|w| w.target.to_lowercase() == target_lower)
            })
            .flat_map(|file| {
                let frontmatter_offset = file.frontmatter_line_count;
                file.content
                    .lines()
                    .enumerate()
                    .filter(|(_, line)| {
                        let line_lower = line.to_lowercase();
                        line_lower.contains(&format!("{OPENING_WIKILINK}{target_lower}"))
                    })
                    .map(move |(idx, line)| TargetLine {
                        file_path: file.path.clone(),
                        number:    frontmatter_offset + idx + 1,
                        text:      line.to_string(),
                    })
            })
            .collect()
    }
}

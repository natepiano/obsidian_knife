use std::cmp::Reverse;
use std::collections::HashMap;
use std::collections::HashSet;
use std::convert::AsRef;
use std::mem::take;
use std::path::Path;

use anyhow::Context as _;
use anyhow::Result as AnyhowResult;
use anyhow::bail;

use super::ObsidianRepository;
use super::constants::INVALID_UTF8_BOUNDARY_PREFIX;
use super::constants::MIN_AMBIGUOUS_TARGETS;
use super::constants::NESTED_PATTERN_WARNING;
use super::constants::TRIPLE_CLOSING_BRACKETS;
use super::constants::TRIPLE_OPENING_BRACKETS;
use super::constants::UNCLASSIFIED_MATCH_WARNING;
use super::constants::WIKILINKS_AUTOMATON_NOT_INITIALIZED;
use super::constants::WIKILINKS_AUTOMATON_NOT_INITIALIZED_DETAIL;
use crate::constants::NEWLINE;
use crate::markdown_file::BackPopulateMatch;
use crate::markdown_file::ImageLinkState;
use crate::markdown_file::MarkdownFile;
use crate::markdown_file::MatchType;
use crate::markdown_file::ReplaceableContent;
use crate::support::VecEnumFilter;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::Wikilink;

#[derive(Debug, Default)]
struct ChangeSet {
    match_types: Vec<MatchType>,
}

impl ChangeSet {
    fn merge(&mut self, match_type: MatchType) {
        if !self.contains(&match_type) {
            self.match_types.push(match_type);
        }
    }

    fn contains(&self, match_type: &MatchType) -> bool { self.match_types.contains(match_type) }
}

impl ObsidianRepository {
    pub fn identify_ambiguous_matches(&mut self) {
        // `target_map` records the canonical `Wikilink.target` for each lowercase target.
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

        // `MarkdownFile.back_populate_matches.unambiguous` is split into ambiguous
        // and still-unambiguous matches.
        for markdown_file in &mut self.markdown_files {
            // `matches_by_text` groups matches by lowercased `BackPopulateMatch.found_text`.
            let mut matches_by_text: HashMap<String, Vec<BackPopulateMatch>> = HashMap::new();

            // `file_matches` takes ownership of `markdown_file.back_populate_matches.unambiguous`.
            let file_matches = take(&mut markdown_file.back_populate_matches.unambiguous);
            for match_info in file_matches {
                let lower_found_text = match_info.found_text.to_lowercase();
                matches_by_text
                    .entry(lower_found_text)
                    .or_default()
                    .push(match_info);
            }

            // `matches_by_text` entries are classified through the `display_text_map` lookup.
            for (found_text_lower, text_matches) in matches_by_text {
                if let Some(targets) = display_text_map.get(&found_text_lower) {
                    if targets.len() >= MIN_AMBIGUOUS_TARGETS {
                        // `BackPopulateMatch` values with multiple targets move into
                        // `markdown_file.back_populate_matches.ambiguous`.
                        markdown_file
                            .back_populate_matches
                            .ambiguous
                            .extend(text_matches.clone());
                    } else {
                        // `text_matches` remains in
                        // `markdown_file.back_populate_matches.unambiguous`.
                        markdown_file
                            .back_populate_matches
                            .unambiguous
                            .extend(text_matches);
                    }
                } else {
                    // Missing `display_text_map` entries keep the `BackPopulateMatch`
                    // values unambiguous and emit `UNCLASSIFIED_MATCH_WARNING`.
                    println!(
                        "{UNCLASSIFIED_MATCH_WARNING} '{found_text_lower}' in file '{}'",
                        markdown_file.path.display()
                    );
                    markdown_file
                        .back_populate_matches
                        .unambiguous
                        .extend(text_matches);
                }
            }
        }
    }

    pub fn find_all_back_populate_matches(&mut self, config: &ValidatedConfig) -> AnyhowResult<()> {
        let automaton = self.wikilinks_automaton.as_ref().context(format!(
            "{WIKILINKS_AUTOMATON_NOT_INITIALIZED} — {WIKILINKS_AUTOMATON_NOT_INITIALIZED_DETAIL}"
        ))?;

        // Turn `wikilinks_sorted` into references.
        let sorted_wikilinks: Vec<&Wikilink> = self.wikilinks_sorted.iter().collect();

        self.markdown_files.process_files_for_back_populate_matches(
            config,
            &sorted_wikilinks,
            automaton,
        );
        Ok(())
    }

    pub fn apply_replaceable_matches(&mut self, operational_timezone: &str) -> AnyhowResult<()> {
        for markdown_file in &mut self.markdown_files {
            let has_replaceable_image_links = markdown_file.image_links.iter().any(|link| {
                matches!(
                    link.state,
                    ImageLinkState::Missing
                        | ImageLinkState::Duplicate { .. }
                        | ImageLinkState::Incompatible { .. }
                )
            });

            if !markdown_file.has_unambiguous_matches() && !has_replaceable_image_links {
                continue;
            }

            let sorted_replaceable_matches = Self::collect_replaceable_matches(markdown_file);

            if sorted_replaceable_matches.is_empty() {
                continue;
            }

            let mut updated_content = String::new();
            let mut content_line_number = 1;
            let mut change_set = ChangeSet::default();

            // Process line by line
            for (zero_based_idx, line) in markdown_file.content.lines().enumerate() {
                let current_content_line = zero_based_idx + 1;
                let absolute_line_number =
                    current_content_line + markdown_file.frontmatter_line_count;

                if content_line_number != current_content_line {
                    updated_content.push_str(line);
                    updated_content.push(NEWLINE);
                    continue;
                }

                // Collect matches for the current line
                let line_matches: Vec<&dyn ReplaceableContent> = sorted_replaceable_matches
                    .iter()
                    .filter(|m| m.line_number() == absolute_line_number)
                    .map(AsRef::as_ref) // Dereference Box to &dyn ReplaceableContent
                    .collect();

                if line_matches.is_empty() {
                    updated_content.push_str(line);
                    updated_content.push(NEWLINE);
                } else {
                    let updated_line =
                        apply_line_replacements(line, &line_matches, &markdown_file.path)?;

                    // Track which types of changes occurred
                    for line_match in &line_matches {
                        change_set.merge(line_match.match_type());
                    }

                    if !updated_line.is_empty() {
                        updated_content.push_str(&updated_line);
                        updated_content.push(NEWLINE);
                    }
                }
                content_line_number += 1;
            }

            // Update the content and mark file as modified
            markdown_file.content = updated_content.trim_end().to_string();

            if change_set.contains(&MatchType::BackPopulate) {
                markdown_file.mark_as_back_populated(operational_timezone)?;
            }
            if change_set.contains(&MatchType::ImageReference) {
                markdown_file.mark_image_reference_as_updated(operational_timezone)?;
            }
        }
        Ok(())
    }

    fn collect_replaceable_matches(
        markdown_file: &MarkdownFile,
    ) -> Vec<Box<dyn ReplaceableContent>> {
        let mut matches = Vec::new();

        // Add `BackPopulateMatch` values.
        matches.extend(
            markdown_file
                .back_populate_matches
                .unambiguous
                .iter()
                .cloned()
                .map(|m| Box::new(m) as Box<dyn ReplaceableContent>),
        );

        // Add `ImageLinkState` values that need replacement.
        matches.extend(
            markdown_file
                .image_links
                .filter_by_predicate(|state| {
                    matches!(
                        state,
                        ImageLinkState::Incompatible { .. }
                            | ImageLinkState::Duplicate { .. }
                            | ImageLinkState::Missing
                    )
                })
                .iter()
                .cloned()
                .map(|m| Box::new(m) as Box<dyn ReplaceableContent>),
        );

        // Sort by line number and reverse position
        matches.sort_by_key(|m| (m.line_number(), Reverse(m.position())));

        matches
    }
}

fn apply_line_replacements(
    line: &str,
    line_matches: &[&dyn ReplaceableContent],
    file_path: &Path,
) -> AnyhowResult<String> {
    let mut updated_line = line.to_string();

    // `sorted_matches` orders `ReplaceableContent` by descending `position`.
    let mut sorted_matches = line_matches.to_vec();
    sorted_matches.sort_by_key(|m| Reverse(m.position()));

    let has_image_replacement = sorted_matches
        .iter()
        .any(|m| m.match_type() == MatchType::ImageReference);

    // `match_info` replacements use right-to-left order.
    for match_info in sorted_matches {
        let start = match_info.position();
        let end = start + match_info.matched_text().len();

        // `updated_line.is_char_boundary` guards `replace_range` byte indexes.
        if !updated_line.is_char_boundary(start) || !updated_line.is_char_boundary(end) {
            bail!(
                "{INVALID_UTF8_BOUNDARY_PREFIX}{}, line {}: match {start}..{end} in \
                 {updated_line:?}, found {:?}",
                file_path.display(),
                match_info.line_number(),
                match_info.matched_text(),
            );
        }

        // `updated_line.replace_range` writes the `ReplaceableContent` replacement.
        updated_line.replace_range(start..end, &match_info.get_replacement());

        // `TRIPLE_OPENING_BRACKETS` and `TRIPLE_CLOSING_BRACKETS` flag nested patterns.
        if updated_line.contains(TRIPLE_OPENING_BRACKETS)
            || updated_line.contains(TRIPLE_CLOSING_BRACKETS)
        {
            eprintln!(
                "\n{NESTED_PATTERN_WARNING} '{}', line {}.\nCurrent line:\n{updated_line}\n",
                file_path.display(),
                match_info.line_number(),
            );
        }
    }

    // `ImageReference` replacements use `normalize_spaces` after trimming.
    Ok(if has_image_replacement {
        let trimmed = updated_line.trim();
        if trimmed.is_empty() {
            String::new()
        } else {
            normalize_spaces(trimmed)
        }
    } else {
        updated_line
    })
}

fn normalize_spaces(text: &str) -> String { text.split_whitespace().collect::<Vec<_>>().join(" ") }

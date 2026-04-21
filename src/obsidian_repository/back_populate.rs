use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use super::ObsidianRepository;
use crate::markdown_file::BackPopulateMatch;
use crate::markdown_file::ImageLinkState;
use crate::markdown_file::MarkdownFile;
use crate::markdown_file::MatchType;
use crate::markdown_file::ReplaceableContent;
use crate::utils::VecEnumFilter;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::Wikilink;

impl ObsidianRepository {
    pub fn identify_ambiguous_matches(&mut self) {
        // Create `target` and `display_text` maps as before...
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

        // Process each file's matches.
        for markdown_file in &mut self.markdown_files {
            // Group matches by their lowercased `found_text` within this file.
            let mut matches_by_text: HashMap<String, Vec<BackPopulateMatch>> = HashMap::new();

            // Drain matches from the file into a temporary map.
            let file_matches = std::mem::take(&mut markdown_file.matches.unambiguous);
            for match_info in file_matches {
                let lower_found_text = match_info.found_text.to_lowercase();
                matches_by_text
                    .entry(lower_found_text)
                    .or_default()
                    .push(match_info);
            }

            // Process each group of matches.
            for (found_text_lower, text_matches) in matches_by_text {
                if let Some(targets) = display_text_map.get(&found_text_lower) {
                    if targets.len() > 1 {
                        // This is an ambiguous match.
                        // Add it to the file's ambiguous collection.
                        markdown_file.matches.ambiguous.extend(text_matches.clone());
                    } else {
                        // Unambiguous matches go back into the `markdown_file`.
                        markdown_file.matches.unambiguous.extend(text_matches);
                    }
                } else {
                    // Handle unclassified matches.
                    println!(
                        "[WARNING] Found unclassified matches for '{found_text_lower}' in file '{}'",
                        markdown_file.path.display()
                    );
                    markdown_file.matches.unambiguous.extend(text_matches);
                }
            }
        }
    }

    #[allow(
        clippy::expect_used,
        reason = "wikilinks_automaton is always initialized in ObsidianRepository::new"
    )]
    pub fn find_all_back_populate_matches(&mut self, config: &ValidatedConfig) {
        let automaton = self
            .wikilinks_automaton
            .as_ref()
            .expect("Wikilinks automaton should be initialized");

        // Turn `wikilinks_sorted` into references.
        let sorted_wikilinks: Vec<&Wikilink> = self.wikilinks_sorted.iter().collect();

        self.markdown_files.process_files_for_back_populate_matches(
            config,
            &sorted_wikilinks,
            automaton,
        );
    }

    pub fn apply_replaceable_matches(&mut self, operational_timezone: &str) {
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
            let mut has_back_populate_changes = false;
            let mut has_image_reference_changes = false;

            // Process line by line
            for (zero_based_idx, line) in markdown_file.content.lines().enumerate() {
                let current_content_line = zero_based_idx + 1;
                let absolute_line_number =
                    current_content_line + markdown_file.frontmatter_line_count;

                if content_line_number != current_content_line {
                    updated_content.push_str(line);
                    updated_content.push('\n');
                    continue;
                }

                // Collect matches for the current line
                let line_matches: Vec<&dyn ReplaceableContent> = sorted_replaceable_matches
                    .iter()
                    .filter(|m| m.line_number() == absolute_line_number)
                    .map(std::convert::AsRef::as_ref) // Dereference Box to &dyn ReplaceableContent
                    .collect();

                if line_matches.is_empty() {
                    updated_content.push_str(line);
                    updated_content.push('\n');
                } else {
                    let updated_line =
                        apply_line_replacements(line, &line_matches, &markdown_file.path);

                    // Track which types of changes occurred
                    for line_match in &line_matches {
                        match line_match.match_type() {
                            MatchType::BackPopulate => has_back_populate_changes = true,
                            MatchType::ImageReference => has_image_reference_changes = true,
                        }
                    }

                    if !updated_line.is_empty() {
                        updated_content.push_str(&updated_line);
                        updated_content.push('\n');
                    }
                }
                content_line_number += 1;
            }

            // Update the content and mark file as modified
            markdown_file.content = updated_content.trim_end().to_string();

            if has_back_populate_changes {
                markdown_file.mark_as_back_populated(operational_timezone);
            }
            if has_image_reference_changes {
                markdown_file.mark_image_reference_as_updated(operational_timezone);
            }
        }
    }

    fn collect_replaceable_matches(
        markdown_file: &MarkdownFile,
    ) -> Vec<Box<dyn ReplaceableContent>> {
        let mut matches = Vec::new();

        // Add `BackPopulateMatch` values.
        matches.extend(
            markdown_file
                .matches
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
        matches.sort_by_key(|m| (m.line_number(), std::cmp::Reverse(m.position())));

        matches
    }
}

#[allow(
    clippy::panic,
    reason = "UTF-8 boundary violation indicates a bug in position calculation"
)]
fn apply_line_replacements(
    line: &str,
    line_matches: &[&dyn ReplaceableContent],
    file_path: &Path,
) -> String {
    let mut updated_line = line.to_string();
    let mut has_image_replacement = false;

    // Sort matches in descending order by `position`
    let mut sorted_matches = line_matches.to_vec();
    sorted_matches.sort_by_key(|m| std::cmp::Reverse(m.position()));

    // Apply replacements in sorted (reverse) order
    for match_info in sorted_matches {
        let start = match_info.position();
        let end = start + match_info.matched_text().len();

        // Check for UTF-8 boundary issues
        if !updated_line.is_char_boundary(start) || !updated_line.is_char_boundary(end) {
            eprintln!(
                "Error: Invalid UTF-8 boundary in file '{}', line {}.\n\
                Match position: {start} to {end}.\nLine content:\n{updated_line}\nFound text: '{}'\n",
                file_path.display(),
                match_info.line_number(),
                match_info.matched_text()
            );
            panic!("Invalid UTF-8 boundary detected. Check positions and text encoding.");
        }

        // Track if this is an image replacement
        if match_info.match_type() == MatchType::ImageReference {
            has_image_replacement = true;
        }

        // Perform the replacement
        updated_line.replace_range(start..end, &match_info.get_replacement());

        // Validation check after each replacement
        if updated_line.contains("[[[") || updated_line.contains("]]]") {
            eprintln!(
                "\nWarning: Potential nested pattern detected after replacement in file '{}', line {}.\n\
                Current line:\n{updated_line}\n",
                file_path.display(),
                match_info.line_number(),
            );
        }
    }

    // If we had any image replacements, clean up the line
    if has_image_replacement {
        let trimmed = updated_line.trim();
        if trimmed.is_empty() {
            String::new()
        } else {
            normalize_spaces(trimmed)
        }
    } else {
        updated_line
    }
}

fn normalize_spaces(text: &str) -> String { text.split_whitespace().collect::<Vec<_>>().join(" ") }

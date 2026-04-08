use aho_corasick::AhoCorasick;

use super::MarkdownFile;
use super::markdown_file_types::BackPopulateMatch;
use super::markdown_file_types::MatchContext;
use super::match_helpers;
use super::text_excluder::CodeBlockExcluder;
use super::text_excluder::InlineCodeExcluder;
use crate::obsidian_repository;
use crate::utils::MARKDOWN_REGEX;
use crate::validated_config::ValidatedConfig;
use crate::wikilink;
use crate::wikilink::ToWikilink;
use crate::wikilink::Wikilink;

impl MarkdownFile {
    pub(super) fn process_file_for_back_populate_replacements_inner(
        &mut self,
        sorted_wikilinks: &[&Wikilink],
        config: &ValidatedConfig,
        automaton: &AhoCorasick,
    ) {
        let content = self.content.clone();
        let mut code_block_tracker = CodeBlockExcluder::new();

        for (line_idx, line) in content.lines().enumerate() {
            // Skip empty/whitespace lines early
            if line.trim().is_empty() {
                continue;
            }

            // Update state and skip if needed
            code_block_tracker.update(line);
            if code_block_tracker.is_in_code_block() {
                continue;
            }

            // Process the line and collect matches
            let matches = self.process_line_for_back_populate_replacements(
                line,
                line_idx,
                automaton,
                sorted_wikilinks,
                config,
            );

            // Store matches instead of accumulating for return
            self.matches.unambiguous.extend(matches);
        }
    }

    pub(super) fn process_line_for_back_populate_replacements(
        &self,
        line: &str,
        line_idx: usize,
        automaton: &AhoCorasick,
        sorted_wikilinks: &[&Wikilink],
        config: &ValidatedConfig,
    ) -> Vec<BackPopulateMatch> {
        let mut matches = Vec::new();
        let exclusion_zones = self.collect_exclusion_zones(line, config);

        // Collect all valid matches
        for mat in automaton.find_iter(line) {
            let wikilink = sorted_wikilinks[mat.pattern()];
            let starts_at = mat.start();
            let ends_at = mat.end();

            if match_helpers::range_overlaps(&exclusion_zones, starts_at, ends_at) {
                continue;
            }

            let matched_text = &line[starts_at..ends_at];
            if !match_helpers::is_word_boundary(line, starts_at, ends_at) {
                continue;
            }

            if self.should_create_match(line, starts_at, matched_text) {
                let mut replacement = if matched_text == wikilink.target {
                    wikilink.target.to_wikilink()
                } else {
                    wikilink.target.to_aliased_wikilink(matched_text)
                };

                let match_context = if match_helpers::is_in_markdown_table(line, matched_text) {
                    MatchContext::MarkdownTable
                } else {
                    MatchContext::Plaintext
                };
                if match_context == MatchContext::MarkdownTable {
                    replacement = replacement.replace('|', r"\|");
                }

                let relative_path =
                    obsidian_repository::format_relative_path(&self.path, config.obsidian_path());

                matches.push(BackPopulateMatch {
                    found_text: matched_text.to_string(),
                    line_number: self.get_real_line_number(line_idx),
                    line_text: line.to_string(),
                    position: starts_at,
                    match_context,
                    relative_path,
                    replacement,
                });
            }
        }

        matches
    }

    pub(super) fn collect_exclusion_zones(
        &self,
        line: &str,
        config: &ValidatedConfig,
    ) -> Vec<(usize, usize)> {
        let mut exclusion_zones = Vec::new();

        // Add invalid wikilinks as exclusion zones
        for invalid_wikilink in &self.wikilinks.invalid {
            // Only add exclusion zone if this invalid wikilink is on the current line
            if invalid_wikilink.line == line {
                exclusion_zones.push(invalid_wikilink.span);
            }
        }

        let regex_sources = [
            config.do_not_back_populate_regexes(),
            self.do_not_back_populate_regexes.as_deref(),
        ];

        // Flatten the iterator to get a single iterator over regexes
        for do_not_back_populate_regexes in regex_sources.iter().flatten() {
            for regex in *do_not_back_populate_regexes {
                for mat in regex.find_iter(line) {
                    exclusion_zones.push((mat.start(), mat.end()));
                }
            }
        }

        // Add inline code spans as exclusion zones
        let mut inline_code = InlineCodeExcluder::new();
        let mut span_start = None;
        for (byte_offset, ch) in line.char_indices() {
            let was_inside = inline_code.is_in_code_block();
            inline_code.update(ch);
            let is_inside = inline_code.is_in_code_block();

            if !was_inside && is_inside {
                span_start = Some(byte_offset);
            } else if was_inside
                && !is_inside
                && let Some(start) = span_start.take()
            {
                exclusion_zones.push((start, byte_offset + ch.len_utf8()));
            }
        }

        // Add Markdown links as exclusion zones
        for mat in MARKDOWN_REGEX.find_iter(line) {
            exclusion_zones.push((mat.start(), mat.end()));
        }

        // they need to be ordered!
        exclusion_zones.sort_by_key(|&(start, _)| start);
        exclusion_zones
    }

    pub(super) fn should_create_match(
        &self,
        line: &str,
        absolute_start: usize,
        matched_text: &str,
    ) -> bool {
        // Check if this is the text's own page or matches any frontmatter aliases
        if let Some(stem) = self.path.file_stem().and_then(|s| s.to_str()) {
            if stem.eq_ignore_ascii_case(matched_text) {
                return false;
            }

            // Check against frontmatter aliases
            if let Some(frontmatter) = &self.frontmatter
                && let Some(aliases) = frontmatter.aliases()
                && aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(matched_text))
            {
                return false;
            }
        }

        !wikilink::is_within_wikilink(line, absolute_start)
    }
}

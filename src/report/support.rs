use std::path::Path;

use super::constants::HIGHLIGHT_CLOSE_TAG;
use super::constants::HIGHLIGHT_EXTRA_TAG_CAPACITY_MULTIPLIER;
use super::constants::HIGHLIGHT_OPEN_TAG;
use super::constants::INVALID_UTF8_BOUNDARY_DETECTED;
use crate::constants::BACKSLASH;
use crate::constants::BACKSLASH_PARITY_DIVISOR;
use crate::constants::CLOSING_BRACKET;
use crate::constants::CLOSING_WIKILINK;
use crate::constants::ESCAPED_BRACKET_CLOSE;
use crate::constants::ESCAPED_BRACKET_OPEN;
use crate::constants::ESCAPED_PIPE_CAPACITY_MULTIPLIER;
use crate::constants::OPENING_BRACKET;
use crate::constants::OPENING_WIKILINK;
use crate::constants::PIPE;

// `escape_pipe` escapes unescaped Markdown table pipes.
pub(super) fn escape_pipe(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() * ESCAPED_PIPE_CAPACITY_MULTIPLIER);
    let chars: Vec<char> = text.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == PIPE {
            // Count the number of consecutive backslashes before '|'
            let mut backslash_count = 0;
            let mut j = i;
            while j > 0 && chars[j - 1] == BACKSLASH {
                backslash_count += 1;
                j -= 1;
            }

            // If even number of backslashes, '|' is not escaped
            if backslash_count % BACKSLASH_PARITY_DIVISOR == 0 {
                escaped.push(BACKSLASH);
            }
            escaped.push(PIPE);
        } else {
            escaped.push(ch);
        }
        i += 1;
    }

    escaped
}

// `escape_brackets` displays wikilink brackets as escaped text.
pub(super) fn escape_brackets(text: &str) -> String {
    text.replace(OPENING_BRACKET, ESCAPED_BRACKET_OPEN)
        .replace(CLOSING_BRACKET, ESCAPED_BRACKET_CLOSE)
}

pub(super) fn format_wikilink(path: &Path, obsidian_path: &Path) -> String {
    let relative_path = path.strip_prefix(obsidian_path).unwrap_or(path);
    let display_name = path.file_stem().unwrap_or_default().to_string_lossy();

    let path_display = relative_path.display();
    format!("{OPENING_WIKILINK}{path_display}\\{PIPE}{display_name}{CLOSING_WIKILINK}")
}

pub(super) fn highlight_matches(text: &str, positions: &[usize], match_length: usize) -> String {
    let mut result = String::with_capacity(text.len() * HIGHLIGHT_EXTRA_TAG_CAPACITY_MULTIPLIER);
    let mut last_end = 0;

    let mut sorted_positions = positions.to_vec();
    sorted_positions.sort_unstable();

    for &start in &sorted_positions {
        let end = start + match_length;

        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            eprintln!("{INVALID_UTF8_BOUNDARY_DETECTED} {start} or {end}");
            return text.to_string();
        }

        result.push_str(&text[last_end..start]);
        result.push_str(HIGHLIGHT_OPEN_TAG);
        result.push_str(&text[start..end]);
        result.push_str(HIGHLIGHT_CLOSE_TAG);
        last_end = end;
    }

    result.push_str(&text[last_end..]);
    result
}

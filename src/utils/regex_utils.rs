use std::sync::Arc;
use std::sync::LazyLock;

use regex::Regex;

use crate::constants::*;

pub static MARKDOWN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[.*?\]\(.*?\)").unwrap());
pub static EMAIL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap());
pub static TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)(#[a-zA-Z0-9_-]+)").unwrap());
pub static RAW_HTTP_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://[^\s]+").unwrap());
static IMAGE_EXTENSIONS_PATTERN: LazyLock<String> = LazyLock::new(|| IMAGE_EXTENSIONS.join("|"));
pub static IMAGE_REGEX: LazyLock<Arc<Regex>> = LazyLock::new(|| {
    Arc::new(
        Regex::new(&format!(
            r"(?ix)
            (!?\[\[([^\]|]+\.(?:{}))[^\]]*\]\])
            |
            (!?\[[^\]]*\]\(([^)]+\.(?:{}))[^)]*\))
            ",
            *IMAGE_EXTENSIONS_PATTERN, *IMAGE_EXTENSIONS_PATTERN
        ))
        .unwrap(),
    )
});

pub fn build_case_insensitive_word_finder(patterns: &Option<Vec<String>>) -> Option<Vec<Regex>> {
    patterns.as_ref().map(|patterns| {
        patterns
            .iter()
            .map(|pattern| {
                Regex::new(&format!(r"(?i)\b{}\b", regex::escape(pattern)))
                    .expect("Failed to build regex for exclusion pattern")
            })
            .collect()
    })
}

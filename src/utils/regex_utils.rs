use crate::constants::*;
use lazy_static::lazy_static;
use regex::Regex;
use std::sync::Arc;

lazy_static! {
    pub static ref MARKDOWN_REGEX: Regex = Regex::new(r"\[.*?\]\(.*?\)").unwrap();
    pub static ref EMAIL_REGEX: Regex =
        Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
    pub static ref TAG_REGEX: Regex = Regex::new(r"(?:^|\s)(#[a-zA-Z0-9_-]+)").unwrap();
    pub static ref RAW_HTTP_REGEX: Regex = Regex::new(r"https?://[^\s]+").unwrap();
    static ref IMAGE_EXTENSIONS_PATTERN: String = IMAGE_EXTENSIONS.join("|");
    pub static ref IMAGE_REGEX: Arc<Regex> = Arc::new(
        Regex::new(&format!(
            r"(?ix)
            (!?\[\[([^\]|]+\.(?:{}))[^\]]*\]\])
            |
            (!?\[[^\]]*\]\(([^)]+\.(?:{}))[^)]*\))
            ",
            *IMAGE_EXTENSIONS_PATTERN, *IMAGE_EXTENSIONS_PATTERN
        ))
        .unwrap()
    );
}

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

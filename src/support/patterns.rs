use std::sync::Arc;
use std::sync::LazyLock;

use regex::Regex;

use crate::constants::IMAGE_EXTENSIONS;
use crate::constants::INVALID_REGEX_EXIT_CODE;

pub(crate) fn compile_regex(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => {
            eprintln!("invalid regex pattern {pattern:?}: {error}");
            std::process::exit(INVALID_REGEX_EXIT_CODE);
        },
    }
}

pub static MARKDOWN_REGEX: LazyLock<Regex> = LazyLock::new(|| compile_regex(r"\[.*?\]\(.*?\)"));
pub static EMAIL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| compile_regex(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}"));
pub static TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| compile_regex(r"(?:^|\s)(#[a-zA-Z0-9_-]+)"));
pub static RAW_HTTP_REGEX: LazyLock<Regex> = LazyLock::new(|| compile_regex(r"https?://[^\s]+"));
static IMAGE_EXTENSIONS_PATTERN: LazyLock<String> = LazyLock::new(|| IMAGE_EXTENSIONS.join("|"));
pub static IMAGE_REGEX: LazyLock<Arc<Regex>> = LazyLock::new(|| {
    let image_pattern = format!(
        r"(?ix)
        (!?\[\[([^\]|]+\.(?:{}))[^\]]*\]\])
        |
        (!?\[[^\]]*\]\(([^)]+\.(?:{}))[^)]*\))
        ",
        *IMAGE_EXTENSIONS_PATTERN, *IMAGE_EXTENSIONS_PATTERN
    );

    Arc::new(compile_regex(&image_pattern))
});

pub fn build_case_insensitive_word_finder(patterns: &[String]) -> Vec<Regex> {
    patterns
        .iter()
        .map(|pattern| {
            let escaped_pattern = regex::escape(pattern);
            let finder_pattern = format!(r"(?i)\b{escaped_pattern}\b");
            compile_regex(&finder_pattern)
        })
        .collect()
}

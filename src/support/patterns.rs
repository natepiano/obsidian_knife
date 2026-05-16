use std::sync::Arc;
use std::sync::LazyLock;

use regex::Regex;

use crate::constants::CASE_INSENSITIVE_WORD_PATTERN_PREFIX;
use crate::constants::CASE_INSENSITIVE_WORD_PATTERN_SUFFIX;
use crate::constants::EMAIL_PATTERN;
use crate::constants::IMAGE_EXTENSIONS;
use crate::constants::IMAGE_EXTENSIONS_SEPARATOR;
use crate::constants::INVALID_REGEX_EXIT_CODE;
use crate::constants::INVALID_REGEX_PATTERN;
use crate::constants::MARKDOWN_LINK_PATTERN;
use crate::constants::RAW_HTTP_PATTERN;
use crate::constants::TAG_PATTERN;

pub(crate) fn compile_regex(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => {
            eprintln!("{INVALID_REGEX_PATTERN} {pattern:?}: {error}");
            std::process::exit(INVALID_REGEX_EXIT_CODE);
        },
    }
}

pub static MARKDOWN_REGEX: LazyLock<Regex> = LazyLock::new(|| compile_regex(MARKDOWN_LINK_PATTERN));
pub static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| compile_regex(EMAIL_PATTERN));
pub static TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| compile_regex(TAG_PATTERN));
pub static RAW_HTTP_REGEX: LazyLock<Regex> = LazyLock::new(|| compile_regex(RAW_HTTP_PATTERN));
static IMAGE_EXTENSIONS_PATTERN: LazyLock<String> =
    LazyLock::new(|| IMAGE_EXTENSIONS.join(IMAGE_EXTENSIONS_SEPARATOR));
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
            let finder_pattern = format!(
                "{CASE_INSENSITIVE_WORD_PATTERN_PREFIX}{escaped_pattern}{CASE_INSENSITIVE_WORD_PATTERN_SUFFIX}"
            );
            compile_regex(&finder_pattern)
        })
        .collect()
}

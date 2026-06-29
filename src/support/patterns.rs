use std::process::exit;
use std::sync::Arc;
use std::sync::LazyLock;

use regex::Regex;
use regex::escape;

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
            exit(INVALID_REGEX_EXIT_CODE);
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
            let escaped_pattern = escape(pattern);
            let finder_pattern = format!(
                "{CASE_INSENSITIVE_WORD_PATTERN_PREFIX}{escaped_pattern}{CASE_INSENSITIVE_WORD_PATTERN_SUFFIX}"
            );
            compile_regex(&finder_pattern)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::MARKDOWN_REGEX;
    #[test]
    fn test_markdown_regex_matches() {
        let regex = MARKDOWN_REGEX.clone();

        let matching_cases = vec![
            "[text](https://example.com)",
            "[link](https://test.com)",
            "[page](folder/page.md)",
            "[img](../images/test.png)",
            "[text](path 'title')",
            "[text](path \"title\")",
            "[](path)",
            "[text]()",
            "[]()",
        ];

        for case in matching_cases {
            assert!(regex.is_match(case), "Regex should match '{case}'");
        }

        let non_matching_cases = vec![
            "plain text",
            "[[wikilink]]",
            "![[imagelink]]",
            "[incomplete",
        ];

        for case in non_matching_cases {
            assert!(!regex.is_match(case), "Regex should not match '{case}'");
        }
    }

    #[test]
    fn test_markdown_link_extraction() {
        let regex = MARKDOWN_REGEX.clone();
        let text = "Here is [one](link1) and [two](link2) and normal text";

        let links: Vec<_> = regex.find_iter(text).map(|m| m.as_str()).collect();
        assert_eq!(links.len(), 2);
        assert_eq!(links[0], "[one](link1)");
        assert_eq!(links[1], "[two](link2)");
    }
}

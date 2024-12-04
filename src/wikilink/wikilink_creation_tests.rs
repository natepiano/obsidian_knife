use super::*;
use crate::wikilink::{ToWikilink, WikilinkParseResult};
use std::path::Path;

// Macro to test simple wikilink creation
macro_rules! test_wikilink {
    ($test_name:ident, $input:expr, $expected:expr) => {
        #[test]
        fn $test_name() {
            let formatted = format_path_as_wikilink(&Path::new($input));
            assert_eq!(formatted, $expected);
        }
    };
}

// Define simple wikilink tests using the macro
test_wikilink!(wikilink_simple, "test", "[[test]]");
test_wikilink!(wikilink_with_md, "test.md", "[[test]]");
test_wikilink!(wikilink_empty, "", "[[]]");
test_wikilink!(wikilink_unicode, "テスト.md", "[[テスト]]");
test_wikilink!(wikilink_with_space, "test café.md", "[[test café]]");

/// Helper function to parse a full wikilink string.
/// It ensures the input starts with `[[` and ends with `]]`,
/// extracts the inner content, and passes it to `parse_wikilink`.
fn parse_full_wikilink(input: &str) -> Option<WikilinkParseResult> {
    if input.starts_with("[[") && input.ends_with("]]") {
        // Extract the substring after `[[` and include the closing `]]`
        let inner = &input[2..];
        let mut chars = inner.char_indices().peekable();
        parse_wikilink(&mut chars)
    } else {
        // Invalid format if it doesn't start and end with brackets
        None
    }
}

/// Asserts that a full wikilink string is parsed correctly as valid.
fn assert_valid_wikilink(
    input: &str,
    expected_target: &str,
    expected_display: &str,
    expected_is_alias: bool,
) {
    let result = parse_full_wikilink(input).expect("Failed to parse wikilink");

    match result {
        WikilinkParseResult::Valid(wikilink) => {
            assert_eq!(
                wikilink.target, expected_target,
                "Target mismatch for input: {}",
                input
            );
            assert_eq!(
                wikilink.display_text, expected_display,
                "Display text mismatch for input: {}",
                input
            );
            assert_eq!(
                wikilink.is_alias, expected_is_alias,
                "Alias flag mismatch for input: {}",
                input
            );
        }
        WikilinkParseResult::Invalid(invalid) => {
            panic!(
                "Expected valid wikilink for input: {}, but got invalid: {} ({:?})",
                input, invalid.content, invalid.reason
            );
        }
    }
}

/// Asserts that a full wikilink string fails to parse as expected.
fn assert_invalid_wikilink(input: &str, expected_reason: InvalidWikilinkReason) {
    let result = parse_full_wikilink(input);

    match result {
        Some(WikilinkParseResult::Invalid(invalid)) => {
            assert_eq!(
                invalid.reason, expected_reason,
                "Expected reason {:?} but got {:?} for input: {}",
                expected_reason, invalid.reason, input
            );
        }
        Some(WikilinkParseResult::Valid(_)) => {
            panic!(
                "Expected invalid wikilink for input: {}, but got valid.",
                input
            );
        }
        None => {
            panic!(
                "Expected invalid wikilink for input: {}, but got None.",
                input
            );
        }
    }
}

#[test]
fn to_aliased_wikilink_variants() {
    let test_cases = vec![
        ("target", "target", "[[target]]"),
        ("Target", "target", "[[Target|target]]"),
        ("test link", "Test Link", "[[test link|Test Link]]"),
        ("Apple", "fruit", "[[Apple|fruit]]"),
        ("Home", "主页", "[[Home|主页]]"),
        ("page.md", "Page", "[[page|Page]]"),
        ("café", "咖啡", "[[café|咖啡]]"),
        ("テスト", "Test", "[[テスト|Test]]"),
    ];

    for (target, display, expected) in test_cases {
        let result = target.to_aliased_wikilink(display);
        assert_eq!(
            result, expected,
            "Failed for target '{}', display '{}'",
            target, display
        );
    }

    // Testing with String type
    let string_target = String::from("Target");
    assert_eq!(
        string_target.to_aliased_wikilink("target"),
        "[[Target|target]]"
    );
    assert_eq!(string_target.to_aliased_wikilink("Target"), "[[Target]]");
}

#[test]
fn test_empty_wikilink_variants() {
    let test_cases = vec![
        ("[[]]", InvalidWikilinkReason::EmptyWikilink),
        ("[[|]]", InvalidWikilinkReason::EmptyWikilink),
        ("[[display|]]", InvalidWikilinkReason::EmptyWikilink),
        ("[[|alias]]", InvalidWikilinkReason::EmptyWikilink),
        ("[[display\\|]]", InvalidWikilinkReason::EmptyWikilink),
    ];

    for (input, expected_reason) in test_cases {
        assert_invalid_wikilink(input, expected_reason);
    }
}

#[test]
fn test_parse_wikilink_basic_and_aliased() {
    let test_cases = vec![
        // Basic cases
        ("[[test]]", "test", "test", false),
        ("[[simple link]]", "simple link", "simple link", false),
        ("[[  spaced  ]]", "spaced", "spaced", false),
        ("[[测试]]", "测试", "测试", false),
        // Aliased cases
        ("[[target|display]]", "target", "display", true),
        ("[[  target  |  display  ]]", "target", "display", true),
        ("[[测试|test]]", "测试", "test", true),
        ("[[test|测试]]", "test", "测试", true),
        ("[[a/b/c|display]]", "a/b/c", "display", true),
    ];

    for (input, target, display, is_alias) in test_cases {
        assert_valid_wikilink(input, target, display, is_alias);
    }
}

#[test]
fn test_parse_wikilink_double_alias() {
    // Invalid cases
    let invalid_cases = vec![
        // Basic double alias case
        (
            "[[target|alias|second]]",
            InvalidWikilinkReason::DoubleAlias,
        ),
        // Multiple pipes
        (
            "[[target|alias|second|third]]",
            InvalidWikilinkReason::DoubleAlias,
        ),
        // Consecutive pipes
        ("[[target||alias]]", InvalidWikilinkReason::DoubleAlias),
        // Mixed consecutive and separated pipes
        (
            "[[target||alias|another]]",
            InvalidWikilinkReason::DoubleAlias,
        ),
        // Complex case with double pipe
        (
            "[[target|display|another]]",
            InvalidWikilinkReason::DoubleAlias,
        ),
        // Multiple consecutive pipes
        ("[[target|||display]]", InvalidWikilinkReason::DoubleAlias),
        // Escaped pipe is still a pipe
        (
            "[[target\\|text|DoubleAlias]]",
            InvalidWikilinkReason::DoubleAlias,
        ),
    ];

    for (input, expected_reason) in invalid_cases {
        assert_invalid_wikilink(input, expected_reason);
    }
}

#[test]
fn test_parse_wikilink_escaped_chars() {
    let test_cases = vec![
        // Regular escape in target
        ("[[test\\]text]]", "test]text", "test]text", false),
        // Escaped characters in aliased link
        ("[[target|display\\]text]]", "target", "display]text", true),
        // Multiple escaped characters
        (
            "[[test\\]with\\[brackets]]",
            "test]with[brackets",
            "test]with[brackets",
            false,
        ),
        // Escaped single brackets
        (
            "[[text\\[in\\]brackets]]",
            "text[in]brackets",
            "text[in]brackets",
            false,
        ),
        (
            "[[target\\[x\\]|display\\[y\\]]]",
            "target[x]",
            "display[y]",
            true,
        ),
    ];

    for (input, target, display, is_alias) in test_cases {
        assert_valid_wikilink(input, target, display, is_alias);
    }
}

#[test]
fn test_parse_wikilink_unmatched_brackets() {
    let test_cases = vec![
        // Basic unmatched brackets
        (
            "[[text]text]]",
            InvalidWikilinkReason::UnmatchedSingleInWikilink,
        ),
        (
            "[[text[text]]",
            InvalidWikilinkReason::UnmatchedSingleInWikilink,
        ),
        // Mixed escape scenarios - only flag when a bracket is actually unmatched
        (
            "[[text[\\]text]]",
            InvalidWikilinkReason::UnmatchedSingleInWikilink,
        ), // first [ is unmatched, second is escaped
        (
            "[[text\\[]text]]",
            InvalidWikilinkReason::UnmatchedSingleInWikilink,
        ), // ] is unmatched, [ is escaped
        // Complex cases with aliases
        (
            "[[target[x|display]]",
            InvalidWikilinkReason::UnmatchedSingleInWikilink,
        ),
        (
            "[[target|display]x]]",
            InvalidWikilinkReason::UnmatchedSingleInWikilink,
        ),
    ];

    for (input, expected_reason) in test_cases {
        assert_invalid_wikilink(input, expected_reason);
    }
}

#[test]
fn test_parse_wikilink_special_chars() {
    let test_cases = vec![
        ("[[!@#$%^&*()]]", "!@#$%^&*()", "!@#$%^&*()", false),
        (
            "[[../path/to/file]]",
            "../path/to/file",
            "../path/to/file",
            false,
        ),
        ("[[file (1)]]", "file (1)", "file (1)", false),
        ("[[file (1)|version 1]]", "file (1)", "version 1", true),
        ("[[target|(text)]]", "target", "(text)", true),
    ];

    for (input, target, display, is_alias) in test_cases {
        assert_valid_wikilink(input, target, display, is_alias);
    }
}

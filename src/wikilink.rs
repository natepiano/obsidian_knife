use crate::constants::*;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::cmp::PartialEq;
use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::path::Path;

lazy_static! {
    pub static ref MARKDOWN_REGEX: Regex = Regex::new(r"\[.*?\]\(.*?\)").unwrap();
}

/// Trait to convert strings to wikilink format
pub trait ToWikilink {
    /// Converts the string to a wikilink format by surrounding it with [[]]
    fn to_wikilink(&self) -> String;

    /// Creates an aliased wikilink using the target (self) and display text
    /// If the texts match (case-sensitive), returns a simple wikilink
    /// Otherwise returns an aliased wikilink in the format [[target|display]]
    fn to_aliased_wikilink(&self, display_text: &str) -> String
    where
        Self: AsRef<str>,
    {
        let target_without_md = strip_md_extension(self.as_ref());

        if target_without_md == display_text {
            target_without_md.to_wikilink()
        } else {
            format!("[[{}|{}]]", target_without_md, display_text)
        }
    }
}

/// Helper function to strip .md extension if present
fn strip_md_extension(text: &str) -> &str {
    if text.ends_with(".md") {
        &text[..text.len() - 3]
    } else {
        text
    }
}

impl ToWikilink for str {
    fn to_wikilink(&self) -> String {
        format!("[[{}]]", strip_md_extension(self))
    }
}

impl ToWikilink for String {
    fn to_wikilink(&self) -> String {
        self.as_str().to_wikilink()
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Wikilink {
    pub display_text: String,
    pub target: String,
    pub is_alias: bool,
}

#[derive(Debug)]
pub struct WikilinkError {
    pub display_text: String,
    pub error_type: WikilinkErrorType,
    file_path: String,
    line_number: Option<usize>,
    line_content: Option<String>,
}

impl fmt::Display for WikilinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let error_msg = match self.error_type {
            WikilinkErrorType::ContainsOpenBrackets => "contains opening brackets '[['",
            WikilinkErrorType::ContainsCloseBrackets => "contains closing brackets ']]'",
            WikilinkErrorType::ContainsPipe => "contains pipe character '|'",
        };
        writeln!(
            f,
            "Invalid wikilink pattern: '{}' {}",
            self.display_text, error_msg
        )?;

        if !self.file_path.is_empty() {
            writeln!(f, "File: {}", self.file_path)?;
        }
        if let Some(num) = &self.line_number {
            writeln!(f, "Line number: {}", num)?;
        }
        if let Some(content) = &self.line_content {
            writeln!(f, "Line content: {}", content)?;
        }
        Ok(())
    }
}

impl Error for WikilinkError {}

#[derive(Debug, PartialEq)]
pub enum WikilinkErrorType {
    ContainsOpenBrackets,
    ContainsCloseBrackets,
    ContainsPipe,
}

#[derive(Debug)]
pub struct WikilinkErrorContext {
    pub file_path: String,
    pub line_number: Option<usize>,
    pub line_content: Option<String>,
}

// Update the implementation of fmt::Display for WikilinkErrorContext
impl fmt::Display for WikilinkErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.file_path.is_empty() {
            writeln!(f, "File: {}", self.file_path)?;
        }
        if let Some(num) = &self.line_number {
            writeln!(f, "Line number: {}", num)?;
        }
        if let Some(content) = &self.line_content {
            writeln!(f, "Line content: {}", content)?;
        }
        Ok(())
    }
}

impl Default for WikilinkErrorContext {
    fn default() -> Self {
        WikilinkErrorContext {
            file_path: String::new(),
            line_number: None,
            line_content: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompiledWikilink {
    pub wikilink: Wikilink,
    hash: u64,
}

impl fmt::Display for CompiledWikilink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}{}",
            self.wikilink.target,
            if self.wikilink.is_alias { "|" } else { "" },
            if self.wikilink.is_alias {
                &self.wikilink.display_text
            } else {
                ""
            }
        )
    }
}

impl CompiledWikilink {
    pub fn new(wikilink: Wikilink) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        wikilink.hash(&mut hasher);
        let hash = hasher.finish();

        CompiledWikilink { wikilink, hash }
    }
}

impl std::hash::Hash for CompiledWikilink {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl PartialEq for CompiledWikilink {
    fn eq(&self, other: &Self) -> bool {
        self.wikilink == other.wikilink
    }
}

impl Eq for CompiledWikilink {}

pub fn is_wikilink(potential_wikilink: Option<&String>) -> bool {
    if let Some(test_wikilink) = potential_wikilink {
        test_wikilink.starts_with(OPENING_WIKILINK) && test_wikilink.ends_with(CLOSING_WIKILINK)
    } else {
        false
    }
}

pub fn create_filename_wikilink(filename: &str) -> Wikilink {
    let display_text = filename.strip_suffix(".md").unwrap_or(filename).to_string();

    Wikilink {
        display_text: display_text.clone(),
        target: display_text,
        is_alias: false,
    }
}

pub fn format_wikilink(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| format!("[[{}]]", s))
        .unwrap_or_else(|| "[[]]".to_string())
}

pub fn compile_wikilink_with_context(
    wikilink: Wikilink,
    file_path: &Path,
    line_number: Option<usize>,
    line_content: Option<&str>,
) -> Result<CompiledWikilink, WikilinkError> {
    compile_wikilink(wikilink).map_err(|e| WikilinkError {
        display_text: e.display_text,
        error_type: e.error_type,
        file_path: file_path.display().to_string(),
        line_number,
        line_content: line_content.map(String::from),
    })
}

pub fn compile_wikilink(wikilink: Wikilink) -> Result<CompiledWikilink, WikilinkError> {
    let search_text = &wikilink.display_text;

    // Check for invalid characters
    if search_text.contains("[[") {
        return Err(WikilinkError {
            display_text: search_text.to_string(),
            error_type: WikilinkErrorType::ContainsOpenBrackets,
            file_path: String::new(),
            line_number: None,
            line_content: None,
        });
    }
    if search_text.contains("]]") {
        return Err(WikilinkError {
            display_text: search_text.to_string(),
            error_type: WikilinkErrorType::ContainsCloseBrackets,
            file_path: String::new(),
            line_number: None,
            line_content: None,
        });
    }
    if search_text.contains("|") {
        return Err(WikilinkError {
            display_text: search_text.to_string(),
            error_type: WikilinkErrorType::ContainsPipe,
            file_path: String::new(),
            line_number: None,
            line_content: None,
        });
    }

    Ok(CompiledWikilink::new(wikilink))
}

// In collect_all_wikilinks, update the calls:
pub fn collect_all_wikilinks(
    content: &str,
    aliases: &Option<Vec<String>>,
    file_path: &Path,
) -> Result<HashSet<CompiledWikilink>, WikilinkError> {
    let mut all_wikilinks = HashSet::new();

    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    // Add filename-based wikilink
    let filename_wikilink = create_filename_wikilink(filename);
    let compiled = compile_wikilink_with_context(filename_wikilink.clone(), file_path, None, None)?;
    all_wikilinks.insert(compiled);

    // Add aliases if present
    if let Some(alias_list) = aliases {
        for alias in alias_list {
            let wikilink = Wikilink {
                display_text: alias.clone(),
                target: filename_wikilink.target.clone(),
                is_alias: true,
            };
            let compiled = compile_wikilink_with_context(wikilink, file_path, None, None)?;
            all_wikilinks.insert(compiled);
        }
    }

    // Process content line by line to get line numbers for error context
    for (line_number, line) in content.lines().enumerate() {
        let wikilinks = extract_wikilinks_from_content(line);
        for wikilink in wikilinks {
            let compiled = compile_wikilink_with_context(
                wikilink,
                file_path,
                Some(line_number + 1),
                Some(line),
            )?;
            all_wikilinks.insert(compiled);
        }
    }

    Ok(all_wikilinks)
}

pub fn extract_wikilinks_from_content(content: &str) -> Vec<Wikilink> {
    let mut wikilinks = Vec::new();
    let mut chars = content.char_indices().peekable();

    while let Some((start_idx, ch)) = chars.next() {
        if ch == '[' && is_next_char(&mut chars, '[') {
            // Check if the previous character was '!' (image link)
            if start_idx > 0 && is_previous_char(content, start_idx, '!') {
                continue; // Skip image links
            }

            // Parse the wikilink
            if let Some(wikilink) = parse_wikilink(&mut chars) {
                wikilinks.push(wikilink);
            }
        }
    }

    wikilinks
}

fn is_previous_char(content: &str, index: usize, expected: char) -> bool {
    content[..index].chars().rev().next() == Some(expected)
}

fn parse_wikilink(chars: &mut std::iter::Peekable<std::str::CharIndices>) -> Option<Wikilink> {
    #[derive(Debug)]
    enum State {
        Target(String),
        Display(String, String),
    }

    impl State {
        fn push_char(&mut self, c: char) {
            match self {
                State::Target(target) => target.push(c),
                State::Display(_, display) => display.push(c),
            }
        }

        fn to_wikilink(self) -> Option<Wikilink> {
            match self {
                State::Target(target) => {
                    let trimmed = target.trim().to_string();
                    Some(Wikilink {
                        display_text: trimmed.clone(),
                        target: trimmed,
                        is_alias: false,
                    })
                }
                State::Display(target, display) => {
                    let trimmed_target = target.trim().to_string();
                    let trimmed_display = display.trim().to_string();
                    Some(Wikilink {
                        display_text: trimmed_display,
                        target: trimmed_target,
                        is_alias: true,
                    })
                }
            }
        }

        fn transition_to_display(self) -> Self {
            match self {
                State::Target(target) => State::Display(target, String::new()),
                display_state => display_state,
            }
        }
    }

    let mut state = State::Target(String::new());
    let mut escape = false;

    while let Some((_, c)) = chars.next() {
        match (escape, c) {
            (true, '|') => {
                state = state.transition_to_display();
                escape = false;
            }
            (true, c) => {
                state.push_char(c);
                escape = false;
            }
            (false, '\\') => escape = true,
            (false, '|') => state = state.transition_to_display(),
            (false, ']') if is_next_char(chars, ']') => return state.to_wikilink(),
            (false, c) => state.push_char(c),
        }
    }

    None
}

/// Helper function to check if the next character matches the expected one
fn is_next_char(chars: &mut std::iter::Peekable<std::str::CharIndices>, expected: char) -> bool {
    if let Some(&(_, next_ch)) = chars.peek() {
        if next_ch == expected {
            chars.next(); // Consume the expected character
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // Helper function for assertions
    fn assert_contains_wikilink(
        wikilinks: &HashSet<CompiledWikilink>,
        target: &str,
        display: Option<&str>,
        is_alias: bool,
    ) {
        let exists = wikilinks.iter().any(|w| {
            w.wikilink.target == target
                && w.wikilink.display_text == display.unwrap_or(target)
                && w.wikilink.is_alias == is_alias
        });
        assert!(
            exists,
            "Expected wikilink with target '{}', display '{:?}', is_alias '{}'",
            target, display, is_alias
        );
    }

    // Macro for parameterized tests
    macro_rules! test_wikilink {
        ($test_name:ident, $input:expr, $expected:expr) => {
            #[test]
            fn $test_name() {
                let result = $input.to_wikilink();
                assert_eq!(result, $expected);
            }
        };
    }

    // Submodule for collecting wikilinks
    mod collect_wikilinks {
        use super::*;
        use tempfile::TempDir;

        #[test]
        fn collect_all_wikilinks_with_aliases() {
            let content = "# Test\nHere's a [[Regular Link]] and [[Target|Display Text]]";
            let aliases = Some(vec!["Alias One".to_string(), "Alias Two".to_string()]);

            let temp_dir = TempDir::new().unwrap();
            let file_path = temp_dir.path().join("test file.md");
            std::fs::write(&file_path, content).unwrap();

            let wikilinks = collect_all_wikilinks(content, &aliases, &file_path).unwrap();

            // Verify expected wikilinks
            assert_contains_wikilink(&wikilinks, "test file", None, false);
            assert_contains_wikilink(&wikilinks, "test file", Some("Alias One"), true);
            assert_contains_wikilink(&wikilinks, "test file", Some("Alias Two"), true);
            assert_contains_wikilink(&wikilinks, "Regular Link", None, false);
            assert_contains_wikilink(&wikilinks, "Target", Some("Display Text"), true);
        }

        #[test]
        fn collect_wikilinks_with_context() {
            let content = "Some [[Link]] here.";
            let temp_dir = TempDir::new().unwrap();
            let file_path = temp_dir.path().join("file.md");
            std::fs::write(&file_path, content).unwrap();

            let wikilinks = collect_all_wikilinks(content, &None, &file_path).unwrap();

            assert_contains_wikilink(&wikilinks, "Link", None, false);
        }
    }

    // Submodule for wikilink creation
    mod wikilink_creation {
        use super::*;

        test_wikilink!(wikilink_simple, "test", "[[test]]");
        test_wikilink!(wikilink_with_md, "test.md", "[[test]]");
        test_wikilink!(wikilink_empty, "", "[[]]");
        test_wikilink!(wikilink_unicode, "テスト.md", "[[テスト]]");
        test_wikilink!(wikilink_with_space, "test café.md", "[[test café]]");

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

        // Helper function to test wikilink parsing
        fn assert_parse_wikilink(
            input: &str,
            exp_target: &str,
            exp_display: &str,
            exp_alias: bool,
        ) {
            let mut chars = input.char_indices().peekable();
            let result = parse_wikilink(&mut chars).unwrap();
            assert_eq!(result.target, exp_target);
            assert_eq!(result.display_text, exp_display);
            assert_eq!(result.is_alias, exp_alias);
        }

        #[test]
        fn test_parse_wikilink_basic_and_aliased() {
            let test_cases = vec![
                // Basic cases
                ("test]]", "test", "test", false),
                ("simple link]]", "simple link", "simple link", false),
                ("  spaced  ]]", "spaced", "spaced", false),
                ("测试]]", "测试", "测试", false),
                // Aliased cases
                ("target|display]]", "target", "display", true),
                ("  target  |  display  ]]", "target", "display", true),
                ("测试|test]]", "测试", "test", true),
                ("test|测试]]", "test", "测试", true),
                ("a/b/c|display]]", "a/b/c", "display", true),
            ];

            for (input, target, display, is_alias) in test_cases {
                assert_parse_wikilink(input, target, display, is_alias);
            }
        }

        #[test]
        fn test_parse_wikilink_escaped_chars() {
            let test_cases = vec![
                // Regular escape in target
                ("test\\]text]]", "test]text", "test]text", false),
                // Escaped characters in aliased link
                ("target|display\\]text]]", "target", "display]text", true),
                // Multiple escaped characters
                (
                    "test\\]with\\[brackets]]",
                    "test]with[brackets",
                    "test]with[brackets",
                    false,
                ),
            ];

            for (input, target, display, is_alias) in test_cases {
                assert_parse_wikilink(input, target, display, is_alias);
            }
        }

        #[test]
        fn test_parse_wikilink_special_chars() {
            let test_cases = vec![
                ("!@#$%^&*()]]", "!@#$%^&*()", "!@#$%^&*()", false),
                (
                    "../path/to/file]]",
                    "../path/to/file",
                    "../path/to/file",
                    false,
                ),
                ("file (1)]]", "file (1)", "file (1)", false),
                ("file (1)|version 1]]", "file (1)", "version 1", true),
                (
                    "outer [inner] text]]",
                    "outer [inner] text",
                    "outer [inner] text",
                    false,
                ),
                ("target|(text)]]", "target", "(text)", true),
            ];

            for (input, target, display, is_alias) in test_cases {
                assert_parse_wikilink(input, target, display, is_alias);
            }
        }

        #[test]
        fn test_parse_wikilink_invalid() {
            let invalid_cases = vec![
                // Missing closing brackets entirely
                "unclosed",
                "unclosed|alias",
                // Single closing bracket
                "missing]",
                // Empty content
                "",
            ];

            for input in invalid_cases {
                let mut chars = input.char_indices().peekable();
                assert!(
                    parse_wikilink(&mut chars).is_none(),
                    "Expected None for invalid input: {}",
                    input
                );
            }
        }
    }

    // Sub-module for error handling
    mod error_handling {
        use super::*;

        #[test]
        fn compile_wikilink_invalid_patterns() {
            let test_cases = vec![
                ("test[[invalid", WikilinkErrorType::ContainsOpenBrackets),
                ("test]]invalid", WikilinkErrorType::ContainsCloseBrackets),
                ("test|invalid", WikilinkErrorType::ContainsPipe),
            ];

            for (pattern, expected_error) in test_cases {
                let wikilink = Wikilink {
                    display_text: pattern.to_string(),
                    target: "test".to_string(),
                    is_alias: false,
                };

                let result = compile_wikilink(wikilink);
                assert!(
                    result.is_err(),
                    "Pattern '{}' should produce an error",
                    pattern
                );

                if let Err(error) = result {
                    assert_eq!(
                        error.error_type, expected_error,
                        "Unexpected error type for pattern '{}'",
                        pattern
                    );
                }
            }
        }

        #[test]
        fn wikilink_error_display() {
            let error = WikilinkError {
                display_text: "test[[bad]]".to_string(),
                error_type: WikilinkErrorType::ContainsOpenBrackets,
                file_path: String::new(),
                line_number: None,
                line_content: None,
            };

            assert_eq!(
                error.to_string().trim(),
                "Invalid wikilink pattern: 'test[[bad]]' contains opening brackets '[['"
            );
        }
    }

    // Submodule for Markdown link tests
    mod markdown_links {
        use super::*;

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
                assert!(regex.is_match(case), "Regex should match '{}'", case);
            }

            let non_matching_cases = vec![
                "plain text",
                "[[wikilink]]",
                "![[imagelink]]",
                "[incomplete",
            ];

            for case in non_matching_cases {
                assert!(!regex.is_match(case), "Regex should not match '{}'", case);
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

    // Additional sub-modules and tests can be added similarly...
}

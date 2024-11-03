use std::cmp::PartialEq;
use crate::{constants::*, frontmatter::FrontMatter};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
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
    pub context: WikilinkErrorContext,
}

impl WikilinkError {
    // Add a method to add context to an existing error
    pub fn with_context(
        mut self,
        file_path: Option<&Path>,
        line_number: Option<usize>,
        line_content: Option<&str>,
    ) -> Self {
        self.context = WikilinkErrorContext {
            file_path: file_path.map(|p| p.display().to_string()),
            line_number,
            line_content: line_content.map(String::from),
        };
        self
    }
}

impl fmt::Display for WikilinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let error_msg = match self.error_type {
            WikilinkErrorType::ContainsOpenBrackets => "contains opening brackets '[['",
            WikilinkErrorType::ContainsCloseBrackets => "contains closing brackets ']]'",
            WikilinkErrorType::ContainsPipe => "contains pipe character '|'",
        };
        write!(
            f,
            "Invalid wikilink pattern: '{}' {}",
            self.display_text, error_msg
        )
    }
}

impl Error for WikilinkError {}

#[derive(Debug)]
pub enum WikilinkErrorType {
    ContainsOpenBrackets,
    ContainsCloseBrackets,
    ContainsPipe,
}

#[derive(Debug, Default)]
pub struct WikilinkErrorContext {
    pub file_path: Option<String>,
    pub line_number: Option<usize>,
    pub line_content: Option<String>,
}

impl fmt::Display for WikilinkErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.file_path {
            writeln!(f, "File: {}", path)?;
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
    file_path: Option<&Path>,
    line_number: Option<usize>,
    line_content: Option<&str>,
) -> Result<CompiledWikilink, WikilinkError> {
    compile_wikilink(wikilink).map_err(|e| e.with_context(file_path, line_number, line_content))
}

pub fn compile_wikilink(wikilink: Wikilink) -> Result<CompiledWikilink, WikilinkError> {
    let search_text = &wikilink.display_text;

    // Check for invalid characters
    if search_text.contains("[[") {
        return Err(WikilinkError {
            display_text: search_text.to_string(),
            error_type: WikilinkErrorType::ContainsOpenBrackets,
            context: WikilinkErrorContext::default(),
        });
    }
    if search_text.contains("]]") {
        return Err(WikilinkError {
            display_text: search_text.to_string(),
            error_type: WikilinkErrorType::ContainsCloseBrackets,
            context: WikilinkErrorContext::default(),
        });
    }
    if search_text.contains("|") {
        return Err(WikilinkError {
            display_text: search_text.to_string(),
            error_type: WikilinkErrorType::ContainsPipe,
            context: WikilinkErrorContext::default(),
        });
    }

    Ok(CompiledWikilink::new(wikilink))
}

pub fn collect_all_wikilinks(
    content: &str,
    frontmatter: &Option<FrontMatter>,
    filename: &str,
    file_path: Option<&Path>,
) -> Result<HashSet<CompiledWikilink>, WikilinkError> {
    let mut all_wikilinks = HashSet::new();

    // Add filename-based wikilink
    let filename_wikilink = create_filename_wikilink(filename);
    let compiled = compile_wikilink_with_context(filename_wikilink.clone(), file_path, None, None)?;
    all_wikilinks.insert(compiled);

    // Add frontmatter aliases
    if let Some(fm) = frontmatter {
        if let Some(aliases) = fm.aliases() {
            for alias in aliases {
                let wikilink = Wikilink {
                    display_text: alias.clone(),
                    target: filename_wikilink.target.clone(),
                    is_alias: true,
                };
                let compiled = compile_wikilink_with_context(wikilink, file_path, None, None)?;
                all_wikilinks.insert(compiled);
            }
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
    #[derive(PartialEq)]
    enum State {
        Target,
        Display,
    }

    use State::*;

    let mut state = Target;
    let mut target = String::new();
    let mut display = String::new();
    let mut escape = false;

    while let Some((_, c)) = chars.next() {
        if escape {
            if c == '|' {
                // Escaped pipe acts as a separator
                state = Display;
            } else {
                // Add the escaped character to the current state
                match state {
                    Target => target.push(c),
                    Display => display.push(c),
                }
            }
            escape = false;
            continue;
        }

        match c {
            '\\' => {
                escape = true;
            }
            '|' => {
                // Unescaped pipe indicates a transition to Display
                state = Display;
            }
            ']' => {
                if is_next_char(chars, ']') {
                    // Closing brackets found, finalize the Wikilink
                    return match state {
                        Target => {
                            let trimmed = target.trim().to_string();
                            Some(Wikilink {
                                display_text: trimmed.clone(),
                                target: trimmed,
                                is_alias: false,
                            })
                        }
                        Display => {
                            let trimmed_target = target.trim().to_string();
                            let trimmed_display = display.trim().to_string();
                            Some(Wikilink {
                                display_text: trimmed_display,
                                target: trimmed_target,
                                is_alias: true,
                            })
                        }
                    }
                } else {
                    // Not a closing bracket, add ']' to the current state
                    match state {
                        Target => target.push(c),
                        Display => display.push(c),
                    }
                }
            }
            _ => {
                // Regular character, add to the current state
                match state {
                    Target => target.push(c),
                    Display => display.push(c),
                }
            }
        }
    }

    // Wikilink not properly closed
    None
}

/// Helper function to check if the next character matches the expected one
fn is_next_char(
    chars: &mut std::iter::Peekable<std::str::CharIndices>,
    expected: char,
) -> bool {
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
    use crate::frontmatter;

    #[test]
    fn test_collect_all_wikilinks() {
        let content = r#"---
aliases:
  - "Alias One"
  - "Alias Two"
---
# Test
Here's a [[Regular Link]] and [[Target|Display Text]]
Also [[Alias One]] is referenced"#;

        let frontmatter = frontmatter::deserialize_frontmatter(content).unwrap();
        let wikilinks =
            collect_all_wikilinks(content, &Some(frontmatter), "test file.md", None).unwrap();

        assert!(wikilinks
            .iter()
            .any(|w| w.wikilink.display_text == "test file"));
        assert!(wikilinks
            .iter()
            .any(|w| w.wikilink.display_text == "Alias One"));
        assert!(wikilinks
            .iter()
            .any(|w| w.wikilink.display_text == "Alias Two"));
        assert!(wikilinks
            .iter()
            .any(|w| w.wikilink.display_text == "Regular Link"));
        assert!(wikilinks.iter().any(|w| {
            w.wikilink.display_text == "Display Text" && w.wikilink.target == "Target"
        }));
    }

    #[test]
    fn test_create_filename_wikilink() {
        let wikilink = create_filename_wikilink("test file.md");
        assert_eq!(wikilink.display_text, "test file");
        assert_eq!(wikilink.target, "test file");
        assert!(!wikilink.is_alias);

        let wikilink = create_filename_wikilink("test file");
        assert_eq!(wikilink.display_text, "test file");
        assert_eq!(wikilink.target, "test file");
        assert!(!wikilink.is_alias);
    }

    #[test]
    fn test_hash_equality() {
        use std::collections::HashSet;

        let wikilink1 = Wikilink {
            display_text: "Test".to_string(),
            target: "Test".to_string(),
            is_alias: false,
        };
        let wikilink2 = Wikilink {
            display_text: "Test".to_string(),
            target: "Test".to_string(),
            is_alias: false,
        };

        let compiled1 = compile_wikilink(wikilink1).unwrap();
        let compiled2 = compile_wikilink(wikilink2).unwrap();

        let mut set = HashSet::new();
        set.insert(compiled1);
        assert!(!set.insert(compiled2), "Duplicate wikilink was inserted");
    }

    #[test]
    fn test_extract_wikilinks_with_escaped_pipes() {
        // Test case with escaped pipe in table
        let content = "| [[Federal Hill\\|Fed Hill]] | description |";
        let wikilinks = extract_wikilinks_from_content(content);

        assert_eq!(wikilinks.len(), 1);
        assert_eq!(wikilinks[0].target, "Federal Hill");
        assert_eq!(wikilinks[0].display_text, "Fed Hill");
        assert!(wikilinks[0].is_alias);

        // Test multiple wikilinks with mixed escaping
        let content = "[[Normal Link]] and [[Place\\|Alias]] and [[Other|Other Alias]]";
        let wikilinks = extract_wikilinks_from_content(content);

        assert_eq!(wikilinks.len(), 3);

        // Check normal link
        assert_eq!(wikilinks[0].target, "Normal Link");
        assert_eq!(wikilinks[0].display_text, "Normal Link");
        assert!(!wikilinks[0].is_alias);

        // Check escaped pipe link
        assert_eq!(wikilinks[1].target, "Place");
        assert_eq!(wikilinks[1].display_text, "Alias");
        assert!(wikilinks[1].is_alias);

        // Check unescaped pipe link
        assert_eq!(wikilinks[2].target, "Other");
        assert_eq!(wikilinks[2].display_text, "Other Alias");
        assert!(wikilinks[2].is_alias);
    }

    #[test]
    fn test_extract_wikilinks_with_unicode() {
        let content = "Here is a [[リンク]] and [[目标|显示文本]] with Unicode.";
        let wikilinks = extract_wikilinks_from_content(content);

        assert_eq!(wikilinks.len(), 2);
        assert_eq!(wikilinks[0].target, "リンク");
        assert_eq!(wikilinks[0].display_text, "リンク");
        assert!(!wikilinks[0].is_alias);

        assert_eq!(wikilinks[1].target, "目标");
        assert_eq!(wikilinks[1].display_text, "显示文本");
        assert!(wikilinks[1].is_alias);
    }

    #[test]
    fn test_extract_wikilinks_with_whitespace() {
        let content = "[[  Spaced Link  ]] and [[  Target  \\|  Alias  ]]";
        let wikilinks = extract_wikilinks_from_content(content);

        assert_eq!(wikilinks.len(), 2);
        assert_eq!(wikilinks[0].target, "Spaced Link");
        assert_eq!(wikilinks[1].target, "Target");
        assert_eq!(wikilinks[1].display_text, "Alias");
    }

    #[test]
    fn test_extract_wikilinks_in_table() {
        let content = "| Header 1 | Header 2 |\n|---|---|\n| [[Place\\|Alias]] | text |";
        let wikilinks = extract_wikilinks_from_content(content);

        assert_eq!(wikilinks.len(), 1);
        assert_eq!(wikilinks[0].target, "Place");
        assert_eq!(wikilinks[0].display_text, "Alias");
        assert!(wikilinks[0].is_alias);
    }

    #[test]
    fn test_ignore_image_wikilinks() {
        let content = r#"
Here is a [[normal link]]
And ![[image.png|500]] should be ignored
Also ![[another image.jpg]] ignored
But [[regular|alias]] works
"#;
        let wikilinks = extract_wikilinks_from_content(content);

        assert_eq!(
            wikilinks.len(),
            2,
            "Should only extract non-image wikilinks"
        );

        assert!(wikilinks.iter().any(|w| w.target == "normal link"));
        assert!(wikilinks
            .iter()
            .any(|w| w.target == "regular" && w.display_text == "alias"));

        assert!(!wikilinks.iter().any(|w| w.target.ends_with(".png")));
        assert!(!wikilinks.iter().any(|w| w.target.ends_with(".jpg")));
    }

    #[test]
    fn test_mixed_wikilinks_with_images() {
        let content = r#"
![[shea butter 20240914234106.png|500]]
[[Shea Butter]] is great for skin
Some more ![[coconut_oil.jpg|200]] images
[[Coconut Oil|Coconut]] is also good
"#;
        let wikilinks = extract_wikilinks_from_content(content);

        assert_eq!(wikilinks.len(), 2, "Should only have non-image wikilinks");
        assert!(wikilinks.iter().any(|w| w.target == "Shea Butter"));
        assert!(wikilinks
            .iter()
            .any(|w| w.target == "Coconut Oil" && w.display_text == "Coconut"));
    }

    #[test]
    fn test_exclamation_mark_handling() {
        let content = r#"
This is amazing! [[normal link]] (exclamation not part of link)
![[image.jpg]] (image link)
text! ![[image2.jpg]] (exclamation before image)
"#;
        let wikilinks = extract_wikilinks_from_content(content);

        assert_eq!(wikilinks.len(), 1, "Should only extract the normal link");
        assert_eq!(wikilinks[0].target, "normal link");
    }

    #[test]
    fn test_markdown_links() {
        let regex = MARKDOWN_REGEX.clone();

        // External links
        assert!(regex.is_match("[text](https://example.com)"));
        assert!(regex.is_match("[link](http://test.com)"));

        // Internal links
        assert!(regex.is_match("[page](folder/page.md)"));
        assert!(regex.is_match("[img](../images/test.png)"));

        // Links with titles
        assert!(regex.is_match("[text](path 'title')"));
        assert!(regex.is_match("[text](path \"title\")"));

        // Invalid links that should still be excluded
        assert!(regex.is_match("[](path)"));
        assert!(regex.is_match("[text]()"));
        assert!(regex.is_match("[]()"));

        // Non-matches
        assert!(!regex.is_match("plain text"));
        assert!(!regex.is_match("[[wikilink]]"));
        assert!(!regex.is_match("![[imagelink]]"));
        assert!(!regex.is_match("[incomplete"));
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

    #[test]
    fn test_compile_wikilink_invalid_patterns() {
        let test_cases = vec![
            (
                "test[[invalid",
                WikilinkErrorType::ContainsOpenBrackets,
                "should reject pattern with opening brackets",
            ),
            (
                "test]]invalid",
                WikilinkErrorType::ContainsCloseBrackets,
                "should reject pattern with closing brackets",
            ),
            (
                "test|invalid",
                WikilinkErrorType::ContainsPipe,
                "should reject pattern with pipe",
            ),
        ];

        for (pattern, _expected_error, message) in test_cases {
            let wikilink = Wikilink {
                display_text: pattern.to_string(),
                target: "test".to_string(),
                is_alias: false,
            };

            let result = compile_wikilink(wikilink);
            assert!(result.is_err(), "{}", message);

            if let Err(error) = result {
                assert!(matches!(error.error_type, _expected_error), "{}", message);
            }
        }
    }

    #[test]
    fn test_wikilink_error_display() {
        let error = WikilinkError {
            display_text: "test[[bad]]".to_string(),
            error_type: WikilinkErrorType::ContainsOpenBrackets,
            context: WikilinkErrorContext::default(),
        };

        assert_eq!(
            error.to_string(),
            "Invalid wikilink pattern: 'test[[bad]]' contains opening brackets '[['"
        );
    }

    #[test]
    fn test_to_wikilink() {
        // Test &str
        assert_eq!("test".to_wikilink(), "[[test]]");

        // Test String
        let string = String::from("test");
        assert_eq!(string.to_wikilink(), "[[test]]");

        // Test &String
        let string_ref = &String::from("test");
        assert_eq!(string_ref.to_wikilink(), "[[test]]");

        // Test with spaces
        assert_eq!("test file".to_wikilink(), "[[test file]]");

        // Test with existing brackets (should still wrap)
        assert_eq!("[[test]]".to_wikilink(), "[[[[test]]]]");

        // Test empty string
        assert_eq!("".to_wikilink(), "[[]]");

        // Test .md extension handling
        assert_eq!("test.md".to_wikilink(), "[[test]]");
        assert_eq!("test file.md".to_wikilink(), "[[test file]]");
        assert_eq!("test.md.md".to_wikilink(), "[[test.md]]");

        // Test with unicode
        assert_eq!("テスト.md".to_wikilink(), "[[テスト]]");
        assert_eq!("test café.md".to_wikilink(), "[[test café]]");
    }

    #[test]
    fn test_to_aliased_wikilink() {
        // Test exact matches (should use simple wikilink)
        assert_eq!("target".to_aliased_wikilink("target"), "[[target]]");
        assert_eq!(
            "Test Link".to_aliased_wikilink("Test Link"),
            "[[Test Link]]"
        );

        // Test case differences (should use aliased format)
        assert_eq!("Target".to_aliased_wikilink("target"), "[[Target|target]]");
        assert_eq!(
            "test link".to_aliased_wikilink("Test Link"),
            "[[test link|Test Link]]"
        );

        // Test completely different texts
        assert_eq!("Apple".to_aliased_wikilink("fruit"), "[[Apple|fruit]]");
        assert_eq!("Home".to_aliased_wikilink("主页"), "[[Home|主页]]");

        // Test with .md extension in target
        assert_eq!("page.md".to_aliased_wikilink("Page"), "[[page|Page]]");
    }

    #[test]
    fn test_to_aliased_wikilink_with_spaces() {
        assert_eq!(
            "Target Page".to_aliased_wikilink("target page"),
            "[[Target Page|target page]]"
        );
        assert_eq!(
            "My Document.md".to_aliased_wikilink("my doc"),
            "[[My Document|my doc]]"
        );
    }

    #[test]
    fn test_to_aliased_wikilink_with_unicode() {
        assert_eq!("café".to_aliased_wikilink("咖啡"), "[[café|咖啡]]");
        assert_eq!("テスト".to_aliased_wikilink("Test"), "[[テスト|Test]]");
    }
    #[test]
    fn test_string_to_aliased_wikilink() {
        // Test with String type
        let string_target = String::from("Target");
        assert_eq!(
            string_target.to_aliased_wikilink("target"),
            "[[Target|target]]"
        );
        assert_eq!(string_target.to_aliased_wikilink("Target"), "[[Target]]");
    }
}

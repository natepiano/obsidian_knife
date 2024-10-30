use crate::frontmatter::FrontMatter;
use crate::{CLOSING_WIKILINK, OPENING_WIKILINK};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::path::Path;

lazy_static! {
    static ref WIKILINK_REGEX: fancy_regex::Regex =
        fancy_regex::Regex::new(r"\[\[(.*?)(?:\\?\|(.*?))?\]\]").unwrap();
    pub static ref MARKDOWN_REGEX: regex::Regex = regex::Regex::new(r"\[.*?\]\(.*?\)").unwrap();
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Wikilink {
    pub display_text: String,
    pub target: String,
    pub is_alias: bool,
}

#[derive(Debug, Clone)]
pub struct CompiledWikilink {
    pub regex: regex::Regex,
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
    pub fn new(regex: regex::Regex, wikilink: Wikilink) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        wikilink.hash(&mut hasher);
        let hash = hasher.finish();

        CompiledWikilink {
            regex,
            wikilink,
            hash,
        }
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

pub(crate) fn compile_wikilink(wikilink: Wikilink) -> CompiledWikilink {
    let search_text = &wikilink.display_text;

    // Escape the text to create a literal match for the exact phrase
    let escaped_pattern = regex::escape(search_text);

    // Add case-insensitive flag with simple word boundaries
    let pattern = format!(r"(?i)\b{}\b", escaped_pattern);

    CompiledWikilink::new(regex::Regex::new(&pattern).unwrap(), wikilink)
}

pub fn collect_all_wikilinks(
    content: &str,
    frontmatter: &Option<FrontMatter>,
    filename: &str,
) -> HashSet<CompiledWikilink> {
    let mut all_wikilinks = HashSet::new();

    // Add filename-based wikilink
    let filename_wikilink = create_filename_wikilink(filename);
    all_wikilinks.insert(compile_wikilink(filename_wikilink.clone()));

    // Add frontmatter aliases
    if let Some(fm) = frontmatter {
        if let Some(aliases) = fm.aliases() {
            all_wikilinks.extend(aliases.iter().map(|alias| {
                compile_wikilink(Wikilink {
                    display_text: alias.clone(),
                    target: filename_wikilink.target.clone(),
                    is_alias: true,
                })
            }));
        }
    }

    // Add wikilinks from content
    all_wikilinks.extend(
        extract_wikilinks_from_content(content)
            .into_iter()
            .map(compile_wikilink),
    );

    all_wikilinks
}

pub fn extract_wikilinks_from_content(content: &str) -> Vec<Wikilink> {
    let mut wikilinks = Vec::new();

    for cap_result in WIKILINK_REGEX.captures_iter(content) {
        if let Ok(cap) = cap_result {
            // Get the full match start position to check for exclamation mark
            let full_match = cap.get(0).unwrap();
            let match_start = full_match.start();

            // Skip if this match starts with an exclamation mark
            if match_start > 0 && content.as_bytes()[match_start - 1] == b'!' {
                continue;
            }

            if let Some(full_phrase) = cap.get(1).map(|m| m.as_str()) {
                let wikilink = if let Some(alias) = cap.get(2).map(|m| m.as_str()) {
                    // Clean up the full_phrase by removing the escaped pipe if present
                    let target = if full_phrase.ends_with('\\') {
                        // Remove trailing backslash
                        full_phrase[..full_phrase.len() - 1].to_string()
                    } else {
                        full_phrase.to_string()
                    };

                    Wikilink {
                        display_text: alias.trim().to_string(),
                        target: target.trim().to_string(),
                        is_alias: true,
                    }
                } else {
                    Wikilink {
                        display_text: full_phrase.trim().to_string(),
                        target: full_phrase.trim().to_string(),
                        is_alias: false,
                    }
                };
                wikilinks.push(wikilink);
            }
        }
    }

    wikilinks
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
        let wikilinks = collect_all_wikilinks(content, &Some(frontmatter), "test file.md");

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
    fn test_compile_wikilink_whole_word() {
        let wikilink = Wikilink {
            display_text: "Test Link".to_string(),
            target: "Test Link".to_string(),
            is_alias: false,
        };
        let compiled = compile_wikilink(wikilink);

        assert!(compiled.regex.is_match("Here is Test Link here"));
        assert!(!compiled.regex.is_match("TestLink"));
        assert!(!compiled.regex.is_match("The TestLink is here"));
    }

    #[test]
    fn test_compile_wikilink_with_punctuation() {
        let wikilink = Wikilink {
            display_text: "Test Link".to_string(),
            target: "Test Link".to_string(),
            is_alias: false,
        };
        let compiled = compile_wikilink(wikilink);

        assert!(compiled.regex.is_match("Here is Test Link."));
        assert!(compiled.regex.is_match("(Test Link)"));
        assert!(compiled.regex.is_match("Test Link;"));
        assert!(compiled.regex.is_match("'Test Link'"));
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

        let compiled1 = compile_wikilink(wikilink1);
        let compiled2 = compile_wikilink(wikilink2);

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
    fn test_word_boundary_with_punctuation() {
        let wikilink = Wikilink {
            display_text: "Ed".to_string(),
            target: "Ed Barnes".to_string(),
            is_alias: true,
        };
        let compiled = compile_wikilink(wikilink);

        // Punctuation creates word boundaries
        assert!(
            compiled.regex.is_match("(Ed)"),
            "Parentheses create word boundaries"
        );
        assert!(
            compiled.regex.is_match("[Ed]"),
            "Brackets create word boundaries"
        );
        assert!(
            compiled.regex.is_match(".Ed."),
            "Periods create word boundaries"
        );
        assert!(
            compiled.regex.is_match(",Ed,"),
            "Commas create word boundaries"
        );
        assert!(
            compiled.regex.is_match("Ed:"),
            "Colon creates word boundary"
        );
        assert!(
            compiled.regex.is_match("Ed: note"),
            "Colon creates word boundary"
        );

        // No word boundaries within words
        assert!(
            !compiled.regex.is_match("Editor"),
            "Should not match within word"
        );
        assert!(
            !compiled.regex.is_match("fedEx"),
            "Should not match within word"
        );

        // Space + punctuation combinations
        assert!(
            compiled.regex.is_match("Hello (Ed)"),
            "Space + parens works"
        );
        assert!(
            compiled.regex.is_match("Hello [Ed]"),
            "Space + brackets works"
        );
        assert!(
            compiled.regex.is_match("Hello Ed!"),
            "Space + exclamation works"
        );
    }

    #[test]
    fn test_word_boundaries_with_different_apostrophes() {
        let wikilink = Wikilink {
            display_text: "t".to_string(),
            target: "test".to_string(),
            is_alias: true,
        };
        let compiled = compile_wikilink(wikilink);

        // Testing with straight apostrophe (U+0027)
        assert!(
            compiled.regex.is_match("don't"),
            "Should match 't' after straight apostrophe"
        );
        assert!(
            compiled.regex.is_match("can't"),
            "Should match 't' in another contraction"
        );

        // Testing with curly apostrophe (U+2019)
        assert!(
            compiled.regex.is_match("don\u{2019}t"),
            "Should match 't' after curly apostrophe"
        );
        assert!(
            compiled.regex.is_match("can\u{2019}t"),
            "Should match 't' in another contraction"
        );

        // Test that 'don' is also a separate word
        let don_wikilink = Wikilink {
            display_text: "don".to_string(),
            target: "do not".to_string(),
            is_alias: true,
        };
        let don_compiled = compile_wikilink(don_wikilink);

        assert!(
            don_compiled.regex.is_match("don't"),
            "Should match 'don' before straight apostrophe"
        );
        assert!(
            don_compiled.regex.is_match("don\u{2019}t"),
            "Should match 'don' before curly apostrophe"
        );
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
}

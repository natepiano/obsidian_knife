use lazy_static::lazy_static;
use std::collections::HashSet;
use serde::{Deserialize, Serialize};
use crate::frontmatter::FrontMatter;

lazy_static! {
    static ref WIKILINK_REGEX: fancy_regex::Regex = fancy_regex::Regex::new(r"\[\[(.*?)(?:\|(.*?))?\]\]").unwrap();
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Wikilink {
    pub display_text: String,
    pub target: String,
    pub is_alias: bool,
}

#[derive(Debug, Clone)]
pub struct CompiledWikilink {
    pub regex: fancy_regex::Regex,
    pub wikilink: Wikilink,
    hash: u64,
}

impl CompiledWikilink {
    pub fn new(regex: fancy_regex::Regex, wikilink: Wikilink) -> Self {
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

    pub fn to_string(&self) -> String {
        if self.wikilink.is_alias {
            format!("[[{}|{}]]", self.wikilink.target, self.wikilink.display_text)
        } else {
            format!("[[{}]]", self.wikilink.target)
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

pub fn find_wikilinks_in_line(line: &str) -> Vec<fancy_regex::Match> {
    WIKILINK_REGEX
        .find_iter(line)
        .filter_map(|m| m.ok())
        .collect()
}

pub fn create_filename_wikilink(filename: &str) -> Wikilink {
    let display_text = filename
        .strip_suffix(".md")
        .unwrap_or(filename)
        .to_string();

    Wikilink {
        display_text: display_text.clone(),
        target: display_text,
        is_alias: false,
    }
}

pub fn compile_wikilink(wikilink: Wikilink) -> CompiledWikilink {
    let search_text = &wikilink.display_text;
    let is_whole_word = search_text
        .chars()
        .all(|c| c.is_alphanumeric() || c.is_whitespace() || c == '|');

    let pattern = create_case_insensitive_regex_pattern(search_text, is_whole_word);

    CompiledWikilink::new(
        fancy_regex::Regex::new(&pattern).unwrap(),
        wikilink
    )
}

fn create_case_insensitive_regex_pattern(pattern: &str, is_whole_word: bool) -> String {
    let trimmed_pattern = pattern.trim();
    let escaped_pattern = regex::escape(trimmed_pattern);
    if is_whole_word {
        format!(
            r"(?i)(?<![^\s\p{{P}}|]){}(?![^\s\p{{P}}|])",
            escaped_pattern
        )
    } else {
        format!(r"(?i){}", escaped_pattern)
    }
}

pub fn parse_wikilink(text: &str) -> Option<Wikilink> {
    if let Ok(Some(cap)) = WIKILINK_REGEX.captures(text) {
        if let Some(full_phrase) = cap.get(1).map(|m| m.as_str()) {
            if let Some(alias) = cap.get(2).map(|m| m.as_str()) {
                Some(Wikilink {
                    display_text: alias.to_string(),
                    target: full_phrase.to_string(),
                    is_alias: true,
                })
            } else {
                Some(Wikilink {
                    display_text: full_phrase.to_string(),
                    target: full_phrase.to_string(),
                    is_alias: false,
                })
            }
        } else {
            None
        }
    } else {
        None
    }
}

pub fn collect_all_wikilinks(
    content: &str,
    frontmatter: &Option<FrontMatter>,
    filename: &str
) -> HashSet<CompiledWikilink> {
    let mut all_wikilinks = HashSet::new();

    // Add filename-based wikilink
    let filename_wikilink = create_filename_wikilink(filename);
    all_wikilinks.insert(compile_wikilink(filename_wikilink));

    // Add frontmatter aliases
    if let Some(fm) = frontmatter {
        if let Some(aliases) = fm.aliases() {
            for alias in aliases {
                let alias_wikilink = Wikilink {
                    display_text: alias.clone(),
                    target: alias.clone(),
                    is_alias: false,
                };
                all_wikilinks.insert(compile_wikilink(alias_wikilink));
            }
        }
    }

    // Add wikilinks from content
    let content_wikilinks = extract_wikilinks_from_content(content);
    for wikilink in content_wikilinks {
        all_wikilinks.insert(compile_wikilink(wikilink));
    }

    all_wikilinks
}

pub fn extract_wikilinks_from_content(content: &str) -> Vec<Wikilink> {
    let mut wikilinks = Vec::new();

    for cap_result in WIKILINK_REGEX.captures_iter(content) {
        if let Ok(cap) = cap_result {
            if let Some(full_phrase) = cap.get(1).map(|m| m.as_str()) {
                let wikilink = if let Some(alias) = cap.get(2).map(|m| m.as_str()) {
                    Wikilink {
                        display_text: alias.to_string(),
                        target: full_phrase.to_string(),
                        is_alias: true,
                    }
                } else {
                    Wikilink {
                        display_text: full_phrase.to_string(),
                        target: full_phrase.to_string(),
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
    use crate::{frontmatter, wikilink};
    use crate::scan::MarkdownFileInfo;

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

        assert!(wikilinks.iter().any(|w| w.wikilink.display_text == "test file"));
        assert!(wikilinks.iter().any(|w| w.wikilink.display_text == "Alias One"));
        assert!(wikilinks.iter().any(|w| w.wikilink.display_text == "Alias Two"));
        assert!(wikilinks.iter().any(|w| w.wikilink.display_text == "Regular Link"));
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

        assert!(compiled.regex.is_match("Here is Test Link here").unwrap());
        assert!(!compiled.regex.is_match("TestLink").unwrap());
        assert!(!compiled.regex.is_match("The TestLink is here").unwrap());
    }

    #[test]
    fn test_compile_wikilink_with_punctuation() {
        let wikilink = Wikilink {
            display_text: "Test Link".to_string(),
            target: "Test Link".to_string(),
            is_alias: false,
        };
        let compiled = compile_wikilink(wikilink);

        assert!(compiled.regex.is_match("Here is Test Link.").unwrap());
        assert!(compiled.regex.is_match("(Test Link)").unwrap());
        assert!(compiled.regex.is_match("Test Link;").unwrap());
        assert!(compiled.regex.is_match("'Test Link'").unwrap());
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
    fn test_parse_wikilink() {
        // Test regular wikilink
        let wikilink = parse_wikilink("[[Test Link]]").unwrap();
        assert_eq!(wikilink.display_text, "Test Link");
        assert_eq!(wikilink.target, "Test Link");
        assert!(!wikilink.is_alias);

        // Test aliased wikilink
        let wikilink = parse_wikilink("[[Target|Display Text]]").unwrap();
        assert_eq!(wikilink.display_text, "Display Text");
        assert_eq!(wikilink.target, "Target");
        assert!(wikilink.is_alias);

        // Test invalid wikilink
        assert!(parse_wikilink("Not a wikilink").is_none());
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
    fn test_alias_wikilink_parsing() {
        let wikilink = parse_wikilink("[[Target|Display Text]]").unwrap();
        assert_eq!(wikilink.display_text, "Display Text");
        assert_eq!(wikilink.target, "Target");
        assert!(wikilink.is_alias);

        let extracted = extract_wikilinks_from_content("Here is a [[Target|Display Text]] with alias");
        assert_eq!(extracted.len(), 1);
        let first = &extracted[0];
        assert_eq!(first.display_text, "Display Text");
        assert_eq!(first.target, "Target");
        assert!(first.is_alias);
    }

    #[test]
    fn test_find_wikilinks_in_line() {
        let line = "Here is a [[Simple Link]] and a [[Target|Aliased Link]] together";
        let matches = find_wikilinks_in_line(line);

        assert_eq!(matches.len(), 2);
        assert_eq!(&line[matches[0].start()..matches[0].end()], "[[Simple Link]]");
        assert_eq!(&line[matches[1].start()..matches[1].end()], "[[Target|Aliased Link]]");
    }
}

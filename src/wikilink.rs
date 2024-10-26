// wikilinks.rs
pub struct Wikilink {
    pub display_text: String,
    pub target: String,
    pub is_alias: bool,
}

pub struct CompiledWikilink {
    pub regex: fancy_regex::Regex,
    pub wikilink: Wikilink,
}

impl CompiledWikilink {
    pub fn to_string(&self) -> String {
        if self.wikilink.is_alias {
            format!("[[{}|{}]]", self.wikilink.target, self.wikilink.display_text)
        } else {
            format!("[[{}]]", self.wikilink.target)
        }
    }
}

pub fn parse_wikilink(text: &str) -> Option<Wikilink> {
    let bracket_regex = fancy_regex::Regex::new(r"\[\[(.*?)(?:\|(.*?))?\]\]").unwrap();

    if let Ok(Some(cap)) = bracket_regex.captures(text) {
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

pub fn compile_wikilink(wikilink: Wikilink) -> CompiledWikilink {
    let search_text = &wikilink.display_text;
    let is_whole_word = search_text
        .chars()
        .all(|c| c.is_alphanumeric() || c.is_whitespace() || c == '|');

    let regex = create_case_insensitive_regex(search_text, is_whole_word);

    CompiledWikilink {
        regex,
        wikilink,
    }
}

pub fn create_case_insensitive_regex(pattern: &str, is_whole_word: bool) -> fancy_regex::Regex {
    let trimmed_pattern = pattern.trim();
    let escaped_pattern = regex::escape(trimmed_pattern);
    let regex_str = if is_whole_word {
        format!(
            r"(?i)(?<![^\s\p{{P}}|]){}(?![^\s\p{{P}}|])",
            escaped_pattern
        )
    } else {
        format!(r"(?i){}", escaped_pattern)
    };
    fancy_regex::Regex::new(&regex_str).unwrap()
}

pub fn extract_wikilinks_from_content(content: &str) -> Vec<Wikilink> {
    let bracket_regex = fancy_regex::Regex::new(r"\[\[(.*?)(?:\|(.*?))?\]\]").unwrap();
    let mut wikilinks = Vec::new();

    for cap_result in bracket_regex.captures_iter(content) {
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

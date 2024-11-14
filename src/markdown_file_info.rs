use crate::file_utils::{get_file_creation_time, read_contents_from_file};
use crate::frontmatter::FrontMatter;
use crate::regex_utils::build_case_insensitive_word_finder;
use crate::wikilink_types::InvalidWikilink;
use crate::yaml_frontmatter::{YamlFrontMatter, YamlFrontMatterError};
use chrono::{DateTime, Local};
use regex::Regex;
use std::error::Error;
use std::path::PathBuf;

#[derive(Debug)]
pub struct MarkdownFileInfo {
    pub created_time: DateTime<Local>,
    pub content: String,
    pub do_not_back_populate_regexes: Option<Vec<Regex>>,
    pub frontmatter: Option<FrontMatter>,
    pub frontmatter_error: Option<YamlFrontMatterError>,
    pub image_links: Vec<String>,
    pub invalid_wikilinks: Vec<InvalidWikilink>,
    pub path: PathBuf,
}

impl MarkdownFileInfo {
    pub fn new(path: PathBuf) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let content = read_contents_from_file(&path)?;
        let created_time = get_file_creation_time(&path)?;

        let (frontmatter, frontmatter_error) = match FrontMatter::from_markdown_str(&content) {
            Ok(fm) => (Some(fm), None),
            Err(error) => (None, Some(error)),
        };

        let do_not_back_populate_regexes = Self::get_do_not_back_populate_regexes(&frontmatter);

        Ok(MarkdownFileInfo {
            created_time,
            content,
            do_not_back_populate_regexes,
            frontmatter,
            frontmatter_error,
            invalid_wikilinks: Vec::new(),
            image_links: Vec::new(),
            path,
        })
    }

    fn get_do_not_back_populate_regexes(frontmatter: &Option<FrontMatter>) -> Option<Vec<Regex>> {
        if let Some(fm) = &frontmatter {
            // first get do_not_back_populate explicit value
            let mut do_not_populate = fm.do_not_back_populate.clone().unwrap_or_default();

            // if there are aliases, add them to that as we don't need text on the page to link to this same page
            if let Some(aliases) = fm.aliases() {
                do_not_populate.extend(aliases.iter().cloned());
            }

            // if we have values then return them along with their regexes
            if !do_not_populate.is_empty() {
                build_case_insensitive_word_finder(&Some(do_not_populate))
            } else {
                // we got nothing from valid frontmatter
                None
            }
        } else {
            // there is no frontmatter
            None
        }
    }

    // Helper method to add invalid wikilinks
    pub fn add_invalid_wikilinks(&mut self, wikilinks: Vec<InvalidWikilink>) {
        self.invalid_wikilinks.extend(wikilinks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml_frontmatter::YamlFrontMatter;
    use std::error::Error;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_persist_frontmatter() -> Result<(), Box<dyn Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.md");

        // Create initial content
        let initial_content = r#"---
date_created: "2024-01-01"
---
# Test Content"#;
        fs::write(&file_path, initial_content)?;

        let mut file_info = MarkdownFileInfo::new(file_path.clone())?;
        file_info.frontmatter = Some(FrontMatter::from_markdown_str(initial_content)?);

        // Update frontmatter directly
        if let Some(fm) = &mut file_info.frontmatter {
            fm.update_date_created(Some("[[2024-01-02]]".to_string()));
            fm.persist(&file_path)?;
        }

        // Verify frontmatter was updated but content preserved
        let updated_content = fs::read_to_string(&file_path)?;
        assert!(updated_content.contains("[[2024-01-02]]"));
        assert!(updated_content.contains("# Test Content"));

        Ok(())
    }

    #[test]
    fn test_persist_frontmatter_preserves_format() -> Result<(), Box<dyn Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.md");

        let initial_content = r#"---
title: Test Doc
tags:
- tag1
- tag2
date_created: "2024-01-01"
---
# Content"#;
        fs::write(&file_path, initial_content)?;

        let mut file_info = MarkdownFileInfo::new(file_path.clone())?;
        file_info.frontmatter = Some(FrontMatter::from_markdown_str(initial_content)?);

        if let Some(fm) = &mut file_info.frontmatter {
            fm.update_date_created(Some("[[2024-01-02]]".to_string()));
            fm.persist(&file_path)?;
        }

        let updated_content = fs::read_to_string(&file_path)?;
        // Match exact YAML format serde_yaml produces
        assert!(updated_content.contains("tags:\n- tag1\n- tag2"));
        assert!(updated_content.contains("[[2024-01-02]]"));

        Ok(())
    }
}

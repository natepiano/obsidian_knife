#[cfg(test)]
mod serde_tests;

use crate::markdown_file_info::MarkdownFileInfo;
use crate::regex_utils::build_case_insensitive_word_finder;
use crate::wikilink::format_wikilink;
use crate::{constants::*, yaml_frontmatter_struct, ThreadSafeWriter};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::error::Error;

// when we set date_created_fix to None it won't serialize - cool
// the macro adds support for serializing any fields not explicitly named
yaml_frontmatter_struct! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FrontMatter {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub aliases: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub date_created: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub date_created_fix: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub date_modified: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub do_not_back_populate: Option<Vec<String>>,
        #[serde(skip)]
        pub needs_persist: bool,
        #[serde(skip)]
        pub needs_filesystem_update: Option<String>,
    }
}

impl FrontMatter {
    pub fn aliases(&self) -> Option<&Vec<String>> {
        self.aliases.as_ref()
    }

    pub fn date_created(&self) -> Option<&String> {
        self.date_created.as_ref()
    }

    pub fn date_modified(&self) -> Option<&String> {
        self.date_modified.as_ref()
    }

    pub fn date_created_fix(&self) -> Option<&String> {
        self.date_created_fix.as_ref()
    }

    pub fn update_date_created(&mut self, value: Option<String>) {
        self.date_created = value;
    }

    pub fn update_date_modified(&mut self, value: Option<String>) {
        self.date_modified = value;
    }

    pub fn update_date_created_fix(&mut self, value: Option<String>) {
        self.date_created_fix = value;
    }

    pub(crate) fn needs_persist(&self) -> bool {
        self.needs_persist
    }

    pub(crate) fn set_needs_persist(&mut self, value: bool) {
        self.needs_persist = value;
    }

    pub(crate) fn needs_filesystem_update(&self) -> Option<&String> {
        self.needs_filesystem_update.as_ref()
    }

    pub(crate) fn set_needs_filesystem_update(&mut self, value: Option<String>) {
        self.needs_filesystem_update = value;
    }

    pub fn get_do_not_back_populate_regexes(&self) -> Option<Vec<Regex>> {
        // first get do_not_back_populate explicit value
        let mut do_not_populate = self.do_not_back_populate.clone().unwrap_or_default();

        // if there are aliases, add them to that as we don't need text on the page to link to this same page
        if let Some(aliases) = self.aliases() {
            do_not_populate.extend(aliases.iter().cloned());
        }

        // if we have values then return them along with their regexes
        if !do_not_populate.is_empty() {
            build_case_insensitive_word_finder(&Some(do_not_populate))
        } else {
            // we got nothing from valid frontmatter
            None
        }
    }
}

pub fn report_frontmatter_issues(
    markdown_files: &[MarkdownFileInfo],
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let files_with_errors: Vec<_> = markdown_files
        .iter()
        .filter_map(|info| info.frontmatter_error.as_ref().map(|err| (&info.path, err)))
        .collect();

    writer.writeln(LEVEL1, "frontmatter")?;

    if files_with_errors.is_empty() {
        return Ok(());
    }

    writer.writeln(
        "",
        &format!(
            "found {} files with frontmatter parsing errors",
            files_with_errors.len()
        ),
    )?;

    for (path, err) in files_with_errors {
        writer.writeln(LEVEL3, &format!("in file {}", format_wikilink(path)))?;
        writer.writeln("", &format!("{}", err))?;
        writer.writeln("", "")?;
    }

    Ok(())
}

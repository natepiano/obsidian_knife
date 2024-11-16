#[cfg(test)]
mod date_fix_tests;
#[cfg(test)]
mod serde_tests;

use crate::markdown_file_info::MarkdownFileInfo;
use crate::wikilink::{format_wikilink, is_wikilink};
use crate::{constants::*, yaml_frontmatter_struct, ThreadSafeWriter};
use serde::{Deserialize, Serialize};
use std::error::Error;
use chrono::{Local, NaiveDate};

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
        pub(crate) needs_persist: bool,
        #[serde(skip)]
        pub(crate) needs_filesystem_update: Option<String>,
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

    // New method for processing dates after deserialization
    pub fn process_dates(&mut self) {
        self.process_date_modified();
        // Later we'll add process_date_created here too
    }

    fn process_date_modified(&mut self) {
        let today = Local::now().format("[[%Y-%m-%d]]").to_string();

        match self.date_modified() {
            Some(date_modified) => {
                if !is_wikilink(Some(date_modified)) && is_valid_date(extract_date(date_modified)) {
                    let fix = format!("[[{}]]", date_modified.trim());
                    self.update_date_modified(Some(fix));
                    self.set_needs_persist(true);
                }
            }
            None => {
                self.update_date_modified(Some(today));
                self.set_needs_persist(true);
            }
        }
    }
}

// todo - make this private
// Extracts the date string from a possible wikilink format
pub fn extract_date(date_str: &str) -> &str {
    let date_str = date_str.trim();
    if is_wikilink(Some(date_str)) {
        date_str
            .trim_start_matches(OPENING_WIKILINK)
            .trim_end_matches(CLOSING_WIKILINK)
            .trim()
    } else {
        date_str
    }
}

// todo: make this private again
// Validates if a string is a valid YYYY-MM-DD date
pub fn is_valid_date(date_str: &str) -> bool {
    NaiveDate::parse_from_str(date_str.trim(), "%Y-%m-%d").is_ok()
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

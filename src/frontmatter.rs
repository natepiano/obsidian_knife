use std::collections::HashMap;

use chrono::DateTime;
use chrono::Utc;
use chrono_tz::Tz;
use chrono_tz::UTC;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;
use serde_yaml::Value;

use crate::constants::CLOSING_WIKILINK;
use crate::constants::FORMAT_DATE;
use crate::constants::OPENING_WIKILINK;
use crate::support;
use crate::yaml_frontmatter_struct;

// `created_fix` serializes only when `Option::is_some` returns true.
// `yaml_frontmatter_struct!` preserves YAML keys without explicit `FrontMatter` fields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum PersistState {
    #[default]
    Clean,
    Modified,
}

yaml_frontmatter_struct! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub(crate) struct FrontMatter {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(crate) aliases: Option<Vec<String>>,
        #[serde(rename = "date_created")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(crate) created: Option<String>,
        #[serde(rename = "date_created_fix")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(crate) created_fix: Option<String>,
        #[serde(rename = "date_modified")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(crate) modified: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(crate) do_not_back_populate: Option<Vec<String>>,
        #[serde(skip)]
        pub(crate) persist_state: PersistState,
        #[serde(skip)]
        pub(crate) raw_created: Option<DateTime<Utc>>,
        #[serde(skip)]
        pub(crate) raw_modified: Option<DateTime<Utc>>,
    }
}

impl FrontMatter {
    pub(crate) fn aliases(&self) -> Option<&[String]> { self.aliases.as_deref() }

    pub(crate) fn date_created(&self) -> Option<&str> { self.created.as_deref() }

    pub(crate) fn date_modified(&self) -> Option<&str> { self.modified.as_deref() }

    pub(crate) fn date_created_fix(&self) -> Option<&str> { self.created_fix.as_deref() }

    pub(crate) fn remove_date_created_fix(&mut self) {
        // `created_fix = None` skips `created_fix` during serialization.
        self.created_fix = None;
    }

    // `raw_created` and `raw_modified` provide filesystem timestamps.
    // `set_date_created` may only change filesystem creation time, so
    // `set_date_modified_now` records a fallback `raw_modified` value.
    pub(crate) fn set_date_created(&mut self, date: DateTime<Utc>, operational_timezone: &str) {
        let timezone: Tz = operational_timezone.parse().unwrap_or(UTC);
        let local_date = date.with_timezone(&timezone);
        self.raw_created = Some(date);
        let formatted_date = local_date.format(FORMAT_DATE);
        self.created = Some(format!(
            "{OPENING_WIKILINK}{formatted_date}{CLOSING_WIKILINK}"
        ));

        if self.raw_modified.is_none() {
            self.set_date_modified_now(operational_timezone);
        }

        self.persist_state = PersistState::Modified;
    }

    // We invoke `set_date_modified` on any changes to `MarkdownFile` so we persist an updated
    // `date_modified` that matches the file, and use `date_modified` as the sentinel for
    // persisting at the end of processing.
    pub(crate) fn set_date_modified_now(&mut self, operational_timezone: &str) {
        self.set_date_modified(Utc::now(), operational_timezone);
    }

    // `set_date_modified` fills missing `date_modified` values.
    pub(crate) fn set_date_modified(&mut self, date: DateTime<Utc>, operational_timezone: &str) {
        let timezone: Tz = operational_timezone.parse().unwrap_or(UTC);
        let local_date = date.with_timezone(&timezone);
        self.raw_modified = Some(date);
        let formatted_date = local_date.format(FORMAT_DATE);
        self.modified = Some(format!(
            "{OPENING_WIKILINK}{formatted_date}{CLOSING_WIKILINK}"
        ));
        self.persist_state = PersistState::Modified;
    }

    pub(crate) fn needs_persist(&self) -> bool { self.persist_state == PersistState::Modified }

    pub(crate) fn get_do_not_back_populate_regexes(&self) -> Option<Vec<Regex>> {
        // `do_not_back_populate` starts with the explicit frontmatter value.
        let mut do_not_populate = self.do_not_back_populate.clone().unwrap_or_default();

        // `aliases` are equivalent no-populate targets for the same page.
        if let Some(aliases) = self.aliases() {
            do_not_populate.extend(aliases.iter().cloned());
        }

        // `do_not_populate` values become case-insensitive regexes.
        if do_not_populate.is_empty() {
            // Empty frontmatter values produce no regexes.
            None
        } else {
            Some(support::build_case_insensitive_word_finder(
                &do_not_populate,
            ))
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use super::FrontMatter;

    fn regex_matches(front_matter: &FrontMatter, expected_count: usize, test_line: &str) {
        let regexes = front_matter.get_do_not_back_populate_regexes().unwrap();
        assert_eq!(regexes.len(), expected_count);
        for regex in regexes {
            assert!(regex.is_match(test_line));
        }
    }

    #[test]
    fn test_markdown_file_aliases_only() {
        let front_matter = FrontMatter {
            aliases: Some(vec!["Only Alias".to_string()]),
            ..FrontMatter::default()
        };

        regex_matches(&front_matter, 1, "Only Alias appears here");
    }

    #[test]
    fn test_scan_markdown_file_with_do_not_back_populate() {
        let front_matter = FrontMatter {
            do_not_back_populate: Some(vec![
                "test phrase".to_string(),
                "another phrase".to_string(),
            ]),
            ..FrontMatter::default()
        };

        regex_matches(&front_matter, 2, "here is a test phrase and another phrase");
    }

    #[test]
    fn test_scan_markdown_file_combines_aliases_with_do_not_back_populate() {
        let front_matter = FrontMatter {
            aliases: Some(vec!["First Alias".to_string(), "Second Alias".to_string()]),
            do_not_back_populate: Some(vec!["exclude this".to_string()]),
            ..FrontMatter::default()
        };

        regex_matches(
            &front_matter,
            3,
            "First Alias and Second Alias and exclude this",
        );
    }
}

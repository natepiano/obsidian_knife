use std::collections::HashMap;

use chrono::DateTime;
use chrono::Utc;
use chrono_tz::Tz;
use chrono_tz::UTC;
use regex::Regex;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde_yaml::Value;

use crate::constants::CLOSING_WIKILINK;
use crate::constants::FORMAT_DATE;
use crate::constants::OPENING_WIKILINK;
use crate::support;
use crate::yaml_frontmatter_struct;

/// Obsidian list properties such as `aliases` and `do_not_back_populate` hold either a
/// single scalar (`do_not_back_populate: style`) or a sequence, so both forms deserialize
/// into the `Vec<String>` that `FrontMatter` stores.
#[derive(Deserialize)]
#[serde(untagged)]
enum ListProperty {
    Scalar(String),
    Sequence(Vec<String>),
}

impl From<ListProperty> for Vec<String> {
    fn from(list_property: ListProperty) -> Self {
        match list_property {
            ListProperty::Scalar(value) => vec![value],
            ListProperty::Sequence(values) => values,
        }
    }
}

yaml_frontmatter_struct! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub(crate) struct FrontMatter {
        #[serde(default, deserialize_with = "deserialize_list_property")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(crate) aliases: Option<Vec<String>>,
        #[serde(rename = "date_created")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(crate) created: Option<String>,
        #[serde(rename = "date_modified")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(crate) modified: Option<String>,
        #[serde(default, deserialize_with = "deserialize_list_property")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(crate) do_not_back_populate: Option<Vec<String>>,
    }
}

impl FrontMatter {
    pub(crate) fn aliases(&self) -> Option<&[String]> { self.aliases.as_deref() }

    pub(crate) fn date_created(&self) -> Option<&str> { self.created.as_deref() }

    #[cfg(test)]
    pub(crate) fn date_modified(&self) -> Option<&str> { self.modified.as_deref() }

    // The tool stamps `date_modified` on every file it changes: the vault's linter only runs
    // when a note is saved in Obsidian, so it would not see a change made here.
    pub(crate) fn set_date_modified_now(&mut self, operational_timezone: &str) {
        self.set_date_modified(Utc::now(), operational_timezone);
    }

    // `set_date_modified` fills missing `date_modified` values.
    pub(crate) fn set_date_modified(&mut self, date: DateTime<Utc>, operational_timezone: &str) {
        let timezone: Tz = operational_timezone.parse().unwrap_or(UTC);
        let local_date = date.with_timezone(&timezone);
        let formatted_date = local_date.format(FORMAT_DATE);
        self.modified = Some(format!(
            "{OPENING_WIKILINK}{formatted_date}{CLOSING_WIKILINK}"
        ));
    }

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

fn deserialize_list_property<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<ListProperty>::deserialize(deserializer)
        .map(|list_property| list_property.map(Vec::from))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use super::FrontMatter;
    use crate::yaml_frontmatter::YamlFrontMatter;

    fn regex_matches(front_matter: &FrontMatter, expected_count: usize, test_line: &str) {
        let regexes = front_matter.get_do_not_back_populate_regexes().unwrap();
        assert_eq!(regexes.len(), expected_count);
        for regex in regexes {
            assert!(regex.is_match(test_line));
        }
    }

    #[test]
    fn test_list_properties_accept_scalar_or_sequence() {
        let front_matter =
            FrontMatter::from_yaml_str("aliases: Only Alias\ndo_not_back_populate: style").unwrap();
        assert_eq!(front_matter.aliases, Some(vec!["Only Alias".to_string()]));
        assert_eq!(
            front_matter.do_not_back_populate,
            Some(vec!["style".to_string()])
        );

        let front_matter = FrontMatter::from_yaml_str(
            "aliases: [First, Second]\ndo_not_back_populate:\n  - one\n  - two",
        )
        .unwrap();
        assert_eq!(
            front_matter.aliases,
            Some(vec!["First".to_string(), "Second".to_string()])
        );
        assert_eq!(
            front_matter.do_not_back_populate,
            Some(vec!["one".to_string(), "two".to_string()])
        );

        let front_matter = FrontMatter::from_yaml_str("tags: [note]").unwrap();
        assert_eq!(front_matter.aliases, None);
        assert_eq!(front_matter.do_not_back_populate, None);
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

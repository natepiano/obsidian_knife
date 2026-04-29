use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use chrono::DateTime;
use chrono::Utc;
use tempfile::TempDir;

use crate::constants::FORMAT_DATE;
use crate::constants::YAML_CLOSING_DELIMITER;
use crate::constants::YAML_OPENING_DELIMITER;
use crate::utils;

#[derive(Clone)]
pub enum Content {
    Text(String),
    Binary(Vec<u8>),
}

impl From<String> for Content {
    fn from(s: String) -> Self { Self::Text(s) }
}

impl From<&str> for Content {
    fn from(s: &str) -> Self { Self::Text(s.to_string()) }
}

impl From<Vec<u8>> for Content {
    fn from(v: Vec<u8>) -> Self { Self::Binary(v) }
}

#[derive(Default)]
struct FrontmatterDates {
    created:     Option<String>,
    created_fix: Option<String>,
    modified:    Option<String>,
}

struct FileSystemDates {
    created:  DateTime<Utc>,
    modified: DateTime<Utc>,
}

pub struct TestFileBuilder {
    aliases:            Option<Vec<String>>,
    content:            Content,
    custom_frontmatter: Option<String>,
    file_system_dates:  FileSystemDates,
    frontmatter_dates:  FrontmatterDates,
    tags:               Option<Vec<String>>,
    title:              Option<String>,
}

impl Default for TestFileBuilder {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            aliases:            None,
            content:            Content::Text("Test content".to_string()),
            custom_frontmatter: None,
            file_system_dates:  FileSystemDates {
                created:  now,
                modified: now,
            },
            frontmatter_dates:  FrontmatterDates::default(),
            tags:               None,
            title:              None,
        }
    }
}

impl TestFileBuilder {
    pub fn new() -> Self { Self::default() }

    pub fn with_custom_frontmatter(mut self, content: String) -> Self {
        self.custom_frontmatter = Some(content);
        self
    }

    pub fn with_frontmatter_dates(
        mut self,
        created: Option<String>,
        modified: Option<String>,
    ) -> Self {
        self.frontmatter_dates.created = created;
        self.frontmatter_dates.modified = modified;
        self
    }

    pub fn with_date_created_fix(mut self, date_created_fix: Option<String>) -> Self {
        self.frontmatter_dates.created_fix = date_created_fix;
        self
    }

    pub fn with_fs_dates(mut self, created: DateTime<Utc>, modified: DateTime<Utc>) -> Self {
        self.file_system_dates.created = created;
        self.file_system_dates.modified = modified;
        self
    }

    pub fn with_matching_dates(mut self, datetime: DateTime<Utc>) -> Self {
        self.frontmatter_dates.created = Some(format!("[[{}]]", datetime.format(FORMAT_DATE)));
        self.frontmatter_dates.modified = Some(format!("[[{}]]", datetime.format(FORMAT_DATE)));
        self.file_system_dates.created = datetime;
        self.file_system_dates.modified = datetime;
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }

    pub fn with_aliases(mut self, aliases: Vec<String>) -> Self {
        self.aliases = Some(aliases);
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_content<T: Into<Content>>(mut self, content: T) -> Self {
        self.content = content.into();
        self
    }

    pub fn create(self, temp_dir: &TempDir, filename: &str) -> PathBuf {
        let Self {
            aliases,
            content,
            custom_frontmatter,
            file_system_dates,
            frontmatter_dates,
            tags,
            title,
        } = self;
        let file_path = temp_dir.path().join(filename);
        let mut file = File::create(&file_path).unwrap();

        let has_frontmatter = frontmatter_dates.created.is_some()
            || frontmatter_dates.modified.is_some()
            || frontmatter_dates.created_fix.is_some()
            || tags.is_some()
            || aliases.is_some()
            || title.is_some()
            || custom_frontmatter.is_some();

        if has_frontmatter {
            writeln!(file, "{}", YAML_OPENING_DELIMITER.trim_end()).unwrap();
            if let Some(created) = frontmatter_dates.created {
                writeln!(file, "date_created: \"{created}\"").unwrap();
            }
            if let Some(modified) = frontmatter_dates.modified {
                writeln!(file, "date_modified: \"{modified}\"").unwrap();
            }
            if let Some(date_created_fix) = frontmatter_dates.created_fix {
                writeln!(file, "date_created_fix: \"{date_created_fix}\"").unwrap();
            }
            if let Some(tags) = tags {
                writeln!(file, "tags:").unwrap();
                for tag in tags {
                    writeln!(file, "- {tag}").unwrap();
                }
            }
            if let Some(aliases) = aliases {
                writeln!(file, "aliases:").unwrap();
                for alias in aliases {
                    writeln!(file, "- {alias}").unwrap();
                }
            }
            if let Some(title) = title {
                writeln!(file, "title: {title}").unwrap();
            }
            if let Some(custom) = custom_frontmatter {
                writeln!(file, "{custom}").unwrap();
            }
            writeln!(file, "{}", YAML_CLOSING_DELIMITER.trim_end()).unwrap();
        }

        match content {
            Content::Text(text) => writeln!(file, "{text}").unwrap(),
            Content::Binary(bytes) => file.write_all(&bytes).unwrap(),
        }

        utils::set_file_dates(
            &file_path,
            Some(file_system_dates.created),
            file_system_dates.modified,
        )
        .unwrap();

        file_path
    }
}

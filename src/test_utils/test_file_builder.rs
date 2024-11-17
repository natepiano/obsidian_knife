use chrono::{DateTime, Local};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::SystemTime;
use tempfile::TempDir;

pub struct TestFileBuilder {
    frontmatter_created: Option<String>,
    frontmatter_modified: Option<String>,
    date_created_fix: Option<String>,
    fs_created: DateTime<Local>,
    fs_modified: DateTime<Local>,
    tags: Option<Vec<String>>,
    aliases: Option<Vec<String>>,
    title: Option<String>, // Changed to Option
    content: String,
}

impl TestFileBuilder {
    pub fn new() -> Self {
        let now = Local::now();
        Self {
            frontmatter_created: None,
            frontmatter_modified: None,
            date_created_fix: None,
            fs_created: now,
            fs_modified: now,
            tags: None,
            aliases: None, // Initialize new field
            title: None,
            content: String::from("Test content"), // Initialize new field
        }
    }

    pub fn with_frontmatter_dates(
        mut self,
        created: Option<String>,
        modified: Option<String>,
    ) -> Self {
        self.frontmatter_created = created;
        self.frontmatter_modified = modified;
        self
    }

    pub fn with_date_created_fix(mut self, date_created_fix: Option<String>) -> Self {
        self.date_created_fix = date_created_fix;
        self
    }

    pub fn with_fs_dates(mut self, created: DateTime<Local>, modified: DateTime<Local>) -> Self {
        self.fs_created = created;
        self.fs_modified = modified;
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

    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    pub fn with_content(mut self, content: String) -> Self {
        self.content = content;
        self
    }

    pub fn create(self, temp_dir: &TempDir, filename: &str) -> PathBuf {
        let file_path = temp_dir.path().join(filename);
        let mut file = File::create(&file_path).unwrap();

        let has_frontmatter = self.frontmatter_created.is_some()
            || self.frontmatter_modified.is_some()
            || self.date_created_fix.is_some()
            || self.tags.is_some()
            || self.aliases.is_some()
            || self.title.is_some();

        if has_frontmatter {
            writeln!(file, "---").unwrap();
            if let Some(created) = self.frontmatter_created {
                writeln!(file, "date_created: \"{}\"", created).unwrap();
            }
            if let Some(modified) = self.frontmatter_modified {
                writeln!(file, "date_modified: \"{}\"", modified).unwrap();
            }
            if let Some(date_created_fix) = self.date_created_fix {
                writeln!(file, "date_created_fix: \"{}\"", date_created_fix).unwrap();
            }
            if let Some(tags) = self.tags {
                writeln!(file, "tags:").unwrap();
                for tag in tags {
                    writeln!(file, "- {}", tag).unwrap();
                }
            }
            if let Some(aliases) = self.aliases {
                writeln!(file, "aliases:").unwrap();
                for alias in aliases {
                    writeln!(file, "- {}", alias).unwrap();
                }
            }
            if let Some(title) = self.title {
                writeln!(file, "title: {}", title).unwrap();
            }
            writeln!(file, "---").unwrap();
        }

        writeln!(file, "{}", self.content).unwrap();

        let created_system = SystemTime::UNIX_EPOCH
            + std::time::Duration::from_secs(self.fs_created.timestamp() as u64);
        let modified_system = SystemTime::UNIX_EPOCH
            + std::time::Duration::from_secs(self.fs_modified.timestamp() as u64);

        let created_time = filetime::FileTime::from_system_time(created_system);
        let modified_time = filetime::FileTime::from_system_time(modified_system);
        filetime::set_file_times(&file_path, created_time, modified_time).unwrap();

        file_path
    }
}

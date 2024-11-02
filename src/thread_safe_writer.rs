use crate::constants::{pluralize, Phrase};
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct ThreadSafeWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
    file: Arc<Mutex<std::fs::File>>,
}

#[derive(Clone, Copy)]
pub enum ColumnAlignment {
    Left,
    Center,
    Right,
}

impl ThreadSafeWriter {
    pub fn new(obsidian_path: &Path) -> io::Result<Self> {
        let file_path = obsidian_path.join("obsidian knife output.md");

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)?;

        Ok(ThreadSafeWriter {
            buffer: Arc::new(Mutex::new(Vec::new())),
            file: Arc::new(Mutex::new(file)),
        })
    }

    // pub fn write_markdown_table(
    //     &self,
    //     headers: &[&str],
    //     rows: &[Vec<String>],
    //     alignments: Option<&[ColumnAlignment]>,
    // ) -> io::Result<()> {
    //     // Write to file (Markdown format)
    //     self.write_markdown_table_to_file(headers, rows, alignments)?;
    //
    //     Ok(())
    // }

    pub(crate) fn write_markdown_table(
        &self,
        headers: &[&str],
        rows: &[Vec<String>],
        alignments: Option<&[ColumnAlignment]>,
    ) -> io::Result<()> {
        let mut file = self.file.lock().unwrap();

        // Write headers
        writeln!(file, "| {} |", headers.join(" | "))?;

        // Write separator with alignment
        let separator = match alignments {
            Some(aligns) => {
                let sep: Vec<String> = aligns
                    .iter()
                    .map(|&a| match a {
                        ColumnAlignment::Left => ":---".to_string(),
                        ColumnAlignment::Center => ":---:".to_string(),
                        ColumnAlignment::Right => "---:".to_string(),
                    })
                    .collect();
                format!("| {} |", sep.join(" | "))
            }
            None => format!(
                "| {} |",
                headers
                    .iter()
                    .map(|_| "---")
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
        };
        writeln!(file, "{}", separator)?;

        // Write data rows
        for row in rows {
            writeln!(file, "| {} |", row.join(" | "))?;
        }

        file.flush()?;
        Ok(())
    }

    pub fn write_properties(&self, properties: &str) -> io::Result<()> {
        // Write to file (Markdown format with prefix and suffix)
        let mut file = self.file.lock().unwrap();
        writeln!(file, "---")?;
        writeln!(file, "{}", properties)?;
        writeln!(file, "---")?;
        file.flush()?;

        // Write to buffer (without prefix and suffix)
        let mut buffer = self.buffer.lock().unwrap();
        writeln!(buffer, "{}", properties)?;

        Ok(())
    }

    pub fn writeln(&self, markdown_prefix: &str, message: &str) -> io::Result<()> {

        // Create the prefix string first
        let prefix = if markdown_prefix.is_empty() {
            String::new()
        } else if markdown_prefix.ends_with(' ') {
            markdown_prefix.to_string()
        } else {
            format!("{} ", markdown_prefix)
        };

        // Then use it to create the full message
        let file_message = format!("{}{}\n", prefix, message);
        let mut file = self.file.lock().unwrap();
        file.write_all(file_message.as_bytes())?;
        file.flush()?;

        Ok(())
    }

    /// Writes a count-based phrase with newlines and proper pluralization
    pub fn writeln_pluralized(&self, count: usize, phrase: Phrase) -> io::Result<()> {
        let message = pluralize(count, phrase);
        self.writeln("", &format!("{} {}\n", count, message))
    }

    /// Same as write_count but with a markdown prefix
    pub fn write_count_with_prefix(
        &self,
        prefix: &str,
        count: usize,
        phrase: Phrase,
    ) -> io::Result<()> {
        let message = pluralize(count, phrase);
        self.writeln(prefix, &format!("{}\n", message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LEVEL2;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_write_count() -> io::Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let writer = ThreadSafeWriter::new(temp_dir.path())?;

        writer.writeln_pluralized(1, Phrase::InvalidDates)?;
        writer.write_count_with_prefix(LEVEL2, 2, Phrase::InvalidDates)?;

        let content = fs::read_to_string(temp_dir.path().join("obsidian knife output.md"))?;
        assert!(content.contains("file has an invalid date"));
        assert!(content.contains("## files have invalid dates"));
        Ok(())
    }
}

use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

use crate::constants::OUTPUT_MARKDOWN_FILE;

pub struct OutputFileWriter {
    file: Mutex<File>,
}

#[derive(Clone, Copy)]
pub enum ColumnAlignment {
    Left,
    Center,
    Right,
}

#[allow(
    clippy::unwrap_used,
    reason = "mutex poisoning is unrecoverable — unwrap is the standard pattern"
)]
impl OutputFileWriter {
    pub fn new(obsidian_path: &Path) -> io::Result<Self> {
        let file_path = obsidian_path.join(OUTPUT_MARKDOWN_FILE);

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)?;

        Ok(Self {
            file: Mutex::new(file),
        })
    }

    pub fn write_markdown_table(
        &self,
        headers: &[&str],
        rows: &[Vec<String>],
        alignments: Option<&[ColumnAlignment]>,
    ) -> io::Result<()> {
        let separator = alignments.map_or_else(
            || {
                format!(
                    "| {} |",
                    headers
                        .iter()
                        .map(|_| "---")
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
            },
            |aligns| {
                let sep: Vec<String> = aligns
                    .iter()
                    .map(|&a| match a {
                        ColumnAlignment::Left => ":---".to_string(),
                        ColumnAlignment::Center => ":---:".to_string(),
                        ColumnAlignment::Right => "---:".to_string(),
                    })
                    .collect();
                format!("| {} |", sep.join(" | "))
            },
        );

        let mut file = self.file.lock().unwrap();

        // markdown tables always have to have a blank line before them
        writeln!(file, "\n| {} |", headers.join(" | "))?;
        writeln!(file, "{separator}")?;

        for row in rows {
            writeln!(file, "| {} |", row.join(" | "))?;
        }

        // there has to be a blank line after a table or it won't render
        writeln!(file, "\n---")?;

        file.flush()?;
        drop(file);
        Ok(())
    }

    pub fn write_properties(&self, properties: &str) -> io::Result<()> {
        let mut file = self.file.lock().unwrap();
        writeln!(file, "---")?;
        writeln!(file, "{properties}")?;
        writeln!(file, "---")?;
        file.flush()
    }

    pub fn writeln(&self, markdown_prefix: &str, message: &str) -> io::Result<()> {
        let prefix = if markdown_prefix.is_empty() {
            String::new()
        } else if markdown_prefix.ends_with(' ') {
            markdown_prefix.to_string()
        } else {
            format!("{markdown_prefix} ")
        };

        let file_message = format!("{prefix}{message}\n");
        let mut file = self.file.lock().unwrap();
        file.write_all(file_message.as_bytes())?;
        file.flush()
    }
}

use crate::constants::{pluralize, PhraseOld};
use crate::OUTPUT_MARKDOWN_FILE;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

pub struct OutputFileWriter {
    file: Mutex<File>,
}

#[derive(Clone, Copy)]
pub enum ColumnAlignment {
    Left,
    Center,
    Right,
}

impl OutputFileWriter {
    pub fn new(obsidian_path: &Path) -> io::Result<Self> {
        let file_path = obsidian_path.join(OUTPUT_MARKDOWN_FILE);

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)?;

        Ok(OutputFileWriter {
            file: Mutex::new(file),
        })
    }

    pub fn write_markdown_table(
        &self,
        headers: &[&str],
        rows: &[Vec<String>],
        alignments: Option<&[ColumnAlignment]>,
    ) -> io::Result<()> {
        let mut file = self.file.lock().unwrap();

        writeln!(file, "| {} |", headers.join(" | "))?;

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

        for row in rows {
            writeln!(file, "| {} |", row.join(" | "))?;
        }

        // there has to be a blank line after a table or it won't render
        writeln!(file, "\n---")?;

        file.flush()?;
        Ok(())
    }

    pub fn write_properties(&self, properties: &str) -> io::Result<()> {
        let mut file = self.file.lock().unwrap();
        writeln!(file, "---")?;
        writeln!(file, "{}", properties)?;
        writeln!(file, "---")?;
        file.flush()
    }

    pub fn writeln(&self, markdown_prefix: &str, message: &str) -> io::Result<()> {
        let prefix = if markdown_prefix.is_empty() {
            String::new()
        } else if markdown_prefix.ends_with(' ') {
            markdown_prefix.to_string()
        } else {
            format!("{} ", markdown_prefix)
        };

        let file_message = format!("{}{}\n", prefix, message);
        let mut file = self.file.lock().unwrap();
        file.write_all(file_message.as_bytes())?;
        file.flush()
    }

    pub fn writeln_pluralized(&self, count: usize, phrase: PhraseOld) -> io::Result<()> {
        let message = pluralize(count, phrase);
        self.writeln("", &format!("{} {}\n", count, message))
    }
}

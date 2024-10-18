use chrono::Local;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct ThreadSafeWriter {
    console: io::Stdout,
    buffer: Arc<Mutex<Vec<u8>>>,
    file: Arc<Mutex<std::fs::File>>,
}

#[derive(Clone, Copy)]
pub enum ColumnAlignment {
    Left,
    // Center,
    Right,
}

impl ThreadSafeWriter {
    pub fn new(obsidian_path: &Path) -> io::Result<Self> {
        let file_path = obsidian_path.join("obsidian_knife_output.md");

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)?;

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        writeln!(file, "---\ntime_stamp: \"{}\"\n---", timestamp)?;

        Ok(ThreadSafeWriter {
            console: io::stdout(),
            buffer: Arc::new(Mutex::new(Vec::new())),
            file: Arc::new(Mutex::new(file)),
        })
    }

    pub fn write_markdown_table(
        &self,
        headers: &[&str],
        rows: &[Vec<String>],
        alignments: Option<&[ColumnAlignment]>,
    ) -> io::Result<()> {
        // Write to file (Markdown format)
        self.write_markdown_table_to_file(headers, rows, alignments)?;

        // Write to console (simplified format)
        self.write_table_to_console(headers, rows)?;

        Ok(())
    }

    fn write_markdown_table_to_file(
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
                        // ColumnAlignment::Center => ":---:".to_string(),
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

    fn write_table_to_console(&self, headers: &[&str], rows: &[Vec<String>]) -> io::Result<()> {
        let mut console = self.console.lock();

        // Write headers
        writeln!(console, "{}", headers.join(" | "))?;

        // Write data rows
        for row in rows {
            writeln!(console, "{}: {}", row[0], row[1])?;
        }

        console.flush()?;
        Ok(())
    }

    pub fn writeln(&self, markdown_prefix: &str, message: &str) -> io::Result<()> {
        if !message.is_empty() {
            let console_message = format!("{}\n", message);

            // Write to console (without markdown prefix)
            self.console.lock().write_all(console_message.as_bytes())?;
            self.console.lock().flush()?;

            // Write to buffer (without markdown prefix)
            let mut buffer = self.buffer.lock().unwrap();
            buffer.extend_from_slice(console_message.as_bytes());
        }

        // Ensure there's a space between the prefix and the message
        let markdown_prefix = if markdown_prefix.is_empty() {
            String::new()
        } else if markdown_prefix.ends_with(' ') {
            markdown_prefix.to_string()
        } else {
            format!("{} ", markdown_prefix)
        };

        // Always write to file, even if message is empty
        let file_message = format!("{}{}\n", markdown_prefix, message);
        let mut file = self.file.lock().unwrap();
        file.write_all(file_message.as_bytes())?;
        file.flush()?;

        Ok(())
    }
}

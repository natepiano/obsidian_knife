use std::sync::{Arc, Mutex};
use std::io::{self, Write};
use chrono::Local;
use std::fs::OpenOptions;
use std::path::{Path};

pub struct ThreadSafeWriter {
    console: io::Stdout,
    buffer: Arc<Mutex<Vec<u8>>>,
    file: Arc<Mutex<std::fs::File>>,
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


    pub fn writeln_markdown(&self, markdown_prefix: &str, message: &str) -> io::Result<()> {
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
        let markdown_prefix = if markdown_prefix.ends_with(' ') {
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

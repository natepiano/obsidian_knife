use std::sync::{Arc, Mutex};
use std::io::{self, Write};
use chrono::Local;
use std::fs::OpenOptions;
use std::path::Path;

pub struct ThreadSafeOutput {
    console: io::Stdout,
    buffer: Arc<Mutex<Vec<u8>>>,
    file: Arc<Mutex<std::fs::File>>,
}

impl ThreadSafeOutput {
    pub fn new(obsidian_path: &Path) -> io::Result<Self> {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let filename = format!("{} obsidian knife.md", timestamp);
        let file_path = obsidian_path.join(filename);

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)?;

        Ok(ThreadSafeOutput {
            console: io::stdout(),
            buffer: Arc::new(Mutex::new(Vec::new())),
            file: Arc::new(Mutex::new(file)),
        })
    }

    pub fn write(&self, message: &str) -> io::Result<()> {
        // Write to console
        self.console.lock().write_all(message.as_bytes())?;
        self.console.lock().flush()?;

        // Write to buffer
        let mut buffer = self.buffer.lock().unwrap();
        buffer.extend_from_slice(message.as_bytes());

        // Write to file
        let mut file = self.file.lock().unwrap();
        file.write_all(message.as_bytes())?;
        file.flush()?;

        Ok(())
    }
}

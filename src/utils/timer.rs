// Add to a new utils.rs file:
use crate::MILLISECONDS;
use std::time::Instant;

pub struct Timer {
    start: Instant,
    label: String,
}

impl Timer {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            start: Instant::now(),
            label: label.into(),
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        if !cfg!(test) {
            println!("{}: {:.2}{MILLISECONDS}", self.label, duration.as_millis());
        }
    }
}

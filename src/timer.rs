// Add to a new utils.rs file:
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
            println!("{} took: {:.2}ms", self.label, duration.as_millis());
        }
    }
}

use std::time::Instant;

use crate::constants::MILLISECONDS;

pub(crate) struct Timer {
    start: Instant,
    label: String,
}

impl Timer {
    pub(crate) fn new(label: impl Into<String>) -> Self {
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
            let label = &self.label;
            let elapsed = duration.as_millis();
            println!("{label}: {elapsed:.2}{MILLISECONDS}");
        }
    }
}

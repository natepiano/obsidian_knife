use crate::utils::{ColumnAlignment, ReportWriter};
use std::error::Error;

/// Core trait for building report tables
pub trait TableBuilder {
    /// The type of data being displayed in the table
    type Item;

    /// Get the table headers
    fn headers(&self) -> Vec<&str>;

    /// Get column alignments for the table
    fn alignments(&self) -> Vec<ColumnAlignment>;

    /// Transform data items into table rows
    fn build_rows(&self, items: &[Self::Item]) -> Vec<Vec<String>>;

    /// Optional table title
    fn title(&self) -> Option<&str> {
        None
    }

    /// Optional table description/summary
    fn description(&self, items: &[Self::Item]) -> Option<String> {
        None
    }
}

/// Represents a table section in a report
pub struct ReportTable<T> {
    items: Vec<T>,
    level: &'static str,
}

impl<T> ReportTable<T> {
    pub fn new(items: Vec<T>, level: &'static str) -> Self {
        Self { items, level }
    }

    /// Write the table using the provided builder and writer
    pub fn write<B: TableBuilder<Item = T>>(
        &self,
        builder: &B,
        writer: &ReportWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Write title if present
        if let Some(title) = builder.title() {
            writer.writeln(self.level, title)?;
        }

        // Write description if present
        if let Some(desc) = builder.description(&self.items) {
            writer.writeln("", &desc)?;
        }

        // Skip empty tables unless overridden
        if self.items.is_empty() {
            return Ok(());
        }

        // Build and write the table
        let headers = builder.headers();
        let alignments = builder.alignments();
        let rows = builder.build_rows(&self.items);

        writer.write_markdown_table(&headers, &rows, Some(&alignments))?;

        Ok(())
    }
}

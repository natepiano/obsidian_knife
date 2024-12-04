use crate::utils::{ColumnAlignment, OutputFileWriter};
use std::error::Error;

/// Core trait for building report tables
pub trait TableDefinition {
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
    fn description(&self, _: &[Self::Item]) -> Option<String> {
        None
    }

    /// markdown level
    fn level(&self) -> &'static str;

    fn hide_title_if_no_rows(&self) -> bool {
        true
    }
}

/// Represents a table section in a report
pub struct ReportWriter<T> {
    items: Vec<T>,
}

impl<T> ReportWriter<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self { items }
    }
    /// Write the table using the provided builder and writer
    pub fn write<B: TableDefinition<Item = T>>(
        &self,
        table: &B,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {

        if self.items.is_empty() && table.hide_title_if_no_rows() {
            return Ok(());
        }

        // Write title if present
        if let Some(title) = table.title() {
            writer.writeln(table.level(), title)?;
        }

        // Write description if present
        if let Some(desc) = table.description(&self.items) {
            writer.writeln("", &desc)?;
        }

        // Skip empty tables unless overridden
        if self.items.is_empty() {
            return Ok(());
        }

        // Build and write the table
        let headers = table.headers();
        let alignments = table.alignments();
        let rows = table.build_rows(&self.items);

        writer.write_markdown_table(&headers, &rows, Some(&alignments))?;

        Ok(())
    }
}

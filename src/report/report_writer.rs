use crate::utils::{ColumnAlignment, OutputFileWriter};
use std::error::Error;

/// definition of the elements of a report to write out as a markdown table
pub trait ReportDefinition {
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
/// writes out the TableDefinition
pub struct ReportWriter<T> {
    items: Vec<T>,
}

impl<T> ReportWriter<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self { items }
    }
    /// Write the table using the provided builder and writer
    pub fn write<B: ReportDefinition<Item = T>>(
        &self,
        report: &B,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {

        if self.items.is_empty() && report.hide_title_if_no_rows() {
            return Ok(());
        }

        // Write title if present
        if let Some(title) = report.title() {
            writer.writeln(report.level(), title)?;
        }

        // Write description if present
        if let Some(desc) = report.description(&self.items) {
            writer.writeln("", &desc)?;
        }

        // Skip empty tables unless overridden
        if self.items.is_empty() {
            return Ok(());
        }

        // Build and write the table
        let headers = report.headers();
        let alignments = report.alignments();
        let rows = report.build_rows(&self.items);

        writer.write_markdown_table(&headers, &rows, Some(&alignments))?;

        Ok(())
    }
}

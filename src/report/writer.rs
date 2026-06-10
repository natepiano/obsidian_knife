use std::error::Error;

use anyhow::Result as AnyhowResult;

use crate::output_file_writer::ColumnAlignment;
use crate::output_file_writer::OutputFileWriter;
use crate::validated_config::ValidatedConfig;

/// definition of the elements of a report to write out as a markdown table
pub(super) trait ReportDefinition<C = ()> {
    /// The type of data being displayed in the table
    type Item;
    /// Get the table headers
    fn headers(&self) -> Vec<&str>;

    /// Get column alignments for the table
    fn alignments(&self) -> Vec<ColumnAlignment>;

    /// Transform data items into table rows
    ///
    /// simple reports can use `_: &()` for this generic parameter so they don't
    /// need to use it and the compiler won't complain
    ///
    /// reports that need config information can use `report_context: &ReportContext`
    /// to access properties such as `change_mode` or `obsidian_path`
    ///
    /// it's slightly hacky but prevents having to dramatically alter the structure and it's
    /// readable enough
    fn build_rows(
        &self,
        items: &[Self::Item],
        validated_config: Option<&ValidatedConfig>,
    ) -> AnyhowResult<Vec<Vec<String>>>;

    /// Optional table title
    fn title(&self) -> Option<String> { None }

    /// Optional table description/summary
    fn description(&self, items: &[Self::Item]) -> String;

    /// markdown level
    fn level(&self) -> &'static str;
}

/// Writes out the `ReportDefinition`.
/// The caller collects the items that will become rows and passes them in as the generic
/// `Vec<T>` parameter.
/// Then `ReportWriter` calls `build_rows` with the items and context, if provided, where the
/// definition transforms the items into rows.
///
/// The lifetime is required because we're storing a reference to `ValidatedConfig`, not owning it.
pub(super) struct ReportWriter<'a, T: Clone> {
    pub(super) items:            Vec<T>,
    pub(super) validated_config: Option<&'a ValidatedConfig>,
}

impl<'a, T: Clone> ReportWriter<'a, T> {
    pub(super) const fn new(items: Vec<T>) -> Self {
        Self {
            items,
            validated_config: None,
        }
    }

    pub(super) fn with_validated_config(self, validated_config: &'a ValidatedConfig) -> Self {
        Self {
            validated_config: Some(validated_config),
            ..self
        }
    }

    /// Write the table using the provided builder and output file writer
    pub(super) fn write<B: ReportDefinition<Item = T>>(
        &self,
        report: &B,
        output_file_writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if self.items.is_empty() {
            return Ok(());
        }

        // `Report::title` supplies the optional heading row.
        if let Some(title) = report.title() {
            output_file_writer.writeln(report.level(), &title)?;
        }

        // `Report::description` supplies the optional body text.
        let description = &report.description(&self.items);
        if !description.is_empty() {
            output_file_writer.writeln("", &report.description(&self.items))?;
        }

        // Report::show_empty controls whether empty tables are written.
        if self.items.is_empty() {
            return Ok(());
        }

        let headers = report.headers();
        let alignments = report.alignments();
        let rows = report.build_rows(&self.items, self.validated_config)?;

        output_file_writer.write_markdown_table(&headers, &rows, Some(&alignments))?;

        Ok(())
    }
}

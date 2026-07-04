use std::error::Error;

pub(super) use super::definition::ReportDefinition;
use crate::output_file_writer::OutputFileWriter;
use crate::validated_config::ValidatedConfig;

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

    /// Writes `report` rows through `OutputFileWriter`.
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

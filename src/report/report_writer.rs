use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use std::error::Error;

/// definition of the elements of a report to write out as a markdown table
pub trait ReportDefinition<C = ()> {
    /// The type of data being displayed in the table
    type Item;
    /// Get the table headers
    fn headers(&self) -> Vec<&str>;

    /// Get column alignments for the table
    fn alignments(&self) -> Vec<ColumnAlignment>;

    /// Transform data items into table rows
    ///
    /// simple reports can use "_: &()" for this generic parameter so they don't
    /// need to use it and the compiler won't complain
    ///
    /// reports that need config information can use "report_context: &ReportContext"
    /// to access properties such as appLy_changes or obsidian_path
    ///
    /// it's slightly hacky but prevents having to dramatically alter the structure and it's
    /// readable enough
    fn build_rows(
        &self,
        items: &[Self::Item],
        config: Option<&ValidatedConfig>,
    ) -> Vec<Vec<String>>;

    /// Optional table title
    fn title(&self) -> Option<String> {
        None
    }

    /// Optional table description/summary
    fn description(&self, items: &[Self::Item]) -> String;

    /// markdown level
    fn level(&self) -> &'static str;

    fn hide_title_if_no_rows(&self) -> bool {
        true
    }
}

/// writes out the TableDefinition
/// the idea is you collect all the items that will get turned into rows and pass them
/// in to the generic Vec<T> parameter
/// then the ReportWriter will call build_rows with the items and the context (if provided)
/// where the definition will do the work to transform items into rows
///
/// lifetime attribute required because we're storing a reference to ValidatedConfig - not owning it
pub struct ReportWriter<'a, T: Clone> {
    pub(crate) items: Vec<T>,
    pub(crate) validated_config: Option<&'a ValidatedConfig>,
}

impl<'a, T: Clone> ReportWriter<'a, T> {
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            validated_config: None,
        }
    }

    pub fn with_validated_config(self, config: &'a ValidatedConfig) -> Self {
        Self {
            validated_config: Some(config),
            ..self
        }
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
            writer.writeln(report.level(), &title)?;
        }

        // Write description if present
        let description = &report.description(&self.items);
        if !description.is_empty() {
            writer.writeln("", &report.description(&self.items))?;
        }

        // Skip empty tables unless overridden
        if self.items.is_empty() {
            return Ok(());
        }

        // Build and write the table
        let headers = report.headers();
        let alignments = report.alignments();
        let rows = report.build_rows(&self.items, self.validated_config);

        writer.write_markdown_table(&headers, &rows, Some(&alignments))?;

        Ok(())
    }
}

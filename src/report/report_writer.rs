use crate::markdown_file_info::MarkdownFileInfo;
use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;

/// definition of the elements of a report to write out as a markdown table
pub trait ReportDefinition<C = ()> {
    /// The type of data being displayed in the table
    type Item;

    /// Get paths that should be checked against files_to_persist for filtering
    /// Returns empty Vec if no filtering should be applied
    /// this allows us to use our knowledge of the types in each report
    /// to return just what is actually necessary
    ///
    /// So for different report types:
    /// - For unreferenced images: return empty Vec (no filtering needed)
    /// - For duplicate images: return paths of all referencing files
    /// - For ambiguous matches: return the single relative_path
    /// - For back populate matches: return the single relative_path
    fn get_item_filter_paths(&self, _item: &Self::Item) -> Vec<PathBuf> {
        Vec::new() // Default implementation = no filtering
    }

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
    pub(crate) files_to_persist: Option<&'a [&'a MarkdownFileInfo]>,
}

impl<'a, T: Clone> ReportWriter<'a, T> {
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            validated_config: None,
            files_to_persist: None,
        }
    }

    pub fn with_validated_config(self, config: &'a ValidatedConfig) -> Self {
        Self {
            validated_config: Some(config),
            ..self
        }
    }

    #[allow(dead_code)]
    pub fn with_files_to_persist(self, files: &'a [&'a MarkdownFileInfo]) -> Self {
        Self {
            files_to_persist: Some(files),
            ..self
        }
    }

    fn filter_items<B: ReportDefinition<Item = T>>(&self, report: &B) -> Vec<T> {
        if let Some(files) = self.files_to_persist {
            let persist_paths: HashSet<_> = files.iter().map(|f| &f.path).collect();

            self.items
                .iter()
                .filter(|item| {
                    let paths = report.get_item_filter_paths(item);
                    // If no paths returned, include the item
                    // If paths returned, all paths must be in persist_paths
                    paths.is_empty() || paths.iter().all(|p| persist_paths.contains(p))
                })
                .cloned()
                .collect()
        } else {
            self.items.clone()
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

        let filtered_items = self.filter_items(report);

        if filtered_items.is_empty() && report.hide_title_if_no_rows() {
            return Ok(());
        }

        // Write title if present
        if let Some(title) = report.title() {
            writer.writeln(report.level(), &title)?;
        }

        // Write description if present
        let description = &report.description(&self.items);
        if description.len() > 0 {
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

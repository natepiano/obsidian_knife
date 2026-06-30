use anyhow::Result as AnyhowResult;

use crate::output_file_writer::ColumnAlignment;
use crate::validated_config::ValidatedConfig;

/// `ReportDefinition` supplies table headers, alignments, rows, title,
/// description, and heading level to `ReportWriter`.
pub(super) trait ReportDefinition<C = ()> {
    /// The type of data being displayed in the table
    type Item;
    /// Returns the table header labels.
    fn headers(&self) -> Vec<&str>;

    /// Returns the table `ColumnAlignment` values.
    fn alignments(&self) -> Vec<ColumnAlignment>;

    /// Returns Markdown table rows for `Self::Item` values.
    ///
    /// Reports that do not need `ValidatedConfig` use `_` for this parameter.
    ///
    /// Reports that need configuration read fields such as `ValidatedConfig.change_mode`
    /// or `ValidatedConfig.obsidian_path` from this parameter.
    fn build_rows(
        &self,
        items: &[Self::Item],
        validated_config: Option<&ValidatedConfig>,
    ) -> AnyhowResult<Vec<Vec<String>>>;

    /// Returns the optional table title.
    fn title(&self) -> Option<String> { None }

    /// Returns the optional table description.
    fn description(&self, items: &[Self::Item]) -> String;

    /// markdown level
    fn level(&self) -> &'static str;
}

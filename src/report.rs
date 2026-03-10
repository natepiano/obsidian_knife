mod ambiguous_matches_report;
mod back_populate_report;
mod duplicate_images_report;
mod frontmatter_issues_report;
mod incompatible_image_report;
mod invalid_wikilink_report;
mod missing_references_report;
mod orchestration;
mod persist_reasons_report;
mod unreferenced_images_report;

mod report_writer;

use crate::constants::*;
use crate::obsidian_repository::ObsidianRepository;
use crate::validated_config::ValidatedConfig;

impl ObsidianRepository {
    pub fn write_reports(
        &self,
        validated_config: &ValidatedConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.write_reports_impl(validated_config)
    }
}

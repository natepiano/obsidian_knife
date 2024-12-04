use crate::obsidian_repository_info::obsidian_repository_info_types::GroupedImages;
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::utils::ThreadSafeWriter;
use crate::validated_config::ValidatedConfig;
use std::error::Error;
use std::path::PathBuf;

impl ObsidianRepositoryInfo {
    pub fn write_reports(
        &self,
        validated_config: &ValidatedConfig,
        grouped_images: &GroupedImages,
        missing_references: &Vec<(PathBuf, String)>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let writer = ThreadSafeWriter::new(validated_config.output_folder())?;
        crate::write_execution_start(&validated_config, &writer)?;

        self.markdown_files.report_frontmatter_issues(&writer)?;
        self.write_invalid_wikilinks_table(&writer)?;
        self.write_image_analysis(
            &validated_config,
            &writer,
            &grouped_images,
            &missing_references,
        )?;
        self.write_back_populate_tables(&validated_config, &writer)?;
        self.markdown_files.write_persist_reasons_table(&writer)?;
        Ok(())
    }
}

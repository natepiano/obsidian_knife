use tempfile::TempDir;

use crate::validated_config::ValidatedConfig;
use crate::validated_config::ValidatedConfigBuilder;
use crate::validated_config::ValidationError;

pub fn get_test_validated_config_builder(temp_dir: &TempDir) -> ValidatedConfigBuilder {
    let mut builder = ValidatedConfigBuilder::default();
    builder.obsidian_path(temp_dir.path().to_path_buf());
    builder.output_folder(temp_dir.path().join("output"));
    builder
}

pub fn get_test_validated_config_result(
    temp_dir: &TempDir,
    modifier: impl FnOnce(&mut ValidatedConfigBuilder),
) -> Result<ValidatedConfig, ValidationError> {
    let mut builder = get_test_validated_config_builder(temp_dir);
    modifier(&mut builder);
    builder.build()
}

pub fn get_test_validated_config(
    temp_dir: &TempDir,
    back_populate_file_filter: Option<&str>,
) -> ValidatedConfig {
    get_test_validated_config_result(temp_dir, |builder| {
        if let Some(filter) = back_populate_file_filter {
            builder.back_populate_file_filter(Some(filter.to_string()));
        }
    })
    .unwrap()
}

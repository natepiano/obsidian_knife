use crate::constants::*;
use crate::obsidian_repository_info::{write_back_populate_table, ObsidianRepositoryInfo};
use crate::utils::ThreadSafeWriter;
use crate::wikilink_types::ToWikilink;
use crate::ValidatedConfig;
use std::error::Error;

pub fn write_back_populate_tables(
    config: &ValidatedConfig,
    obsidian_repository_info: &mut ObsidianRepositoryInfo,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL1, BACK_POPULATE_COUNT_PREFIX)?;

    if let Some(filter) = config.back_populate_file_filter() {
        writer.writeln(
            "",
            &format!(
                "{} {}\n{}\n",
                BACK_POPULATE_FILE_FILTER_PREFIX,
                filter.to_wikilink(),
                BACK_POPULATE_FILE_FILTER_SUFFIX
            ),
        )?;
    }

    // only writes if there are any
    obsidian_repository_info.write_ambiguous_matches_table(writer)?;

    let unambiguous_matches = obsidian_repository_info
        .markdown_files
        .unambiguous_matches();

    if !unambiguous_matches.is_empty() {
        write_back_populate_table(
            writer,
            &unambiguous_matches,
            true,
            obsidian_repository_info.wikilinks_sorted.len(),
        )?;
    }

    Ok(())
}

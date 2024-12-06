#[cfg(test)]
mod scan_tests;

use crate::{
    markdown_file_info::MarkdownFileInfo, obsidian_repository_info::ObsidianRepositoryInfo,
};

use crate::markdown_file_info::ImageLink;
use crate::markdown_files::MarkdownFiles;
use crate::utils::collect_repository_files;
use crate::utils::Timer;
use crate::wikilink::Wikilink;
use crate::ValidatedConfig;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use rayon::prelude::*;
use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub fn pre_scan_obsidian_repo(
    config: &ValidatedConfig,
) -> Result<ObsidianRepositoryInfo, Box<dyn Error + Send + Sync>> {
    let _timer = Timer::new("scan_obsidian_folder");

    let obsidian_repository_info = pre_scan_folders(config)?;

    Ok(obsidian_repository_info)
}

pub fn pre_scan_folders(
    config: &ValidatedConfig,
) -> Result<ObsidianRepositoryInfo, Box<dyn Error + Send + Sync>> {
    let ignore_folders = config.ignore_folders().unwrap_or(&[]);
    let mut obsidian_repository_info = ObsidianRepositoryInfo::default();

    let (markdown_paths, image_files, other_files) =
        collect_repository_files(config, ignore_folders)?;

    obsidian_repository_info.other_files = other_files;

    // Get markdown files info and accumulate all_wikilinks from scan_markdown_files
    let (markdown_files, all_wikilinks) =
        pre_scan_markdown_files(&markdown_paths, config.operational_timezone())?;

    let (sorted, ac) = sort_and_build_wikilinks_ac(all_wikilinks);
    obsidian_repository_info.wikilinks_sorted = sorted;
    obsidian_repository_info.wikilinks_ac = Some(ac);
    obsidian_repository_info.markdown_files = markdown_files;

    partition_found_and_missing_image_references(
        config,
        &mut obsidian_repository_info,
        &image_files,
    )?;

    Ok(obsidian_repository_info)
}

fn partition_found_and_missing_image_references(
    config: &ValidatedConfig,
    obsidian_repository_info: &mut ObsidianRepositoryInfo,
    image_files: &Vec<PathBuf>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Process image info
    obsidian_repository_info.image_path_to_references_map = obsidian_repository_info
        .markdown_files
        .get_image_info_map(config, &image_files)?;

    let image_filenames: HashSet<String> = image_files
        .iter()
        .filter_map(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_lowercase())
        .collect();

    fn image_exists_in_set(image_filename: &str, image_filenames: &HashSet<String>) -> bool {
        image_filenames.contains(&image_filename.to_lowercase())
    }

    // Update each markdown file's image links
    for markdown_file in obsidian_repository_info.markdown_files.iter_mut() {
        let (found, missing): (Vec<ImageLink>, Vec<ImageLink>) = markdown_file
            .image_links
            .found
            .drain(..)
            .partition(|link| image_exists_in_set(&link.filename, &image_filenames));

        markdown_file.image_links.found = found;
        markdown_file.image_links.missing = missing;
    }
    Ok(())
}

fn compare_wikilinks(a: &Wikilink, b: &Wikilink) -> std::cmp::Ordering {
    b.display_text
        .len()
        .cmp(&a.display_text.len())
        .then(a.display_text.cmp(&b.display_text))
        .then_with(|| match (a.is_alias, b.is_alias) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.target.cmp(&b.target),
        })
}

fn sort_and_build_wikilinks_ac(all_wikilinks: HashSet<Wikilink>) -> (Vec<Wikilink>, AhoCorasick) {
    let mut wikilinks: Vec<_> = all_wikilinks.into_iter().collect();
    wikilinks.sort_unstable_by(compare_wikilinks);

    let mut patterns = Vec::with_capacity(wikilinks.len());
    patterns.extend(wikilinks.iter().map(|w| w.display_text.as_str()));

    let ac = AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .match_kind(MatchKind::LeftmostLongest)
        .build(&patterns)
        .expect("Failed to build Aho-Corasick automaton for wikilinks");

    (wikilinks, ac)
}

fn pre_scan_markdown_files(
    markdown_paths: &[PathBuf],
    timezone: &str,
) -> Result<(MarkdownFiles, HashSet<Wikilink>), Box<dyn Error + Send + Sync>> {
    // Use Arc<Mutex<...>> for safe shared collection
    let markdown_files = Arc::new(Mutex::new(MarkdownFiles::new()));
    // todo: just collect this at the end as you don't need to build it as you go
    //       this will reduce thread contention i would hope
    let all_wikilinks = Arc::new(Mutex::new(HashSet::new()));

    markdown_paths.par_iter().try_for_each(|file_path| {
        match MarkdownFileInfo::new(file_path.clone(), timezone) {
            Ok(file_info) => {
                let wikilinks = file_info.wikilinks.valid.clone();
                markdown_files.lock().unwrap().push(file_info);
                all_wikilinks.lock().unwrap().extend(wikilinks);
                Ok(())
            }
            Err(e) => {
                eprintln!("Error processing file {:?}: {}", file_path, e);
                Err(e)
            }
        }
    })?;

    // Extract data from Arc<Mutex<...>>
    let markdown_files = Arc::try_unwrap(markdown_files)
        .unwrap()
        .into_inner()
        .unwrap();
    let all_wikilinks = Arc::try_unwrap(all_wikilinks)
        .unwrap()
        .into_inner()
        .unwrap();

    Ok((markdown_files, all_wikilinks))
}

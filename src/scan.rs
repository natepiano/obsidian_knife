#[cfg(test)]
mod scan_tests;

use crate::{
    constants::*, file_utils::collect_repository_files, markdown_file_info::MarkdownFileInfo,
    obsidian_repository_info::ObsidianRepositoryInfo, wikilink::collect_file_wikilinks,
    wikilink_types::Wikilink,
};

use crate::config::ValidatedConfig;
use crate::utils::Sha256Cache;
use crate::utils::Timer;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use rayon::prelude::*;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub hash: String,
    pub(crate) references: Vec<String>,
}

pub fn scan_obsidian_folder(
    config: &ValidatedConfig,
) -> Result<ObsidianRepositoryInfo, Box<dyn Error + Send + Sync>> {
    let _timer = Timer::new("scan_obsidian_folder");

    let obsidian_repository_info = scan_folders(config)?;

    Ok(obsidian_repository_info)
}

fn get_image_info_map(
    config: &ValidatedConfig,
    markdown_files: &[MarkdownFileInfo],
    image_files: &[PathBuf],
) -> Result<HashMap<PathBuf, ImageInfo>, Box<dyn Error + Send + Sync>> {
    let _timer = Timer::new("get_image_info_map");

    let cache_file_path = config.obsidian_path().join(CACHE_FOLDER).join(CACHE_FILE);
    let cache = Arc::new(Mutex::new(Sha256Cache::new(cache_file_path.clone())?.0));

    // Pre-process markdown references
    let markdown_refs: HashMap<String, Vec<String>> = markdown_files
        .par_iter()
        .filter(|file_info| !file_info.image_links.is_empty())
        .map(|file_info| {
            let path = file_info.path.to_string_lossy().to_string();
            let images: HashSet<_> = file_info
                .image_links
                .iter()
                .map(|link| link.to_string())
                .collect();
            (path, images.into_iter().collect())
        })
        .collect();

    // Process images
    let image_info_map: HashMap<_, _> = image_files
        .par_iter()
        .filter_map(|image_path| {
            let hash = cache.lock().ok()?.get_or_update(image_path).ok()?.0;

            let image_name = image_path.file_name()?.to_str()?;
            let references: Vec<String> = markdown_refs
                .iter()
                .filter_map(|(path, links)| {
                    if links.iter().any(|link| link.contains(image_name)) {
                        Some(path.clone())
                    } else {
                        None
                    }
                })
                .collect();

            Some((image_path.clone(), ImageInfo { hash, references }))
        })
        .collect();

    // Final cache operations
    if let Ok(mut cache) = Arc::try_unwrap(cache).unwrap().into_inner() {
        cache.remove_non_existent_entries();
        cache.save()?;
    }

    Ok(image_info_map)
}

pub fn scan_folders(
    config: &ValidatedConfig,
) -> Result<ObsidianRepositoryInfo, Box<dyn Error + Send + Sync>> {
    let ignore_folders = config.ignore_folders().unwrap_or(&[]);
    let mut obsidian_repository_info = ObsidianRepositoryInfo::default();

    let (markdown_files, image_files, other_files) =
        collect_repository_files(config, ignore_folders)?;

    obsidian_repository_info.other_files = other_files;

    // Get markdown files info and accumulate all_wikilinks from scan_markdown_files
    let (markdown_info, all_wikilinks) = scan_markdown_files(&markdown_files)?;
    obsidian_repository_info.markdown_files = markdown_info;

    let (sorted, ac) = sort_and_build_wikilinks_ac(all_wikilinks);
    obsidian_repository_info.wikilinks_sorted = sorted;
    obsidian_repository_info.wikilinks_ac = Some(ac);

    // Process image info
    obsidian_repository_info.image_map = get_image_info_map(
        config,
        &obsidian_repository_info.markdown_files,
        &image_files,
    )?;

    Ok(obsidian_repository_info)
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

fn scan_markdown_files(
    markdown_files: &[PathBuf],
) -> Result<(Vec<MarkdownFileInfo>, HashSet<Wikilink>), Box<dyn Error + Send + Sync>> {
    let _timer = Timer::new("scan_markdown_files");

    let extensions_pattern = IMAGE_EXTENSIONS.join("|");
    let image_regex = Arc::new(Regex::new(&format!(
        r"(!\[(?:[^\]]*)\]\([^)]+\)|!\[\[([^\]]+\.(?:{}))(?:\|[^\]]+)?\]\])",
        extensions_pattern
    ))?);

    // Use Arc<Mutex<...>> for safe shared collection
    let markdown_info = Arc::new(Mutex::new(Vec::new()));
    let all_wikilinks = Arc::new(Mutex::new(HashSet::new()));

    markdown_files.par_iter().try_for_each(|file_path| {
        match scan_markdown_file(file_path, &image_regex) {
            Ok((file_info, wikilinks)) => {
                markdown_info.lock().unwrap().push(file_info);
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
    let markdown_info = Arc::try_unwrap(markdown_info)
        .unwrap()
        .into_inner()
        .unwrap();
    let all_wikilinks = Arc::try_unwrap(all_wikilinks)
        .unwrap()
        .into_inner()
        .unwrap();

    Ok((markdown_info, all_wikilinks))
}

fn scan_markdown_file(
    file_path: &PathBuf,
    image_regex: &Arc<Regex>,
) -> Result<(MarkdownFileInfo, Vec<Wikilink>), Box<dyn Error + Send + Sync>> {
    let mut markdown_file_info = MarkdownFileInfo::new(file_path.clone())?;

    // extract_do_not_back_populate(&mut markdown_file_info);

    let aliases = markdown_file_info
        .frontmatter
        .as_ref()
        .and_then(|fm| fm.aliases().cloned());

    // collect_file_wikilinks constructs a set of wikilinks from the content (&content),
    // the aliases (&aliases) in the frontmatter and the name of the file itself (file_path)
    let extracted_wikilinks =
        collect_file_wikilinks(&markdown_file_info.content, &aliases, file_path)?;

    // Store invalid wikilinks in markdown_file_info
    markdown_file_info.add_invalid_wikilinks(extracted_wikilinks.invalid);

    collect_image_references(image_regex, &mut markdown_file_info)?;

    Ok((markdown_file_info, extracted_wikilinks.valid))
}

fn collect_image_references(
    image_regex: &Arc<Regex>,
    markdown_file_info: &mut MarkdownFileInfo,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let reader = BufReader::new(markdown_file_info.content.as_bytes());

    for line_result in reader.lines() {
        let line = line_result?;
        for capture in image_regex.captures_iter(&line) {
            if let Some(reference) = capture.get(0) {
                let reference_string = reference.as_str().to_string();
                markdown_file_info.image_links.push(reference_string);
            }
        }
    }

    Ok(())
}

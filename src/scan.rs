#[cfg(test)]
mod scan_tests;

use crate::markdown_file_info::MarkdownFileInfo;

use crate::markdown_file_info::ImageLink;
use crate::markdown_files::MarkdownFiles;
use crate::wikilink::Wikilink;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use rayon::prelude::*;
use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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

pub(crate) fn sort_and_build_wikilinks_ac(
    all_wikilinks: HashSet<Wikilink>,
) -> (Vec<Wikilink>, AhoCorasick) {
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

pub(crate) fn pre_scan_markdown_files(
    markdown_paths: &[PathBuf],
    timezone: &str,
) -> Result<MarkdownFiles, Box<dyn Error + Send + Sync>> {
    // Use Arc<Mutex<...>> for safe shared collection
    let markdown_files = Arc::new(Mutex::new(MarkdownFiles::new()));

    markdown_paths.par_iter().try_for_each(|file_path| {
        match MarkdownFileInfo::new(file_path.clone(), timezone) {
            Ok(file_info) => {
                markdown_files.lock().unwrap().push(file_info);
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

    Ok(markdown_files)
}

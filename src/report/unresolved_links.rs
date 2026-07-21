use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;

use anyhow::Result as AnyhowResult;

use super::support;
use super::writer::ReportDefinition;
use super::writer::ReportWriter;
use crate::constants::FILES;
use crate::constants::FOUND;
use crate::constants::IN;
use crate::constants::LEVEL1;
use crate::constants::LEVEL2;
use crate::constants::LINK_CLICK_TO_CREATE;
use crate::constants::OCCURRENCES;
use crate::constants::UNRESOLVED_LINKS;
use crate::constants::UNRESOLVED_LINKS_DESCRIPTION;
use crate::description_builder::DescriptionBuilder;
use crate::obsidian_repository::ObsidianRepository;
use crate::obsidian_repository::UnresolvedLink;
use crate::output_file_writer::ColumnAlignment;
use crate::output_file_writer::OutputFileWriter;
use crate::phrase::Phrase;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::ToWikilink;

struct UnresolvedLinksTable;

impl ReportDefinition for UnresolvedLinksTable {
    type Item = UnresolvedLink;

    fn headers(&self) -> Vec<&str> { vec![LINK_CLICK_TO_CREATE, OCCURRENCES, FILES] }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![
            ColumnAlignment::Left,
            ColumnAlignment::Center,
            ColumnAlignment::Center,
        ]
    }

    fn build_rows(
        &self,
        items: &[Self::Item],
        _: Option<&ValidatedConfig>,
    ) -> AnyhowResult<Vec<Vec<String>>> {
        // `ObsidianRepository::collect_unresolved_links` sorts by target, so one aggregated
        // group per missing note comes from grouping consecutive `UnresolvedLink` items;
        // rows then order by occurrences descending, target ascending on ties.
        let mut groups: Vec<(String, usize, HashSet<&PathBuf>)> = Vec::new();

        for unresolved_link in items {
            match groups.last_mut() {
                Some((target, occurrences, files))
                    if target.eq_ignore_ascii_case(&unresolved_link.target) =>
                {
                    *occurrences += 1;
                    files.insert(&unresolved_link.file_path);
                },
                _ => groups.push((
                    unresolved_link.target.clone(),
                    1,
                    HashSet::from([&unresolved_link.file_path]),
                )),
            }
        }

        groups.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        Ok(groups.into_iter().map(group_row).collect())
    }

    fn description(&self, items: &[Self::Item]) -> String {
        let unique_files: HashSet<&PathBuf> = items.iter().map(|link| &link.file_path).collect();

        DescriptionBuilder::new()
            .text(FOUND)
            .pluralize_with_count(Phrase::Wikilink(items.len()))
            .text(IN)
            .pluralize_with_count(Phrase::File(unique_files.len()))
            .text_with_newline("")
            .no_space(UNRESOLVED_LINKS_DESCRIPTION)
            .build()
    }

    fn level(&self) -> &'static str { LEVEL2 }
}

fn group_row((target, occurrences, files): (String, usize, HashSet<&PathBuf>)) -> Vec<String> {
    // The wikilink stays unescaped so Obsidian renders it clickable - clicking an
    // unresolved link creates the note. The output folder is in `ignore_folders`,
    // so the report's own links are never scanned back in.
    vec![
        support::escape_pipe(&target.to_wikilink()),
        occurrences.to_string(),
        files.len().to_string(),
    ]
}

impl ObsidianRepository {
    pub(super) fn write_unresolved_links_report(
        &self,
        output_file_writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let unresolved_links = self.collect_unresolved_links();
        if unresolved_links.is_empty() {
            return Ok(());
        }

        output_file_writer.writeln(LEVEL1, UNRESOLVED_LINKS)?;

        let report_writer = ReportWriter::new(unresolved_links);
        report_writer.write(&UnresolvedLinksTable, output_file_writer)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn unresolved_link(target: &str, file: &str, line_number: usize) -> UnresolvedLink {
        UnresolvedLink {
            target: target.to_string(),
            file_path: PathBuf::from(file),
            line_number,
        }
    }

    #[test]
    fn test_unresolved_links_aggregate_per_target() {
        let items = vec![
            unresolved_link("Missing Note", "2026-01-01.md", 5),
            unresolved_link("Missing Note", "2026-01-01.md", 9),
            unresolved_link("Missing Note", "waiting.md", 2),
            unresolved_link("Other Note", "waiting.md", 3),
            unresolved_link("Zebra Note", "2026-01-01.md", 1),
            unresolved_link("Zebra Note", "2026-01-02.md", 4),
            unresolved_link("Zebra Note", "2026-01-03.md", 6),
            unresolved_link("Zebra Note", "waiting.md", 8),
        ];

        let rows = UnresolvedLinksTable.build_rows(&items, None).unwrap();

        assert_eq!(rows.len(), 3, "one row per missing note");
        assert_eq!(
            rows[0][0], "[[Zebra Note]]",
            "rows sort by occurrences descending, not target"
        );
        assert_eq!(rows[0][1], "4");
        assert_eq!(
            rows[1][0], "[[Missing Note]]",
            "link renders unescaped so Obsidian makes it clickable"
        );
        assert_eq!(rows[1][1], "3", "occurrence count spans files");
        assert_eq!(rows[1][2], "2", "file count is distinct");
        assert_eq!(rows[2][0], "[[Other Note]]");
        assert_eq!(rows[2][1], "1");
        assert_eq!(rows[2][2], "1");
    }
}

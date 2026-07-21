use std::collections::HashMap;
use std::ffi::OsStr;

use super::MarkdownFile;
use super::back_populate;
use super::replaceable_content::MatchType;
use super::replaceable_content::ReplaceableContent;
use super::text_excluder::CodeBlockExcluder;
use crate::constants::ESCAPED_PIPE;
use crate::constants::PIPE;
use crate::support;
use crate::validated_config::ValidatedConfig;
use crate::wikilink;
use crate::wikilink::SpannedWikilink;
use crate::wikilink::ToWikilink;

/// One wikilink whose target note does not exist but whose target text matches exactly one
/// real alias or filename; `replacement` re-targets the link at that note.
#[derive(Clone, Debug)]
pub struct PhantomLinkMatch {
    pub found_text:    String,
    pub line_number:   usize,
    pub position:      usize,
    pub relative_path: String,
    pub replacement:   String,
}

impl ReplaceableContent for PhantomLinkMatch {
    fn line_number(&self) -> usize { self.line_number }

    fn position(&self) -> usize { self.position }

    fn get_replacement(&self) -> String { self.replacement.clone() }

    fn matched_text(&self) -> String { self.found_text.clone() }

    fn match_type(&self) -> MatchType { MatchType::PhantomLink }
}

impl MarkdownFile {
    /// `resolutions` maps each lowercased phantom target to the real note target its text
    /// uniquely matches; every content wikilink with a resolved target becomes a
    /// `PhantomLinkMatch`.
    pub(crate) fn find_phantom_link_matches(
        &self,
        resolutions: &HashMap<String, String>,
        validated_config: &ValidatedConfig,
    ) -> Vec<PhantomLinkMatch> {
        let file_stem = self
            .path
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or_default();

        let mut matches = Vec::new();
        self.for_each_content_wikilink(|line_number, line, spanned_wikilink| {
            let SpannedWikilink { wikilink, span } = spanned_wikilink;
            let Some(real_target) = resolutions.get(&wikilink.target.to_lowercase()) else {
                return;
            };

            // A converted link inside the resolved note itself would be a self-link.
            if real_target.eq_ignore_ascii_case(file_stem) {
                return;
            }

            let (start, end) = span;
            let found_text = line[start..end].to_string();

            let mut replacement = real_target.to_aliased_wikilink(&wikilink.display_text);
            if back_populate::is_in_markdown_table(line, &found_text) {
                replacement = replacement.replace(PIPE, ESCAPED_PIPE);
            }

            matches.push(PhantomLinkMatch {
                found_text,
                line_number,
                position: start,
                relative_path: support::format_relative_path(
                    &self.path,
                    validated_config.obsidian_path(),
                ),
                replacement,
            });
        });

        matches
    }

    /// Runs `callback` with the one-based content line number, the line text, and each
    /// `SpannedWikilink` that `wikilink::extract_wikilinks` finds outside code blocks.
    pub(crate) fn for_each_content_wikilink(
        &self,
        mut callback: impl FnMut(usize, &str, SpannedWikilink),
    ) {
        let mut code_block_excluder = CodeBlockExcluder::new();

        for (line_idx, line) in self.content.lines().enumerate() {
            code_block_excluder.update(line);
            if code_block_excluder.is_in_code_block() {
                continue;
            }

            for spanned_wikilink in wikilink::extract_wikilinks(line).valid {
                callback(self.get_real_line_number(line_idx), line, spanned_wikilink);
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::collections::HashMap;

    use crate::test_support;
    use crate::test_support::TestFileBuilder;
    use crate::validated_config::ChangeMode;

    fn kali_resolutions() -> HashMap<String, String> {
        HashMap::from([("kali".to_string(), "Kali Amen".to_string())])
    }

    #[test]
    fn test_find_phantom_link_matches_bare_and_piped() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        let file_path = TestFileBuilder::new()
            .with_content("[[Kali]] was here\ntalk to [[Kali|K-A]] tonight")
            .create(&temp_dir, "diary.md");
        let markdown_file = test_support::get_test_markdown_file(file_path);

        let matches =
            markdown_file.find_phantom_link_matches(&kali_resolutions(), &validated_config);

        assert_eq!(matches.len(), 2, "both phantom links should match");
        assert_eq!(matches[0].found_text, "[[Kali]]");
        assert_eq!(matches[0].replacement, "[[Kali Amen|Kali]]");
        assert_eq!(matches[0].position, 0);
        assert_eq!(matches[1].found_text, "[[Kali|K-A]]");
        assert_eq!(
            matches[1].replacement, "[[Kali Amen|K-A]]",
            "piped phantom links keep their display text"
        );
    }

    #[test]
    fn test_find_phantom_link_matches_escapes_table_pipes() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        let file_path = TestFileBuilder::new()
            .with_content("| [[Kali\\|K]] | note |")
            .create(&temp_dir, "diary.md");
        let markdown_file = test_support::get_test_markdown_file(file_path);

        let matches =
            markdown_file.find_phantom_link_matches(&kali_resolutions(), &validated_config);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].found_text, "[[Kali\\|K]]");
        assert_eq!(
            matches[0].replacement, "[[Kali Amen\\|K]]",
            "markdown table replacements keep escaped pipes"
        );
    }

    #[test]
    fn test_find_phantom_link_matches_skips_self_link() {
        let (temp_dir, validated_config, _) =
            test_support::create_test_environment(ChangeMode::DryRun, None, Some(vec![]), None);

        let file_path = TestFileBuilder::new()
            .with_content("[[Kali]] appears on the target's own page")
            .create(&temp_dir, "Kali Amen.md");
        let markdown_file = test_support::get_test_markdown_file(file_path);

        let matches =
            markdown_file.find_phantom_link_matches(&kali_resolutions(), &validated_config);

        assert!(
            matches.is_empty(),
            "converting inside the resolved note would create a self-link"
        );
    }
}

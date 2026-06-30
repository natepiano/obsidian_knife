use crate::constants::CLOSING_WIKILINK;
use crate::constants::MARKDOWN_SUFFIX;
use crate::constants::OPENING_WIKILINK;
use crate::constants::PIPE;

/// `ToWikilink` converts strings to wikilink text.
pub trait ToWikilink {
    /// Returns `self` as `[[target]]` wikilink text.
    fn to_wikilink(&self) -> String;

    /// Builds an aliased wikilink from `self` and `display_text`.
    /// Matching target and display text return `[[target]]`; differing values return
    /// `[[target|display]]`.
    fn to_aliased_wikilink(&self, display_text: &str) -> String
    where
        Self: AsRef<str>,
    {
        let target_without_markdown = strip_markdown_extension(self.as_ref());

        if target_without_markdown == display_text {
            target_without_markdown.to_wikilink()
        } else {
            format!(
                "{OPENING_WIKILINK}{target_without_markdown}{PIPE}{display_text}{CLOSING_WIKILINK}"
            )
        }
    }
}

impl ToWikilink for str {
    fn to_wikilink(&self) -> String {
        format!(
            "{OPENING_WIKILINK}{}{CLOSING_WIKILINK}",
            strip_markdown_extension(self)
        )
    }
}

impl ToWikilink for String {
    fn to_wikilink(&self) -> String { self.as_str().to_wikilink() }
}

/// Removes `MARKDOWN_SUFFIX` from `text` when it is present.
fn strip_markdown_extension(text: &str) -> &str {
    text.strip_suffix(MARKDOWN_SUFFIX).unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use crate::wikilink::ToWikilink;

    #[test]
    fn to_aliased_wikilink_variants() {
        let test_cases = vec![
            ("target", "target", "[[target]]"),
            ("Target", "target", "[[Target|target]]"),
            ("test link", "Test Link", "[[test link|Test Link]]"),
            ("Apple", "fruit", "[[Apple|fruit]]"),
            ("Home", "主页", "[[Home|主页]]"),
            ("page.md", "Page", "[[page|Page]]"),
            ("café", "咖啡", "[[café|咖啡]]"),
            ("テスト", "Test", "[[テスト|Test]]"),
        ];

        for (target, display, expected) in test_cases {
            let result = target.to_aliased_wikilink(display);
            assert_eq!(
                result, expected,
                "Failed for target '{target}', display '{display}'"
            );
        }

        let string_target = String::from("Target");
        assert_eq!(
            string_target.to_aliased_wikilink("target"),
            "[[Target|target]]"
        );
        assert_eq!(string_target.to_aliased_wikilink("Target"), "[[Target]]");
    }
}

use std::sync::Arc;
use regex::Regex;
use tempfile::TempDir;
use crate::IMAGE_EXTENSIONS;
use crate::scan::process_content;
use crate::test_utils::TestFileBuilder;
use crate::wikilink_types::{InvalidWikilinkReason, Wikilink};

fn assert_contains_wikilink(
    wikilinks: &[Wikilink],
    target: &str,
    display: Option<&str>,
    is_alias: bool,
) {
    let exists = wikilinks.iter().any(|w| {
        w.target == target && w.display_text == display.unwrap_or(target) && w.is_alias == is_alias
    });
    assert!(
        exists,
        "Expected wikilink with target '{}', display '{:?}', is_alias '{}'",
        target, display, is_alias
    );
}

fn create_image_regex() -> Arc<Regex> {
    let extensions_pattern = IMAGE_EXTENSIONS.join("|");
    Arc::new(Regex::new(&format!(
        r"(!\[(?:[^\]]*)\]\([^)]+\)|!\[\[([^\]]+\.(?:{}))(?:\|[^\]]+)?\]\])",
        extensions_pattern
    )).unwrap())
}

#[test]
fn test_process_content_with_aliases() {
    let content = "# Test\nHere's a [[Regular Link]] and [[Target|Display Text]]";
    let aliases = Some(vec!["Alias One".to_string(), "Alias Two".to_string()]);
    let image_regex = create_image_regex();

    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(content.to_string())
        .with_aliases(aliases.as_ref().unwrap_or(&Vec::new()).clone())
        .create(&temp_dir, "test file.md");

    let (extracted, image_links) = process_content(
        content,
        &aliases,
        &file_path,
        &image_regex
    ).unwrap();

    // Verify expected wikilinks
    assert_contains_wikilink(&extracted.valid, "test file", None, false);
    assert_contains_wikilink(&extracted.valid, "test file", Some("Alias One"), true);
    assert_contains_wikilink(&extracted.valid, "test file", Some("Alias Two"), true);
    assert_contains_wikilink(&extracted.valid, "Regular Link", None, false);
    assert_contains_wikilink(&extracted.valid, "Target", Some("Display Text"), true);

    // Verify no invalid wikilinks in this case
    assert!(extracted.invalid.is_empty(), "Should not have invalid wikilinks");

    // Verify no image links in this case
    assert!(image_links.is_empty(), "Should not have image links");
}

#[test]
fn test_process_content_with_invalid() {
    let content = "Some [[good link]] and [[bad|link|extra]] here\n[[unmatched";
    let image_regex = create_image_regex();

    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(content.to_string())
        .create(&temp_dir, "test.md");

    let (extracted, image_links) = process_content(
        content,
        &None,
        &file_path,
        &image_regex
    ).unwrap();

    // Check valid wikilinks
    assert_contains_wikilink(&extracted.valid, "test", None, false); // filename
    assert_contains_wikilink(&extracted.valid, "good link", None, false);

    // Verify invalid wikilinks with line information
    assert_eq!(extracted.invalid.len(), 2, "Should have exactly two invalid wikilinks");

    // Find and verify the double alias invalid wikilink
    let double_alias = extracted.invalid.iter()
        .find(|w| w.reason == InvalidWikilinkReason::DoubleAlias)
        .expect("Should have a double alias invalid wikilink");

    assert_eq!(double_alias.line_number, 1);
    assert_eq!(double_alias.line, "Some [[good link]] and [[bad|link|extra]] here");
    assert_eq!(double_alias.content, "[[bad|link|extra]]");

    // Find and verify the unmatched opening invalid wikilink
    let unmatched = extracted.invalid.iter()
        .find(|w| w.reason == InvalidWikilinkReason::UnmatchedOpening)
        .expect("Should have an unmatched opening invalid wikilink");

    assert_eq!(unmatched.line_number, 2);
    assert_eq!(unmatched.line, "[[unmatched");
    assert_eq!(unmatched.content, "[[unmatched");

    // Verify no image links
    assert!(image_links.is_empty(), "Should not have image links");
}

#[test]
fn test_process_content_with_empty() {
    let content = "Test [[]] here\nAnd [[|]] there";
    let image_regex = create_image_regex();

    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(content.to_string())
        .create(&temp_dir, "test.md");

    let (extracted, image_links) = process_content(
        content,
        &None,
        &file_path,
        &image_regex
    ).unwrap();

    assert_eq!(extracted.invalid.len(), 2, "Should have two invalid empty wikilinks");

    // Verify first empty wikilink
    let first_empty = &extracted.invalid[0];
    assert_eq!(first_empty.line_number, 1);
    assert_eq!(first_empty.line, "Test [[]] here");
    assert_eq!(first_empty.content, "[[]]");
    assert_eq!(first_empty.reason, InvalidWikilinkReason::EmptyWikilink);

    // Verify second empty wikilink
    let second_empty = &extracted.invalid[1];
    assert_eq!(second_empty.line_number, 2);
    assert_eq!(second_empty.line, "And [[|]] there");
    assert_eq!(second_empty.content, "[[|]]");
    assert_eq!(second_empty.reason, InvalidWikilinkReason::EmptyWikilink);

    // Verify no image links
    assert!(image_links.is_empty(), "Should not have image links");
}

#[test]
fn test_process_content_with_images() {
    let content = "# Test\n![[image.png]]\nHere's a [[link]] and ![alt text](another.jpg)";
    let image_regex = create_image_regex();

    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(content.to_string())
        .create(&temp_dir, "test.md");

    let (extracted, image_links) = process_content(
        content,
        &None,
        &file_path,
        &image_regex
    ).unwrap();

    // Check wikilinks
    assert_contains_wikilink(&extracted.valid, "test", None, false);
    assert_contains_wikilink(&extracted.valid, "link", None, false);

    // Check image links
    assert_eq!(image_links.len(), 2, "Should have two image links");
    assert!(image_links.contains(&"![[image.png]]".to_string()));
    assert!(image_links.contains(&"![alt text](another.jpg)".to_string()));
}

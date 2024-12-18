use crate::markdown_file::{
    ImageLink, ImageLinkRendering, ImageLinkTarget, ImageLinkType, MarkdownFile,
};
use crate::test_utils::TestFileBuilder;
use crate::utils::IMAGE_REGEX;
use crate::wikilink::{InvalidWikilinkReason, Wikilink};
use tempfile::TempDir;

fn assert_contains_wikilink(
    wikilinks: &[Wikilink],
    target: &str,
    display: Option<&str>,
    is_alias: bool,
) {
    let exists = wikilinks.iter().any(|w| {
        w.target == target && w.display_text == display.unwrap_or(target) && w.is_alias() == is_alias
    });
    assert!(
        exists,
        "Expected wikilink with target '{}', display '{:?}', is_alias '{}'",
        target, display, is_alias
    );
}

#[test]
fn test_process_content_with_aliases() {
    let content = "# Test\nHere's a [[Regular Link]] and [[Target|Display Text]]";
    let aliases = Some(vec!["Alias One".to_string(), "Alias Two".to_string()]);

    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(content.to_string())
        .with_aliases(aliases.as_ref().unwrap_or(&Vec::new()).clone())
        .create(&temp_dir, "test file.md");

    let file_info = MarkdownFile::new(file_path, "UTC").unwrap();
    let extracted = file_info.process_wikilinks().unwrap();
    let image_links = file_info.process_image_links();

    // Verify expected wikilinks
    assert_contains_wikilink(&extracted.valid, "test file", None, false);
    assert_contains_wikilink(&extracted.valid, "test file", Some("Alias One"), true);
    assert_contains_wikilink(&extracted.valid, "test file", Some("Alias Two"), true);
    assert_contains_wikilink(&extracted.valid, "Regular Link", None, false);
    assert_contains_wikilink(&extracted.valid, "Target", Some("Display Text"), true);

    // Verify no invalid wikilinks in this case
    assert!(
        extracted.invalid.is_empty(),
        "Should not have invalid wikilinks"
    );

    // Verify no image links in this case
    assert!(image_links.is_empty(), "Should not have image links");
}

#[test]
fn test_process_content_with_invalid() {
    let content = "Some [[good link]] and [[bad|link|extra]] here\n[[unmatched";

    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(content.to_string())
        .create(&temp_dir, "test.md");

    let file_info = MarkdownFile::new(file_path, "UTC").unwrap();
    let extracted = file_info.process_wikilinks().unwrap();
    let image_links = file_info.process_image_links();

    // Check valid wikilinks
    assert_contains_wikilink(&extracted.valid, "test", None, false); // filename
    assert_contains_wikilink(&extracted.valid, "good link", None, false);

    // Verify invalid wikilinks with line information
    assert_eq!(
        extracted.invalid.len(),
        2,
        "Should have exactly two invalid wikilinks"
    );

    // Find and verify the double alias invalid wikilink
    let double_alias = extracted
        .invalid
        .iter()
        .find(|w| w.reason == InvalidWikilinkReason::DoubleAlias)
        .expect("Should have a double alias invalid wikilink");

    assert_eq!(double_alias.line_number, 1);
    assert_eq!(
        double_alias.line,
        "Some [[good link]] and [[bad|link|extra]] here"
    );
    assert_eq!(double_alias.content, "[[bad|link|extra]]");

    // Find and verify the unmatched opening invalid wikilink
    let unmatched = extracted
        .invalid
        .iter()
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

    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(content.to_string())
        .create(&temp_dir, "test.md");

    let file_info = MarkdownFile::new(file_path, "UTC").unwrap();
    let extracted = file_info.process_wikilinks().unwrap();
    let image_links = file_info.process_image_links();

    assert_eq!(
        extracted.invalid.len(),
        2,
        "Should have two invalid empty wikilinks"
    );

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
    let content = "# Test\n![[image.png]]\nHere's a [[link]] and ![[another.jpg]]";

    let temp_dir = TempDir::new().unwrap();
    let file_path = TestFileBuilder::new()
        .with_content(content.to_string())
        .create(&temp_dir, "test.md");

    let file_info = MarkdownFile::new(file_path, "UTC").unwrap();
    let extracted = file_info.process_wikilinks().unwrap();
    let image_links = file_info.process_image_links();

    // Check wikilinks
    assert_contains_wikilink(&extracted.valid, "test", None, false);
    assert_contains_wikilink(&extracted.valid, "link", None, false);

    // Check image links
    assert_eq!(image_links.len(), 2, "Should have two image links");
    // assert!(image_links
    //     .iter()
    //     .any(|link| link.raw_link == "![[image.png]]"));
    // assert!(image_links
    //     .iter()
    //     .any(|link| link.raw_link == "![[another.jpg]]"));

    // Optionally, also test the filenames were extracted correctly
    assert!(image_links.iter().any(|link| link.filename == "image.png"));
    assert!(image_links
        .iter()
        .any(|link| link.filename == "another.jpg"));
}

#[derive(Debug)]
struct ImageLinkTestCase {
    input: &'static str,
    expected_filename: &'static str,
    expected_type: ImageLinkType,
}

impl ImageLinkTestCase {
    const fn new(input: &'static str, filename: &'static str, link_type: ImageLinkType) -> Self {
        Self {
            input,
            expected_filename: filename,
            expected_type: link_type,
        }
    }
}

#[test]
fn test_image_link_types() {
    let test_cases = [
        // Wikilinks
        ImageLinkTestCase::new(
            "![[image.png]]",
            "image.png",
            ImageLinkType::Wikilink(ImageLinkRendering::Embedded),
        ),
        ImageLinkTestCase::new(
            "[[image.jpg]]",
            "image.jpg",
            ImageLinkType::Wikilink(ImageLinkRendering::LinkOnly),
        ),
        ImageLinkTestCase::new(
            "![[image.png|alt text]]",
            "image.png",
            ImageLinkType::Wikilink(ImageLinkRendering::Embedded),
        ),
        // Markdown Internal Links
        ImageLinkTestCase::new(
            "![alt](image.png)",
            "image.png",
            ImageLinkType::MarkdownLink(ImageLinkTarget::Internal, ImageLinkRendering::Embedded),
        ),
        ImageLinkTestCase::new(
            "[alt](image.jpg)",
            "image.jpg",
            ImageLinkType::MarkdownLink(ImageLinkTarget::Internal, ImageLinkRendering::LinkOnly),
        ),
        // Markdown External Links
        ImageLinkTestCase::new(
            "![alt](https://example.com/image.png)",
            "https://example.com/image.png",
            ImageLinkType::MarkdownLink(ImageLinkTarget::External, ImageLinkRendering::Embedded),
        ),
        ImageLinkTestCase::new(
            "[alt](https://example.com/image.jpg)",
            "https://example.com/image.jpg",
            ImageLinkType::MarkdownLink(ImageLinkTarget::External, ImageLinkRendering::LinkOnly),
        ),
    ];

    for case in test_cases.iter() {
        let captures = IMAGE_REGEX
            .captures(case.input)
            .unwrap_or_else(|| panic!("Regex failed to match valid image link: {}", case.input));

        let raw_image_link = captures
            .get(0)
            .unwrap_or_else(|| panic!("Failed to get capture group for: {}", case.input))
            .as_str();

        // Add line number 1 and position 0 as test defaults
        let image_link = ImageLink::new(raw_image_link.to_string(), 1, 0);

        assert_eq!(
            image_link.filename, case.expected_filename,
            "Filename mismatch for input: {}",
            case.input
        );
        assert_eq!(
            image_link.image_link_type, case.expected_type,
            "ImageLinkType mismatch for input: {}",
            case.input
        );
    }
}

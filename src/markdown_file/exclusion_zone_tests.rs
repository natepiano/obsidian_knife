use crate::markdown_file::back_populate_tests::create_test_environment;
use crate::wikilink::{InvalidWikilink, InvalidWikilinkReason};

#[test]
fn test_collect_exclusion_zones_with_invalid_wikilinks() {
    let (_, config, mut repo_info) = create_test_environment(
        false,
        None,
        None,
        Some("Text [[invalid|link|extra]] and more text"),
    );

    let file_info = repo_info.markdown_files.first_mut().unwrap();

    // Add an invalid wikilink
    file_info.wikilinks.invalid.push(InvalidWikilink {
        content: "[[invalid|link|extra]]".to_string(),
        reason: InvalidWikilinkReason::DoubleAlias,
        span: (5, 27),
        line: "Text [[invalid|link|extra]] and more text".to_string(),
        line_number: 1,
    });

    let zones =
        file_info.collect_exclusion_zones("Text [[invalid|link|extra]] and more text", &config);

    assert!(!zones.is_empty(), "Should have at least one exclusion zone");
    assert!(
        zones.contains(&(5, 27)),
        "Should contain invalid wikilink span"
    );
}

#[test]
fn test_exclusion_zones_with_multiple_invalid_wikilinks() {
    let (_, config, mut repo_info) = create_test_environment(false, None, None, None);

    let markdown_file_info = repo_info.markdown_files.first_mut().unwrap();

    // Add multiple invalid wikilinks
    markdown_file_info.wikilinks.invalid.extend(vec![
        InvalidWikilink {
            content: "[[test|one|two]]".to_string(),
            reason: InvalidWikilinkReason::DoubleAlias,
            span: (0, 16),
            line: "[[test|one|two]] some text [[]]".to_string(),
            line_number: 1,
        },
        InvalidWikilink {
            content: "[[]]".to_string(),
            reason: InvalidWikilinkReason::EmptyWikilink,
            span: (27, 31),
            line: "[[test|one|two]] some text [[]]".to_string(),
            line_number: 1,
        },
    ]);

    let zones =
        markdown_file_info.collect_exclusion_zones("[[test|one|two]] some text [[]]", &config);

    assert_eq!(zones.len(), 2, "Should have two exclusion zones");
    assert!(
        zones.contains(&(0, 16)),
        "Should contain first invalid wikilink span"
    );
    assert!(
        zones.contains(&(27, 31)),
        "Should contain second invalid wikilink span"
    );
}

#[test]
fn test_exclusion_zones_only_matches_current_line() {
    let (_, config, mut repo_info) = create_test_environment(
        false,
        None,
        None,
        Some("Line 1 with [[bad|link|here]]\nLine 2 with normal text"),
    );

    let markdown_file_info = repo_info.markdown_files.first_mut().unwrap();

    // Add invalid wikilink from a different line
    markdown_file_info.wikilinks.invalid.push(InvalidWikilink {
        content: "[[bad|link|here]]".to_string(),
        reason: InvalidWikilinkReason::DoubleAlias,
        span: (10, 26),
        line: "Line 1 with [[bad|link|here]]".to_string(),
        line_number: 1,
    });

    // Check exclusion zones for line2
    let zones = markdown_file_info.collect_exclusion_zones("Line 2 with normal text", &config);

    assert!(
        zones.is_empty(),
        "Should not have exclusion zones for different line"
    );
}

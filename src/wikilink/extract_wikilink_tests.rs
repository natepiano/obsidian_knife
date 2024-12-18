use super::*;
use crate::wikilink::InvalidWikilinkReason;

struct WikilinkTestCase {
    description: &'static str,
    input: &'static str,
    expected_valid: Vec<(&'static str, &'static str, bool)>, // (target, display, is_alias)
    expected_invalid: Vec<(&'static str, InvalidWikilinkReason, (usize, usize))>, // (content, reason, span)
}

fn assert_wikilink_extraction(test_case: WikilinkTestCase) {
    let extracted = extract_wikilinks(test_case.input);

    // Verify valid wikilinks
    assert_eq!(
        extracted.valid.len(),
        test_case.expected_valid.len(),
        "Mismatch in number of valid wikilinks for: {}",
        test_case.description
    );

    for ((target, display, is_alias), wikilink) in
        test_case.expected_valid.iter().zip(extracted.valid.iter())
    {
        assert_eq!(
            wikilink.target, *target,
            "Target mismatch in {}",
            test_case.description
        );
        assert_eq!(
            wikilink.display_text, *display,
            "Display text mismatch in {}",
            test_case.description
        );
        assert_eq!(
            wikilink.is_alias(), *is_alias,
            "Alias flag mismatch in {}",
            test_case.description
        );
    }

    // Verify invalid wikilinks
    assert_eq!(
        extracted.invalid.len(),
        test_case.expected_invalid.len(),
        "Mismatch in number of invalid wikilinks for: {}",
        test_case.description
    );

    for ((content, reason, span), invalid) in test_case
        .expected_invalid
        .iter()
        .zip(extracted.invalid.iter())
    {
        assert_eq!(
            invalid.content, *content,
            "Content mismatch in {}",
            test_case.description
        );
        assert_eq!(
            invalid.reason, *reason,
            "Reason mismatch in {}",
            test_case.description
        );
        assert_eq!(
            invalid.span, *span,
            "Span mismatch in {}",
            test_case.description
        );
    }
}

#[test]
fn test_various_extractions() {
    let test_cases = vec![
        WikilinkTestCase {
            description: "Double alias with closing brackets",
            input: "Text with [[target|alias|extra]] here",
            expected_valid: vec![],
            expected_invalid: vec![(
                "[[target|alias|extra]]",
                InvalidWikilinkReason::DoubleAlias,
                (10, 32),
            )],
        },
        WikilinkTestCase {
            description: "Double alias without closing",
            input: "Text with [[target|alias|extra",
            expected_valid: vec![],
            expected_invalid: vec![(
                "[[target|alias|extra",
                InvalidWikilinkReason::UnmatchedOpening,
                (10, 30),
            )],
        },
        WikilinkTestCase {
            description: "Unmatched closing bracket within wikilink",
            input: "Text with [[test]text]] here",
            expected_valid: vec![],
            expected_invalid: vec![(
                "[[test]text]]",
                InvalidWikilinkReason::UnmatchedSingleInWikilink,
                (10, 23),
            )],
        },
        WikilinkTestCase {
            description: "Unmatched opening bracket within wikilink",
            input: "Text with [[test[text]] here",
            expected_valid: vec![],
            expected_invalid: vec![(
                "[[test[text]]",
                InvalidWikilinkReason::UnmatchedSingleInWikilink,
                (10, 23),
            )],
        },
        WikilinkTestCase {
            description: "Nested wikilink",
            input: "Text with [[target[[inner]] here",
            expected_valid: vec![],
            expected_invalid: vec![(
                "[[target[[inner]]",
                InvalidWikilinkReason::NestedOpening,
                (10, 27),
            )],
        },
    ];

    for test_case in test_cases {
        assert_wikilink_extraction(test_case);
    }
}

#[test]
fn test_unmatched_brackets() {
    let test_cases = vec![
        WikilinkTestCase {
            description: "Single unmatched closing brackets",
            input: "Some text here]] more text",
            expected_valid: vec![],
            expected_invalid: vec![(
                "Some text here]]",
                InvalidWikilinkReason::UnmatchedClosing,
                (0, 16),
            )],
        },
        WikilinkTestCase {
            description: "Multiple unmatched closings",
            input: "Text]] more]] text",
            expected_valid: vec![],
            expected_invalid: vec![
                ("Text]]", InvalidWikilinkReason::UnmatchedClosing, (0, 6)),
                (" more]]", InvalidWikilinkReason::UnmatchedClosing, (6, 13)),
            ],
        },
        WikilinkTestCase {
            description: "Mixed valid and invalid brackets",
            input: "[[Valid Link]] but here]] and [[Another]]",
            expected_valid: vec![
                ("Valid Link", "Valid Link", false),
                ("Another", "Another", false),
            ],
            expected_invalid: vec![(
                " but here]]",
                InvalidWikilinkReason::UnmatchedClosing,
                (14, 25),
            )],
        },
        // New Test Case for Unmatched Opening
        WikilinkTestCase {
            description: "Unmatched opening brackets at the end",
            input: "Here is an [[unmatched link",
            expected_valid: vec![],
            expected_invalid: vec![(
                "[[unmatched link",
                InvalidWikilinkReason::UnmatchedOpening,
                (11, 27),
            )],
        },
        WikilinkTestCase {
            description: "No wikilinks",
            input: "This is a plain text without any wikilinks.",
            expected_valid: vec![],
            expected_invalid: vec![],
        },
    ];

    for test_case in test_cases {
        assert_wikilink_extraction(test_case);
    }
}

#[test]
fn test_unclosed_markdown_links() {
    let test_cases = vec![
        WikilinkTestCase {
            description: "Basic unclosed markdown link",
            input: "[display",
            expected_valid: vec![],
            expected_invalid: vec![(
                "[display",
                InvalidWikilinkReason::UnmatchedMarkdownOpening,
                (0, 8),
            )],
        },
        WikilinkTestCase {
            description: "Unclosed link in context",
            input: "some text [link",
            expected_valid: vec![],
            expected_invalid: vec![(
                "[link",
                InvalidWikilinkReason::UnmatchedMarkdownOpening,
                (10, 15),
            )],
        },
        WikilinkTestCase {
            description: "Mixed valid wikilink and unclosed markdown",
            input: "[[valid link]] [unclosed",
            expected_valid: vec![("valid link", "valid link", false)],
            expected_invalid: vec![(
                "[unclosed",
                InvalidWikilinkReason::UnmatchedMarkdownOpening,
                (15, 24),
            )],
        },
        WikilinkTestCase {
            description: "Multiple unclosed markdown links",
            input: "[first [second",
            expected_valid: vec![],
            expected_invalid: vec![
                (
                    "[first",
                    InvalidWikilinkReason::UnmatchedMarkdownOpening,
                    (0, 6),
                ),
                (
                    "[second",
                    InvalidWikilinkReason::UnmatchedMarkdownOpening,
                    (7, 14),
                ),
            ],
        },
        WikilinkTestCase {
            description: "Escaped brackets should not trigger",
            input: "\\[not a link",
            expected_valid: vec![],
            expected_invalid: vec![],
        },
        WikilinkTestCase {
            description: "Valid markdown link followed by unclosed",
            input: "[valid](link) [unclosed",
            expected_valid: vec![],
            expected_invalid: vec![
                (
                    "[unclosed",
                    InvalidWikilinkReason::UnmatchedMarkdownOpening,
                    (14, 23),
                ), // Fixed span
            ],
        },
    ];

    for test_case in test_cases {
        assert_wikilink_extraction(test_case);
    }
}

#[test]
fn test_email_detection() {
    let test_cases = vec![
        WikilinkTestCase {
            description: "Simple email address",
            input: "Contact bob@example.com for more info",
            expected_valid: vec![],
            expected_invalid: vec![(
                "bob@example.com",
                InvalidWikilinkReason::EmailAddress,
                (8, 23),
            )],
        },
        WikilinkTestCase {
            description: "Email with wikilink",
            input: "[[Contact]] john.doe@company.org today",
            expected_valid: vec![("Contact", "Contact", false)],
            expected_invalid: vec![(
                "john.doe@company.org",
                InvalidWikilinkReason::EmailAddress,
                (12, 32),
            )],
        },
        WikilinkTestCase {
            description: "Multiple emails",
            input: "Email user1@test.com or user2@test.com",
            expected_valid: vec![],
            expected_invalid: vec![
                (
                    "user1@test.com",
                    InvalidWikilinkReason::EmailAddress,
                    (6, 20),
                ),
                (
                    "user2@test.com",
                    InvalidWikilinkReason::EmailAddress,
                    (24, 38),
                ),
            ],
        },
    ];

    for test_case in test_cases {
        assert_wikilink_extraction(test_case);
    }
}

#[test]
fn test_tag_detection() {
    let test_cases = vec![
        WikilinkTestCase {
            description: "Simple tag at start of line",
            input: "#obsidian_knife is great",
            expected_valid: vec![],
            expected_invalid: vec![("#obsidian_knife", InvalidWikilinkReason::Tag, (0, 15))],
        },
        WikilinkTestCase {
            description: "Tag after space",
            input: "Check out this #ka-fave tag",
            expected_valid: vec![],
            expected_invalid: vec![("#ka-fave", InvalidWikilinkReason::Tag, (14, 23))],
        },
        WikilinkTestCase {
            description: "Multiple tags",
            input: "#tag1 some text #tag2",
            expected_valid: vec![],
            expected_invalid: vec![
                ("#tag1", InvalidWikilinkReason::Tag, (0, 5)),
                ("#tag2", InvalidWikilinkReason::Tag, (15, 21)),
            ],
        },
        WikilinkTestCase {
            description: "Tag with wikilink",
            input: "[[Note]] #important reference",
            expected_valid: vec![("Note", "Note", false)],
            expected_invalid: vec![("#important", InvalidWikilinkReason::Tag, (8, 19))],
        },
        WikilinkTestCase {
            description: "Tag with underscore and numbers",
            input: "Task #two_do_123 pending",
            expected_valid: vec![],
            expected_invalid: vec![("#two_do_123", InvalidWikilinkReason::Tag, (4, 16))],
        },
    ];

    for test_case in test_cases {
        assert_wikilink_extraction(test_case);
    }
}

#[test]
fn test_raw_http_detection() {
    let test_cases = vec![
        WikilinkTestCase {
            description: "link at start of line",
            input: "https://google.com/ is blah",
            expected_valid: vec![],
            expected_invalid: vec![(
                "https://google.com/",
                InvalidWikilinkReason::RawHttpLink,
                (0, 19),
            )],
        },
        WikilinkTestCase {
            description: "link after space",
            input: "Check out this https://google.com/ link",
            expected_valid: vec![],
            expected_invalid: vec![(
                "https://google.com/",
                InvalidWikilinkReason::RawHttpLink,
                (15, 34),
            )],
        },
        WikilinkTestCase {
            description: "Multiple links",
            input: "http://this.com/ some text http://that.com/",
            expected_valid: vec![],
            expected_invalid: vec![
                (
                    "http://this.com/",
                    InvalidWikilinkReason::RawHttpLink,
                    (0, 16),
                ),
                (
                    "http://that.com/",
                    InvalidWikilinkReason::RawHttpLink,
                    (27, 43),
                ),
            ],
        },
    ];

    for test_case in test_cases {
        assert_wikilink_extraction(test_case);
    }
}

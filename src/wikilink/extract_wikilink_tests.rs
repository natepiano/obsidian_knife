use super::InvalidWikilinkReason;
use super::*;
use crate::test_support::AliasExpectation;

struct WikilinkTestCase {
    description: &'static str,
    input:       &'static str,
    valid:       Vec<(&'static str, &'static str, AliasExpectation)>,
    invalid:     Vec<(&'static str, InvalidWikilinkReason, (usize, usize))>,
}

fn assert_wikilink_extraction(test_case: &WikilinkTestCase) {
    let extracted = extract_wikilinks(test_case.input);

    // Verify valid wikilinks
    assert_eq!(
        extracted.valid.len(),
        test_case.valid.len(),
        "Mismatch in number of valid wikilinks for: {}",
        test_case.description
    );

    for ((target, display, alias_expectation), wikilink) in
        test_case.valid.iter().zip(extracted.valid.iter())
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
            wikilink.is_alias(),
            alias_expectation.is_alias(),
            "Alias flag mismatch in {}",
            test_case.description
        );
    }

    // Verify invalid wikilinks
    assert_eq!(
        extracted.invalid.len(),
        test_case.invalid.len(),
        "Mismatch in number of invalid wikilinks for: {}",
        test_case.description
    );

    for ((content, reason, span), invalid) in test_case.invalid.iter().zip(extracted.invalid.iter())
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
            input:       "Text with [[target|alias|extra]] here",
            valid:       vec![],
            invalid:     vec![(
                "[[target|alias|extra]]",
                InvalidWikilinkReason::DoubleAlias,
                (10, 32),
            )],
        },
        WikilinkTestCase {
            description: "Double alias without closing",
            input:       "Text with [[target|alias|extra",
            valid:       vec![],
            invalid:     vec![(
                "[[target|alias|extra",
                InvalidWikilinkReason::UnmatchedOpening,
                (10, 30),
            )],
        },
        WikilinkTestCase {
            description: "Unmatched closing bracket within wikilink",
            input:       "Text with [[test]text]] here",
            valid:       vec![],
            invalid:     vec![(
                "[[test]text]]",
                InvalidWikilinkReason::UnmatchedSingle,
                (10, 23),
            )],
        },
        WikilinkTestCase {
            description: "Unmatched opening bracket within wikilink",
            input:       "Text with [[test[text]] here",
            valid:       vec![],
            invalid:     vec![(
                "[[test[text]]",
                InvalidWikilinkReason::UnmatchedSingle,
                (10, 23),
            )],
        },
        WikilinkTestCase {
            description: "Nested wikilink",
            input:       "Text with [[target[[inner]] here",
            valid:       vec![],
            invalid:     vec![(
                "[[target[[inner]]",
                InvalidWikilinkReason::NestedOpening,
                (10, 27),
            )],
        },
        WikilinkTestCase {
            description: "Aliased wikilink",
            input:       "Text with [[target|alias]] here",
            valid:       vec![("target", "alias", AliasExpectation::Aliased)],
            invalid:     vec![],
        },
    ];

    for test_case in test_cases {
        assert_wikilink_extraction(&test_case);
    }
}

#[test]
fn test_unmatched_brackets() {
    let test_cases = vec![
        WikilinkTestCase {
            description: "Single unmatched closing brackets",
            input:       "Some text here]] more text",
            valid:       vec![],
            invalid:     vec![(
                "Some text here]]",
                InvalidWikilinkReason::UnmatchedClosing,
                (0, 16),
            )],
        },
        WikilinkTestCase {
            description: "Multiple unmatched closings",
            input:       "Text]] more]] text",
            valid:       vec![],
            invalid:     vec![
                ("Text]]", InvalidWikilinkReason::UnmatchedClosing, (0, 6)),
                (" more]]", InvalidWikilinkReason::UnmatchedClosing, (6, 13)),
            ],
        },
        WikilinkTestCase {
            description: "Mixed valid and invalid brackets",
            input:       "[[Valid Link]] but here]] and [[Another]]",
            valid:       vec![
                ("Valid Link", "Valid Link", AliasExpectation::DirectLink),
                ("Another", "Another", AliasExpectation::DirectLink),
            ],
            invalid:     vec![(
                " but here]]",
                InvalidWikilinkReason::UnmatchedClosing,
                (14, 25),
            )],
        },
        // New Test Case for Unmatched Opening
        WikilinkTestCase {
            description: "Unmatched opening brackets at the end",
            input:       "Here is an [[unmatched link",
            valid:       vec![],
            invalid:     vec![(
                "[[unmatched link",
                InvalidWikilinkReason::UnmatchedOpening,
                (11, 27),
            )],
        },
        WikilinkTestCase {
            description: "No wikilinks",
            input:       "This is a plain text without any wikilinks.",
            valid:       vec![],
            invalid:     vec![],
        },
    ];

    for test_case in test_cases {
        assert_wikilink_extraction(&test_case);
    }
}

#[test]
fn test_unclosed_markdown_links() {
    let test_cases = vec![
        WikilinkTestCase {
            description: "Basic unclosed markdown link",
            input:       "[display",
            valid:       vec![],
            invalid:     vec![(
                "[display",
                InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
                (0, 8),
            )],
        },
        WikilinkTestCase {
            description: "Unclosed link in context",
            input:       "some text [link",
            valid:       vec![],
            invalid:     vec![(
                "[link",
                InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
                (10, 15),
            )],
        },
        WikilinkTestCase {
            description: "Mixed valid wikilink and unclosed markdown",
            input:       "[[valid link]] [unclosed",
            valid:       vec![("valid link", "valid link", AliasExpectation::DirectLink)],
            invalid:     vec![(
                "[unclosed",
                InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
                (15, 24),
            )],
        },
        WikilinkTestCase {
            description: "Multiple unclosed markdown links",
            input:       "[first [second",
            valid:       vec![],
            invalid:     vec![
                (
                    "[first",
                    InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
                    (0, 6),
                ),
                (
                    "[second",
                    InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
                    (7, 14),
                ),
            ],
        },
        WikilinkTestCase {
            description: "Escaped brackets should not trigger",
            input:       "\\[not a link",
            valid:       vec![],
            invalid:     vec![],
        },
        WikilinkTestCase {
            description: "Valid markdown link followed by unclosed",
            input:       "[valid](link) [unclosed",
            valid:       vec![],
            invalid:     vec![
                (
                    "[unclosed",
                    InvalidWikilinkReason::UnmatchedMarkdownLinkOpening,
                    (14, 23),
                ), // Fixed span
            ],
        },
    ];

    for test_case in test_cases {
        assert_wikilink_extraction(&test_case);
    }
}

#[test]
fn test_markdown_clickable_image_not_flagged() {
    // CI badge: [![alt](image-url)](link-url) is valid markdown, not an invalid wikilink
    let test_cases = vec![
        WikilinkTestCase {
            description: "CI badge should not produce UnmatchedMarkdownLinkOpening",
            input:       "[![CI](https://example.com/badge.svg)](https://example.com/ci)",
            valid:       vec![],
            invalid:     vec![(
                "https://example.com/badge.svg)](https://example.com/ci)",
                InvalidWikilinkReason::RawHttpLink,
                (7, 62),
            )],
        },
        WikilinkTestCase {
            description: "Badge with surrounding text",
            input:       "Check out [![Build](https://example.com/b.svg)](https://example.com) the project",
            valid:       vec![],
            invalid:     vec![(
                "https://example.com/b.svg)](https://example.com)",
                InvalidWikilinkReason::RawHttpLink,
                (20, 68),
            )],
        },
    ];

    for test_case in test_cases {
        assert_wikilink_extraction(&test_case);
    }
}

#[test]
fn test_email_detection() {
    let test_cases = vec![
        WikilinkTestCase {
            description: "Simple email address",
            input:       "Contact bob@example.com for more info",
            valid:       vec![],
            invalid:     vec![(
                "bob@example.com",
                InvalidWikilinkReason::EmailAddress,
                (8, 23),
            )],
        },
        WikilinkTestCase {
            description: "Email with wikilink",
            input:       "[[Contact]] john.doe@company.org today",
            valid:       vec![("Contact", "Contact", AliasExpectation::DirectLink)],
            invalid:     vec![(
                "john.doe@company.org",
                InvalidWikilinkReason::EmailAddress,
                (12, 32),
            )],
        },
        WikilinkTestCase {
            description: "Multiple emails",
            input:       "Email user1@test.com or user2@test.com",
            valid:       vec![],
            invalid:     vec![
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
        assert_wikilink_extraction(&test_case);
    }
}

#[test]
fn test_tag_detection() {
    let test_cases = vec![
        WikilinkTestCase {
            description: "Simple tag at start of line",
            input:       "#obsidian_knife is great",
            valid:       vec![],
            invalid:     vec![("#obsidian_knife", InvalidWikilinkReason::Tag, (0, 15))],
        },
        WikilinkTestCase {
            description: "Tag after space",
            input:       "Check out this #ka-fave tag",
            valid:       vec![],
            invalid:     vec![("#ka-fave", InvalidWikilinkReason::Tag, (14, 23))],
        },
        WikilinkTestCase {
            description: "Multiple tags",
            input:       "#tag1 some text #tag2",
            valid:       vec![],
            invalid:     vec![
                ("#tag1", InvalidWikilinkReason::Tag, (0, 5)),
                ("#tag2", InvalidWikilinkReason::Tag, (15, 21)),
            ],
        },
        WikilinkTestCase {
            description: "Tag with wikilink",
            input:       "[[Note]] #important reference",
            valid:       vec![("Note", "Note", AliasExpectation::DirectLink)],
            invalid:     vec![("#important", InvalidWikilinkReason::Tag, (8, 19))],
        },
        WikilinkTestCase {
            description: "Tag with underscore and numbers",
            input:       "Task #two_do_123 pending",
            valid:       vec![],
            invalid:     vec![("#two_do_123", InvalidWikilinkReason::Tag, (4, 16))],
        },
    ];

    for test_case in test_cases {
        assert_wikilink_extraction(&test_case);
    }
}

#[test]
fn test_raw_http_detection() {
    let test_cases = vec![
        WikilinkTestCase {
            description: "link at start of line",
            input:       "https://google.com/ is blah",
            valid:       vec![],
            invalid:     vec![(
                "https://google.com/",
                InvalidWikilinkReason::RawHttpLink,
                (0, 19),
            )],
        },
        WikilinkTestCase {
            description: "link after space",
            input:       "Check out this https://google.com/ link",
            valid:       vec![],
            invalid:     vec![(
                "https://google.com/",
                InvalidWikilinkReason::RawHttpLink,
                (15, 34),
            )],
        },
        WikilinkTestCase {
            description: "Multiple links",
            input:       "http://this.com/ some text http://that.com/",
            valid:       vec![],
            invalid:     vec![
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
        assert_wikilink_extraction(&test_case);
    }
}

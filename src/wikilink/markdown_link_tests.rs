use crate::regex_utils::MARKDOWN_REGEX;

#[test]
fn test_markdown_regex_matches() {
    let regex = MARKDOWN_REGEX.clone();

    let matching_cases = vec![
        "[text](https://example.com)",
        "[link](https://test.com)",
        "[page](folder/page.md)",
        "[img](../images/test.png)",
        "[text](path 'title')",
        "[text](path \"title\")",
        "[](path)",
        "[text]()",
        "[]()",
    ];

    for case in matching_cases {
        assert!(regex.is_match(case), "Regex should match '{}'", case);
    }

    let non_matching_cases = vec![
        "plain text",
        "[[wikilink]]",
        "![[imagelink]]",
        "[incomplete",
    ];

    for case in non_matching_cases {
        assert!(!regex.is_match(case), "Regex should not match '{}'", case);
    }
}

#[test]
fn test_markdown_link_extraction() {
    let regex = MARKDOWN_REGEX.clone();
    let text = "Here is [one](link1) and [two](link2) and normal text";

    let links: Vec<_> = regex.find_iter(text).map(|m| m.as_str()).collect();
    assert_eq!(links.len(), 2);
    assert_eq!(links[0], "[one](link1)");
    assert_eq!(links[1], "[two](link2)");
}

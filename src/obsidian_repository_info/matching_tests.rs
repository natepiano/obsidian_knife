use crate::obsidian_repository_info::back_populate_tests::create_test_environment;
use crate::obsidian_repository_info::is_within_wikilink;
use crate::wikilink_types::Wikilink;

#[test]
fn test_find_matches_with_existing_wikilinks() {
    let content = "[[Some Link]] and Test Link in same line\n\
           Test Link [[Other Link]] Test Link mixed\n\
           This don't match\n\
           This don't match either\n\
           But this Test Link should match";

    let (_temp_dir, config, mut repo_info) =
        create_test_environment(false, None, None, Some(content));

    // Find matches - this now stores them in repo_info.markdown_files
    repo_info.find_all_back_populate_matches(&config).unwrap();

    // Get all matches from the first (and only) file
    let matches = &repo_info.markdown_files[0].matches;

    // We expect 4 matches for "Test Link" outside existing wikilinks and contractions
    assert_eq!(matches.len(), 4, "Mismatch in number of matches");

    // Verify that the matches are at the expected positions
    let expected_lines = vec![1, 2, 2, 5];
    let actual_lines: Vec<usize> = matches.iter().map(|m| m.line_number).collect();
    assert_eq!(
        actual_lines, expected_lines,
        "Mismatch in line numbers of matches"
    );
}

#[test]
fn test_overlapping_wikilink_matches() {
    let content = "[[Kyriana McCoy|Kyriana]] - Kyri and [[Kalina McCoy|Kali]]";
    let wikilinks = vec![
        Wikilink {
            display_text: "Kyri".to_string(),
            target: "Kyri".to_string(),
            is_alias: false,
        },
        Wikilink {
            display_text: "Kyri".to_string(),
            target: "Kyriana McCoy".to_string(),
            is_alias: true,
        },
    ];

    let (_temp_dir, config, mut repo_info) =
        create_test_environment(false, None, Some(wikilinks), Some(content));

    // Find matches - this now stores them in repo_info.markdown_files
    repo_info.find_all_back_populate_matches(&config).unwrap();

    // Get matches from the first (and only) file
    let matches = &repo_info.markdown_files[0].matches;

    assert_eq!(matches.len(), 1, "Expected exactly one match");
    assert_eq!(matches[0].position, 28, "Expected match at position 28");
}

#[test]
fn test_is_within_wikilink() {
    let test_cases = vec![
        // ASCII cases
        ("before [[link]] after", 7, false),
        ("before [[link]] after", 8, false),
        ("before [[link]] after", 9, true),
        ("before [[link]] after", 10, true),
        ("before [[link]] after", 11, true),
        ("before [[link]] after", 12, true),
        ("before [[link]] after", 13, false),
        ("before [[link]] after", 14, false),
        // Unicode cases
        ("привет [[ссылка]] текст", 13, false),
        ("привет [[ссылка]] текст", 14, false),
        ("привет [[ссылка]] текст", 15, true),
        ("привет [[ссылка]] текст", 25, true),
        ("привет [[ссылка]] текст", 27, false),
        ("привет [[ссылка]] текст", 28, false),
        ("привет [[ссылка]] текст", 12, false),
        ("привет [[ссылка]] текст", 29, false),
    ];

    for (text, pos, expected) in test_cases {
        assert_eq!(
            is_within_wikilink(text, pos),
            expected,
            "Failed for text '{}' at position {}",
            text,
            pos
        );
    }
}

use crate::markdown_file::back_populate_tests;
use crate::markdown_file::{BackPopulateMatch, MarkdownFile};
use crate::obsidian_repository::ObsidianRepository;
use crate::test_utils::TestFileBuilder;
use crate::wikilink::Wikilink;

// Helper struct for test cases
struct TestCase {
    content: &'static str,
    wikilink: Wikilink,
    expected_matches: Vec<(&'static str, &'static str)>,
    description: &'static str,
}

fn get_case_sensitivity_test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            content: "test link TEST LINK Test Link",
            wikilink: Wikilink {
                display_text: "Test Link".to_string(),
                target: "Test Link".to_string(),
            },
            // careful - these must match the order returned by process_line
            expected_matches: vec![
                ("test link", "[[Test Link|test link]]"),
                ("TEST LINK", "[[Test Link|TEST LINK]]"),
                ("Test Link", "[[Test Link]]"),
            ],
            description: "Basic case-insensitive matching",
        },
        TestCase {
            content: "josh likes apples",
            wikilink: Wikilink {
                display_text: "josh".to_string(),
                target: "Joshua Strayhorn".to_string(),
            },
            expected_matches: vec![("josh", "[[Joshua Strayhorn|josh]]")],
            description: "Alias case preservation",
        },
        TestCase {
            content: "karen likes math",
            wikilink: Wikilink {
                display_text: "Karen".to_string(),
                target: "Karen McCoy".to_string(),
            },
            expected_matches: vec![("karen", "[[Karen McCoy|karen]]")],
            description: "Alias case preservation when display case differs from content",
        },
        TestCase {
            content: "| Test Link | Another test link |",
            wikilink: Wikilink {
                display_text: "Test Link".to_string(),
                target: "Test Link".to_string(),
            },
            expected_matches: vec![
                ("Test Link", "[[Test Link]]"),
                ("test link", "[[Test Link|test link]]"),
            ],
            description: "Case handling in tables",
        },
    ]
}

pub(crate) fn verify_match(
    actual_match: &BackPopulateMatch,
    expected_text: &str,
    expected_base_replacement: &str,
    case_description: &str,
) {
    assert_eq!(
        actual_match.found_text, expected_text,
        "Wrong matched text for case: {}",
        case_description
    );

    let expected_replacement = if actual_match.in_markdown_table {
        expected_base_replacement.replace('|', r"\|")
    } else {
        expected_base_replacement.to_string()
    };

    assert_eq!(
        actual_match.replacement,
        expected_replacement,
        "Wrong replacement for case: {}\nExpected: {}\nActual: {}\nIn table: {}",
        case_description,
        expected_replacement,
        actual_match.replacement,
        actual_match.in_markdown_table
    );
}

#[test]
fn test_case_insensitive_targets() {
    // Create test environment
    let (temp_dir, config, _) =
        back_populate_tests::create_test_environment(false, None, Some(vec![]), None);

    // Create test files with case variations using TestFileBuilder
    TestFileBuilder::new()
        .with_content("# Sample\nAmazon") // Changed to not use "Test" in content
        .with_title("Sample".to_string()) // Changed from "Test"
        .create(&temp_dir, "Amazon.md");

    TestFileBuilder::new()
        .with_content("# Sample Document\nAmazon is huge\namazon is also huge")
        .with_title("Test Document".to_string()) // This adds frontmatter with the title
        .create(&temp_dir, "test1.md");

    // Scan folders to populate repository
    let mut repository = ObsidianRepository::new(&config).unwrap();

    // Find our test file
    let test_file = repository
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("test1.md"))
        .expect("Should find test1.md");

    // Verify we found both case variations initially
    assert_eq!(
        test_file.matches.unambiguous.len(),
        2,
        "Should have matches for both case variations"
    );

    // Get ambiguous matches
    repository.identify_ambiguous_matches();

    // Find our test file again after ambiguous matching
    let test_file = repository
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("test1.md"))
        .expect("Should find test1.md");

    // All matches should remain in the markdown file as unambiguous
    assert_eq!(
        test_file.matches.unambiguous.len(),
        2,
        "Both matches should be considered unambiguous"
    );
}

#[test]
fn test_case_sensitivity_behavior() {
    // Initialize test environment without specific wikilinks
    let (temp_dir, config, mut repository) =
        back_populate_tests::create_test_environment(false, None, None, None);

    for case in get_case_sensitivity_test_cases() {
        let file_path = back_populate_tests::create_markdown_test_file(
            &temp_dir,
            "test.md",
            case.content,
            &mut repository,
        );

        // Create a custom wikilink and build AC automaton directly
        let wikilink = case.wikilink;
        let ac = back_populate_tests::build_aho_corasick(&[wikilink.clone()]);

        let markdown_info =
            MarkdownFile::new(file_path.clone(), config.operational_timezone()).unwrap();

        let matches = markdown_info.process_line_for_back_populate_replacements(
            case.content,
            0,
            &ac,
            &[&wikilink],
            &config,
        );

        assert_eq!(
            matches.len(),
            case.expected_matches.len(),
            "Wrong number of matches for case: {}",
            case.description
        );

        for ((expected_text, expected_base_replacement), actual_match) in
            case.expected_matches.iter().zip(matches.iter())
        {
            verify_match(
                actual_match,
                expected_text,
                expected_base_replacement,
                case.description,
            );
        }
    }
}

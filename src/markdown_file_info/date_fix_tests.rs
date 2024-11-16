use super::*;
use crate::frontmatter::FrontMatter;
use crate::test_utils::assert_test_case;
use crate::wikilink::is_wikilink;
use crate::yaml_frontmatter::YamlFrontMatter;

fn create_frontmatter(
    date_modified: &Option<String>,
    needs_persist: bool,
    needs_filesystem_update: Option<String>,
) -> FrontMatter {
    // First create base YAML for deserialization
    let yaml = match date_modified {
        Some(date) => format!("---\ndate_modified: \"{}\"\n---\n", date), // Using double quotes like the actual file
        None => "---\ntitle: test\n---\n".to_string(),
    };

    // Deserialize from YAML
    let mut fm = FrontMatter::from_markdown_str(&yaml).unwrap();

    // Set the skipped fields that aren't part of YAML serialization
    fm.set_needs_persist(needs_persist);
    fm.set_needs_filesystem_update(needs_filesystem_update);

    fm
}

// Test case struct for date modification
struct DateModTestCase {
    name: &'static str,
    initial_date: Option<String>,
    expected_format: bool,
    should_persist: bool,
}

#[test]
fn test_process_date_modified() {
    let test_cases = vec![
        DateModTestCase {
            name: "no date_modified - should set today's date",
            initial_date: None,
            expected_format: true,
            should_persist: true,
        },
        DateModTestCase {
            name: "raw date - should convert to wikilink",
            initial_date: Some("2024-01-15".to_string()),
            expected_format: true,
            should_persist: true,
        },
        DateModTestCase {
            name: "already in wikilink format - no change needed",
            initial_date: Some("[[2024-01-15]]".to_string()),
            expected_format: true,
            should_persist: false,
        },
    ];

    for case in test_cases {
        let mut fm = create_frontmatter(&case.initial_date, false, None);

        // Get the result and update frontmatter
        let (new_date, needs_persist) = process_date_modified_helper(case.initial_date);
        if needs_persist {
            fm.update_date_modified(new_date);
            fm.set_needs_persist(true);
        }
        // Check if date is in correct format
        assert_test_case(
            fm.date_modified
                .as_ref()
                .map(|d| is_wikilink(Some(d)))
                .unwrap_or(false),
            case.expected_format,
            &format!("{} - date format", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        // Check if persistence flag is set correctly
        assert_test_case(
            fm.needs_persist(),
            case.should_persist,
            &format!("{} - persistence flag", case.name),
            |actual, expected| assert_eq!(actual, expected),
        );

        // If date was added/modified, verify it's a valid date
        if fm.date_modified.is_some() {
            let date_str = extract_date(fm.date_modified.as_ref().unwrap());
            assert_test_case(
                is_valid_date(date_str),
                true,
                &format!("{} - date validity", case.name),
                |actual, expected| assert_eq!(actual, expected),
            );
        }
    }
}

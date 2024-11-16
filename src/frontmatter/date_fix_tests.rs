use crate::frontmatter::{extract_date, is_valid_date, FrontMatter};
use crate::test_utils::assert_test_case;
use crate::wikilink::is_wikilink;

// Helper function to create FrontMatter with default values
fn create_frontmatter(
    date_modified: Option<String>,
    needs_persist: bool,
    needs_filesystem_update: Option<String>,
) -> FrontMatter {
    FrontMatter {
        aliases: None,
        date_created: None,
        date_created_fix: None,
        date_modified,
        do_not_back_populate: None,
        needs_persist,
        needs_filesystem_update,
        other_fields: Default::default(),
    }
}

// Test case struct for persistence flags
struct PersistenceFlagTestCase {
    name: &'static str,
    initial_state: bool,
    new_state: bool,
    expected_state: bool,
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
            expected_format: true,  // expect wikilink format
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
        let mut fm = create_frontmatter(case.initial_date, false, None);

        fm.process_date_modified();

        // Check if date is in correct format
        assert_test_case(
            fm.date_modified.as_ref().map(|d| is_wikilink(Some(d))).unwrap_or(false),
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

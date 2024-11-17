use crate::back_populate::back_populate_tests::create_test_environment;
use crate::back_populate::FileProcessingState;

#[test]
fn test_config_creation() {
    // Basic usage with defaults
    let (_, basic_config, _) = create_test_environment(false, None, None, None);
    assert!(!basic_config.apply_changes());

    // With apply_changes set to true
    let (_, apply_config, _) = create_test_environment(true, None, None, None);
    assert!(apply_config.apply_changes());

    // With do_not_back_populate patterns
    let patterns = vec!["pattern1".to_string(), "pattern2".to_string()];
    let (_, pattern_config, _) = create_test_environment(false, Some(patterns.clone()), None, None);
    assert_eq!(
        pattern_config.do_not_back_populate(),
        Some(patterns.as_slice())
    );

    // With both parameters
    let (_, full_config, _) =
        create_test_environment(true, Some(vec!["pattern".to_string()]), None, None);
    assert!(full_config.apply_changes());
    assert!(full_config.do_not_back_populate().is_some());
}

#[test]
fn test_file_processing_state() {
    let mut state = FileProcessingState::new();

    // Initial state
    assert!(!state.should_skip_line(), "Initial state should not skip");

    // Frontmatter
    state.update_for_line("---");
    assert!(state.should_skip_line(), "Should skip in frontmatter");
    state.update_for_line("title: Test");
    assert!(state.should_skip_line(), "Should skip frontmatter content");
    state.update_for_line("---");
    assert!(
        !state.should_skip_line(),
        "Should not skip after frontmatter"
    );

    // Code block
    state.update_for_line("```rust");
    assert!(state.should_skip_line(), "Should skip in code block");
    state.update_for_line("let x = 42;");
    assert!(state.should_skip_line(), "Should skip code block content");
    state.update_for_line("```");
    assert!(
        !state.should_skip_line(),
        "Should not skip after code block"
    );

    // Combined frontmatter and code block
    state.update_for_line("---");
    assert!(state.should_skip_line(), "Should skip in frontmatter again");
    state.update_for_line("description: complex");
    assert!(state.should_skip_line(), "Should skip frontmatter content");
    state.update_for_line("---");
    assert!(
        !state.should_skip_line(),
        "Should not skip after frontmatter"
    );

    state.update_for_line("```");
    assert!(
        state.should_skip_line(),
        "Should skip in another code block"
    );
    state.update_for_line("print('Hello')");
    assert!(state.should_skip_line(), "Should skip code block content");
    state.update_for_line("```");
    assert!(
        !state.should_skip_line(),
        "Should not skip after code block"
    );
}

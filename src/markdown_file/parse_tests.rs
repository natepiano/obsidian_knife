use crate::markdown_file::MarkdownFile;
use crate::test_utils::{get_test_markdown_file_info, TestFileBuilder};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[test]
fn test_parse_content_separation() {
    let temp_dir = TempDir::new().unwrap();

    // Test 1: File with frontmatter and content
    let file_with_fm = TestFileBuilder::new()
        .with_title("Test".to_string())
        .with_content("This is the actual content")
        .create(&temp_dir, "with_fm.md");

    let mfi = get_test_markdown_file_info(file_with_fm);
    assert_eq!(mfi.content.trim(), "This is the actual content");

    // Test 2: File with no frontmatter
    let file_no_fm = TestFileBuilder::new()
        .with_content("Pure content\nNo frontmatter")
        .create(&temp_dir, "no_fm.md");

    let mfi = get_test_markdown_file_info(file_no_fm);
    assert_eq!(mfi.content.trim(), "Pure content\nNo frontmatter");

    // Test 3: File with --- separators in content
    let content = "First line\n---\nMiddle section\n---\nLast section";
    let file_with_separators = TestFileBuilder::new()
        .with_title("Test".to_string())
        .with_content(content)
        .create(&temp_dir, "with_separators.md");

    let mfi = get_test_markdown_file_info(file_with_separators);
    assert_eq!(mfi.content.trim(), content);
}

fn create_test_file(content: &str, temp_dir: &Path) -> PathBuf {
    let file_path = temp_dir.join("test.md");
    fs::write(&file_path, content).unwrap();
    file_path
}

#[test]
fn test_frontmatter_line_counting() {
    let temp_dir = TempDir::new().unwrap();

    let test_cases = vec![
        (
            "---\ntitle: test\n---\nContent",
            3, // 1 line of YAML + 2 delimiters = 3
        ),
        (
            "---\ntitle: test\ntags: [a,b]\n---\nContent",
            4, // 2 lines of YAML + 2 delimiters = 4
        ),
        (
            "---\ntitle: test\ntags:\n  - a\n  - b\n---\nContent",
            6, // 4 lines of YAML + 2 delimiters = 6
        ),
    ];

    for (content, expected_frontmatter_lines) in test_cases {
        let file_path = create_test_file(content, temp_dir.path());
        let markdown_file = MarkdownFile::new(file_path, "UTC").unwrap();
        assert_eq!(
            markdown_file.frontmatter_line_count, expected_frontmatter_lines,
            "Failed for content:\n{}",
            content
        );
    }
}

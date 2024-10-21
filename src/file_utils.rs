use chrono::Local;
use regex::Regex;
use std::error::Error;
use std::fs;
use std::path::Path;

pub fn update_file<P: AsRef<Path>>(
    path: P,
    update_fn: impl FnOnce(&str) -> String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let path = path.as_ref();
    let content = fs::read_to_string(path)?;
    let updated_content = if path.extension().and_then(|s| s.to_str()) == Some("md") {
        update_markdown_content(&content, update_fn)
    } else {
        update_fn(&content)
    };
    fs::write(path, updated_content)?;
    Ok(())
}

fn update_markdown_content(
    content: &str,
    update_fn: impl FnOnce(&str) -> String,
) -> String {
    let frontmatter_regex = Regex::new(r"(?s)^---\n(.*?)\n---").unwrap();
    let date_modified_regex = Regex::new(r"(?m)^date_modified:\s*(.*)$").unwrap();

    let today = Local::now().format("[[%Y-%m-%d]]").to_string();

    let updated_content = if let Some(captures) = frontmatter_regex.captures(content) {
        let frontmatter = captures.get(1).unwrap().as_str();
        let updated_frontmatter = if date_modified_regex.is_match(frontmatter) {
            date_modified_regex.replace(frontmatter, |_: &regex::Captures| {
                format!("date_modified: \"{}\"", today)
            }).to_string()
        } else {
            format!("{}\ndate_modified: \"{}\"", frontmatter.trim(), today)
        };

        frontmatter_regex.replace(content, |_: &regex::Captures| {
            format!("---\n{}\n---", updated_frontmatter.trim())
        }).to_string()
    } else {
        format!("---\ndate_modified: \"{}\"\n---\n{}", today, content.trim_start())
    };

    update_fn(&updated_content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_update_markdown_content() {
        let today = Local::now().format("[[%Y-%m-%d]]").to_string();

        // Test case 1: Existing frontmatter with date_modified
        let content1 = "---\ntitle: Test\ndate_modified: \"[[2023-01-01]]\"\n---\nContent";
        let expected1 = format!("---\ntitle: Test\ndate_modified: \"{}\"\n---\nContent", today);
        assert_eq!(update_markdown_content(content1, |s| s.to_string()), expected1);

        // Test case 2: Existing frontmatter without date_modified
        let content2 = "---\ntitle: Test\n---\nContent";
        let expected2 = format!("---\ntitle: Test\ndate_modified: \"{}\"\n---\nContent", today);
        assert_eq!(update_markdown_content(content2, |s| s.to_string()), expected2);

        // Test case 3: No frontmatter
        let content3 = "Content without frontmatter";
        let expected3 = format!("---\ndate_modified: \"{}\"\n---\nContent without frontmatter", today);
        assert_eq!(update_markdown_content(content3, |s| s.to_string()), expected3);
    }

    #[test]
    fn test_update_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        let content = "---\ntitle: Test\n---\nContent";
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();

        update_file(file_path, |s| s.replace("Content", "Updated Content")).unwrap();

        let updated_content = fs::read_to_string(temp_dir.path().join("test.md")).unwrap();
        let today = Local::now().format("[[%Y-%m-%d]]").to_string();
        assert!(updated_content.contains(&format!("date_modified: \"{}\"", today)));
        assert!(updated_content.contains("Updated Content"));
    }
}

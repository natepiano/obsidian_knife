use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};
use crate::validated_config::ValidatedConfig;
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};

pub fn process_simplify_wikilinks(
    config: &ValidatedConfig,
    collected_files: &HashMap<PathBuf, crate::scan::MarkdownFileInfo>,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _ = match config.simplify_wikilinks() {
        Some(patterns) if !patterns.is_empty() => patterns,
        _ => {
            writer.writeln("#", "simplify wikilinks")?;
            writer.writeln(
                "",
                "no simplification patterns specified - skipping wikilink simplification.",
            )?;
            return Ok(());
        }
    };

    writer.writeln("#", "simplify wikilinks")?;
    // Count total wikilinks
    let total_wikilinks: usize = collected_files
        .values()
        .map(|file_info| file_info.wikilinks.len())
        .sum();

    writer.writeln("", &format!("total wikilinks found: {}", total_wikilinks))?;
    writer.writeln(
        "",
        "the following wikilinks match the specified simplification patterns:\n",
    )?;

    let mut table_data = Vec::new();

    for (file_path, file_info) in collected_files {
        for wikilink in &file_info.wikilinks {
            let file_wikilink = format_wikilink(file_path);
            table_data.push(vec![
                file_wikilink,
                wikilink.line.to_string(),
                escape_pipes(&wikilink.line_text),
                escape_pipes(&wikilink.search_text),
                escape_pipes(&wikilink.replace_text),
            ]);
        }
    }

    if table_data.is_empty() {
        writer.writeln("", "no matching wikilinks found.")?;
        return Ok(());
    }

    let headers = &[
        "file",
        "line number",
        "line content",
        "search text",
        "replace with",
    ];

    writer.write_markdown_table(
        headers,
        &table_data,
        Some(&[
            ColumnAlignment::Left,
            ColumnAlignment::Right,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]),
    )?;

    if config.apply_changes() {
        writer.writeln("", "\napplying changes...")?;
        apply_simplifications(config, collected_files, writer)?;
    } else {
        writer.writeln("", "\ndry run mode: No changes applied.")?;
    }

    Ok(())
}

fn escape_pipes(s: &str) -> String {
    s.replace('|', "\\|")
}

fn format_wikilink(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| format!("[[{}]]", s))
        .unwrap_or_else(|| "[[]]".to_string())
}

fn apply_simplifications(
    config: &ValidatedConfig,
    collected_files: &HashMap<PathBuf, crate::scan::MarkdownFileInfo>,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let simplify_patterns = config.simplify_wikilinks().unwrap_or(&[]);
    let mut changes_made = 0;

    for (file_path, file_info) in collected_files {
        let mut file_content = std::fs::read_to_string(file_path)?;
        let mut file_changed = false;

        for wikilink in &file_info.wikilinks {
            if simplify_patterns.contains(&wikilink.replace_text) {
                file_content = file_content.replace(&wikilink.search_text, &wikilink.replace_text);
                file_changed = true;
                changes_made += 1;
            }
        }

        if file_changed {
            std::fs::write(file_path, file_content)?;
            writer.writeln("", &format!("Updated file: {:?}", file_path))?;
        }
    }

    writer.writeln("", &format!("Total changes made: {}", changes_made))?;
    Ok(())
}

use crate::file_utils::update_file;
use crate::scan::MarkdownFileInfo;
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
    let simplify_patterns = match config.simplify_wikilinks() {
        Some(patterns) if !patterns.is_empty() => patterns,
        _ => {
            writer.writeln("#", "simplify wikilinks")?;
            writer.writeln("", "no wikilink patterns specified")?;
            return Ok(());
        }
    };

    // Count total wikilinks
    let total_wikilinks: usize = collected_files
        .values()
        .map(|file_info| file_info.wikilinks.len())
        .sum();

    write_wikilinks_table_header(writer, &simplify_patterns, &total_wikilinks)?;

    // If there are wikilinks that match patterns, show the table
    if total_wikilinks > 0 {
        let table_data = get_wikilink_table_data(config, collected_files, simplify_patterns);

        write_wikilinks_table(writer, table_data)?;

        apply_simplifications(config, collected_files, writer)?;
    }

    Ok(())
}

fn write_wikilinks_table_header(
    writer: &ThreadSafeWriter,
    simplify_patterns: &&[String],
    total_wikilinks: &usize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln("#", "simplify wikilinks")?;

    // Display specified patterns
    writer.writeln(
        "",
        &format!("simplify patterns: {}", simplify_patterns.join(", ")),
    )?;

    writer.writeln(
        "",
        &format!(
            "count of wikilinks that render as a simplify pattern: {}",
            total_wikilinks
        ),
    )?;
    Ok(())
}

fn get_wikilink_table_data(
    config: &ValidatedConfig,
    collected_files: &HashMap<PathBuf, MarkdownFileInfo>,
    simplify_patterns: &[String],
) -> Vec<Vec<String>> {
    let mut table_data = Vec::new();

    for (file_path, file_info) in collected_files {
        for wikilink in &file_info.wikilinks {
            let file_wikilink = format_wikilink(file_path);
            let will_replace = simplify_patterns.contains(&wikilink.replace_text);
            let replaced = if will_replace {
                if config.apply_changes() {
                    "true"
                } else {
                    "false"
                }
            } else {
                "false"
            };
            table_data.push(vec![
                file_wikilink,
                wikilink.line.to_string(),
                escape_pipes(&wikilink.line_text),
                escape_pipes(&wikilink.search_text),
                escape_pipes(&wikilink.replace_text),
                replaced.to_string(),
            ]);
        }
    }
    table_data
}

fn write_wikilinks_table(
    writer: &ThreadSafeWriter,
    table_data: Vec<Vec<String>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if !table_data.is_empty() {
        writer.writeln("", "")?; // Add empty line before table
        let headers = &[
            "file",
            "line number",
            "line content",
            "search text",
            "replace with",
            "replaced",
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
                ColumnAlignment::Center,
            ]),
        )?;
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
    if !config.apply_changes() {
        return Ok(());
    }

    let simplify_patterns = config.simplify_wikilinks().unwrap_or(&[]);
    let mut changes_made = 0;

    for (file_path, file_info) in collected_files {
        let mut file_changed = false;

        update_file(file_path, |content| {
            let mut updated_content = content.to_string();
            for wikilink in &file_info.wikilinks {
                if simplify_patterns.contains(&wikilink.replace_text) {
                    updated_content =
                        updated_content.replace(&wikilink.search_text, &wikilink.replace_text);
                    file_changed = true;
                    changes_made += 1;
                }
            }
            updated_content
        })?;

        if file_changed {
            writer.writeln("", &format!("Updated file: {:?}", file_path))?;
        }
    }

    writer.writeln("", &format!("Total changes made: {}", changes_made))?;
    Ok(())
}

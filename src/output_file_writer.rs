use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::sync::MutexGuard;

use crate::constants::MARKDOWN_TABLE_ALIGNMENT_CENTER;
use crate::constants::MARKDOWN_TABLE_ALIGNMENT_LEFT;
use crate::constants::MARKDOWN_TABLE_ALIGNMENT_RIGHT;
use crate::constants::MARKDOWN_TABLE_ROW_TEMPLATE;
use crate::constants::MARKDOWN_TABLE_SEPARATOR;
use crate::constants::MARKDOWN_TABLE_TRAILING_SEPARATOR;
use crate::constants::OUTPUT_MARKDOWN_FILE;

pub(crate) struct OutputFileWriter {
    file: Mutex<File>,
}

#[derive(Clone, Copy)]
pub(crate) enum ColumnAlignment {
    Left,
    Center,
    Right,
}

impl OutputFileWriter {
    fn markdown_table_row(cells: &str) -> String {
        MARKDOWN_TABLE_ROW_TEMPLATE.replacen("{}", cells, 1)
    }

    fn lock_file(&self) -> io::Result<MutexGuard<'_, File>> {
        self.file
            .lock()
            .map_err(|error| io::Error::other(format!("output file lock poisoned: {error}")))
    }

    pub(crate) fn new(obsidian_path: &Path) -> io::Result<Self> {
        let file_path = obsidian_path.join(OUTPUT_MARKDOWN_FILE);

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)?;

        Ok(Self {
            file: Mutex::new(file),
        })
    }

    pub(crate) fn write_markdown_table(
        &self,
        headers: &[&str],
        rows: &[Vec<String>],
        alignments: Option<&[ColumnAlignment]>,
    ) -> io::Result<()> {
        let separator = alignments.map_or_else(
            || {
                Self::markdown_table_row(
                    &headers
                        .iter()
                        .map(|_| MARKDOWN_TABLE_SEPARATOR)
                        .collect::<Vec<_>>()
                        .join(" | "),
                )
            },
            |aligns| {
                let sep: Vec<String> = aligns
                    .iter()
                    .map(|&alignment| match alignment {
                        ColumnAlignment::Left => MARKDOWN_TABLE_ALIGNMENT_LEFT.to_string(),
                        ColumnAlignment::Center => MARKDOWN_TABLE_ALIGNMENT_CENTER.to_string(),
                        ColumnAlignment::Right => MARKDOWN_TABLE_ALIGNMENT_RIGHT.to_string(),
                    })
                    .collect();
                Self::markdown_table_row(&sep.join(" | "))
            },
        );

        let mut file = self.lock_file()?;

        // markdown tables always have to have a blank line before them
        writeln!(file, "\n{}", Self::markdown_table_row(&headers.join(" | ")))?;
        writeln!(file, "{separator}")?;

        for row in rows {
            writeln!(file, "{}", Self::markdown_table_row(&row.join(" | ")))?;
        }

        // there has to be a blank line after a table or it won't render
        writeln!(file, "{MARKDOWN_TABLE_TRAILING_SEPARATOR}")?;

        file.flush()?;
        drop(file);
        Ok(())
    }

    pub(crate) fn write_properties(&self, properties: &str) -> io::Result<()> {
        let mut file = self.lock_file()?;
        writeln!(file, "---")?;
        writeln!(file, "{properties}")?;
        writeln!(file, "---")?;
        file.flush()
    }

    pub(crate) fn writeln(&self, markdown_prefix: &str, message: &str) -> io::Result<()> {
        let prefix = if markdown_prefix.is_empty() {
            String::new()
        } else if markdown_prefix.ends_with(' ') {
            markdown_prefix.to_string()
        } else {
            format!("{markdown_prefix} ")
        };

        let file_message = format!("{prefix}{message}\n");
        let mut file = self.lock_file()?;
        file.write_all(file_message.as_bytes())?;
        file.flush()
    }
}

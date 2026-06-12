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
use crate::constants::MARKDOWN_TABLE_CELL_SEPARATOR;
use crate::constants::MARKDOWN_TABLE_PLACEHOLDER;
use crate::constants::MARKDOWN_TABLE_ROW_PLACEHOLDER_REPLACEMENT_LIMIT;
use crate::constants::MARKDOWN_TABLE_ROW_TEMPLATE;
use crate::constants::MARKDOWN_TABLE_SEPARATOR;
use crate::constants::MARKDOWN_TABLE_TRAILING_SEPARATOR;
use crate::constants::OUTPUT_FILE_LOCK_POISONED;
use crate::constants::OUTPUT_MARKDOWN_FILE;
use crate::constants::SPACE;
use crate::constants::YAML_CLOSING_DELIMITER;
use crate::constants::YAML_OPENING_DELIMITER;

#[derive(Clone, Copy)]
pub(crate) enum ColumnAlignment {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy)]
enum MarkdownPrefix<'a> {
    Empty,
    AlreadySpaced(&'a str),
    NeedsSpace(&'a str),
}

pub(crate) struct OutputFileWriter {
    file: Mutex<File>,
}

impl OutputFileWriter {
    fn markdown_table_row(cells: &str) -> String {
        MARKDOWN_TABLE_ROW_TEMPLATE.replacen(
            MARKDOWN_TABLE_PLACEHOLDER,
            cells,
            MARKDOWN_TABLE_ROW_PLACEHOLDER_REPLACEMENT_LIMIT,
        )
    }

    fn lock_file(&self) -> io::Result<MutexGuard<'_, File>> {
        self.file
            .lock()
            .map_err(|error| io::Error::other(format!("{OUTPUT_FILE_LOCK_POISONED}: {error}")))
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
                        .join(MARKDOWN_TABLE_CELL_SEPARATOR),
                )
            },
            |aligns| {
                let alignment_separators: Vec<String> = aligns
                    .iter()
                    .map(|&alignment| match alignment {
                        ColumnAlignment::Left => MARKDOWN_TABLE_ALIGNMENT_LEFT.to_string(),
                        ColumnAlignment::Center => MARKDOWN_TABLE_ALIGNMENT_CENTER.to_string(),
                        ColumnAlignment::Right => MARKDOWN_TABLE_ALIGNMENT_RIGHT.to_string(),
                    })
                    .collect();
                Self::markdown_table_row(&alignment_separators.join(MARKDOWN_TABLE_CELL_SEPARATOR))
            },
        );

        let mut file = self.lock_file()?;

        // Markdown tables require a blank line before the header row.
        writeln!(
            file,
            "\n{}",
            Self::markdown_table_row(&headers.join(MARKDOWN_TABLE_CELL_SEPARATOR))
        )?;
        writeln!(file, "{separator}")?;

        for row in rows {
            writeln!(
                file,
                "{}",
                Self::markdown_table_row(&row.join(MARKDOWN_TABLE_CELL_SEPARATOR))
            )?;
        }

        // `MARKDOWN_TABLE_TRAILING_SEPARATOR` terminates the rendered table.
        writeln!(file, "{MARKDOWN_TABLE_TRAILING_SEPARATOR}")?;

        file.flush()?;
        drop(file);
        Ok(())
    }

    pub(crate) fn write_properties(&self, properties: &str) -> io::Result<()> {
        let mut file = self.lock_file()?;
        write!(file, "{YAML_OPENING_DELIMITER}")?;
        writeln!(file, "{properties}")?;
        write!(file, "{YAML_CLOSING_DELIMITER}")?;
        file.flush()
    }

    pub(crate) fn writeln(&self, markdown_prefix: &str, message: &str) -> io::Result<()> {
        let prefix = MarkdownPrefix::from(markdown_prefix).into_string();

        let file_message = format!("{prefix}{message}\n");
        let mut file = self.lock_file()?;
        file.write_all(file_message.as_bytes())?;
        file.flush()
    }
}

impl<'a> From<&'a str> for MarkdownPrefix<'a> {
    fn from(markdown_prefix: &'a str) -> Self {
        match (markdown_prefix.is_empty(), markdown_prefix.ends_with(SPACE)) {
            (true, _) => Self::Empty,
            (false, true) => Self::AlreadySpaced(markdown_prefix),
            (false, false) => Self::NeedsSpace(markdown_prefix),
        }
    }
}

impl MarkdownPrefix<'_> {
    fn into_string(self) -> String {
        match self {
            Self::Empty => String::new(),
            Self::AlreadySpaced(markdown_prefix) => markdown_prefix.to_string(),
            Self::NeedsSpace(markdown_prefix) => format!("{markdown_prefix} "),
        }
    }
}

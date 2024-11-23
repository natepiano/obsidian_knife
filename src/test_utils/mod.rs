mod assert_utils;
mod test_file_builder;

use crate::markdown_file_info::MarkdownFileInfo;
use crate::DEFAULT_TIMEZONE;
pub use assert_utils::*;
use std::path::PathBuf;
pub use test_file_builder::parse_datetime;
pub use test_file_builder::TestFileBuilder;

use chrono::{DateTime, TimeZone, Utc};

/// Creates a DateTime<Utc> set to midnight Eastern time (05:00 UTC) for the given date
/// This ensures consistent time handling across the codebase for matching filesystem dates
/// with date strings like "[[2024-01-15]]" when viewed in Eastern timezone
///
/// # Arguments
/// * `year` - The year (e.g., 2024)
/// * `month` - The month (1-12)
/// * `day` - The day of the month (1-31)
///
/// # Returns
/// DateTime<Utc> set to 05:00:00 UTC (midnight Eastern) for the given date
///

pub fn eastern_midnight(year: i32, month: u32, day: u32) -> DateTime<Utc> {
    // Using 05:00 UTC (midnight Eastern) ensures dates like "[[2024-01-15]]" match
    // the filesystem dates when viewed in Eastern timezone
    Utc.with_ymd_and_hms(year, month, day, 5, 0, 0).unwrap()
}

pub fn get_test_markdown_file_info(path: PathBuf) -> MarkdownFileInfo {
    MarkdownFileInfo::new(path, DEFAULT_TIMEZONE).unwrap()
}

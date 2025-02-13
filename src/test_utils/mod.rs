mod assert_utils;
mod test_file_builder;

use crate::markdown_file::MarkdownFile;
use crate::DEFAULT_TIMEZONE;
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use std::path::PathBuf;

pub use assert_utils::*;
pub use test_file_builder::TestFileBuilder;

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

pub fn parse_datetime(s: &str) -> DateTime<Utc> {
    if let Ok(naive_dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        Utc.from_utc_datetime(&naive_dt)
    } else if let Ok(naive_date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let naive_dt = naive_date.and_hms_opt(0, 0, 0).unwrap();
        Utc.from_utc_datetime(&naive_dt)
    } else {
        panic!("Invalid format");
    }
}

pub fn get_test_markdown_file(path: PathBuf) -> MarkdownFile {
    MarkdownFile::new(path, DEFAULT_TIMEZONE).unwrap()
}

use std::path::PathBuf;

use chrono::DateTime;
use chrono::NaiveDate;
use chrono::NaiveDateTime;
use chrono::TimeZone;
use chrono::Utc;

use crate::constants::DEFAULT_TIMEZONE;
use crate::markdown_file::MarkdownFile;

pub fn eastern_midnight(year: i32, month: u32, day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, 5, 0, 0).unwrap()
}

pub fn parse_datetime(s: &str) -> DateTime<Utc> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").map_or_else(
        |_| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d").map_or_else(
                |_| panic!("Invalid format"),
                |naive_date| {
                    let naive_dt = naive_date.and_hms_opt(0, 0, 0).unwrap();
                    Utc.from_utc_datetime(&naive_dt)
                },
            )
        },
        |naive_dt| Utc.from_utc_datetime(&naive_dt),
    )
}

pub fn get_test_markdown_file(path: PathBuf) -> MarkdownFile {
    MarkdownFile::new(path, DEFAULT_TIMEZONE).unwrap()
}

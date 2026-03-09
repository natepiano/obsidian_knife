mod assert_utils;
mod markdown_file_fixtures;
mod test_file_builder;
mod validated_config_fixtures;

use std::path::PathBuf;

pub use assert_utils::*;
use chrono::DateTime;
use chrono::NaiveDate;
use chrono::NaiveDateTime;
use chrono::TimeZone;
use chrono::Utc;
pub use markdown_file_fixtures::build_aho_corasick;
pub use markdown_file_fixtures::create_markdown_test_file;
pub use markdown_file_fixtures::create_test_environment;
pub use test_file_builder::TestFileBuilder;
pub use validated_config_fixtures::get_test_validated_config;
pub use validated_config_fixtures::get_test_validated_config_builder;
pub use validated_config_fixtures::get_test_validated_config_result;

use crate::constants::DEFAULT_TIMEZONE;
use crate::markdown_file::MarkdownFile;

pub fn eastern_midnight(year: i32, month: u32, day: u32) -> DateTime<Utc> {
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

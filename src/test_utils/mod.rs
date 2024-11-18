mod assert_utils;
mod test_file_builder;

pub use assert_utils::*;
use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};
pub use test_file_builder::TestFileBuilder;

pub fn parse_datetime(s: &str) -> chrono::DateTime<Local> {
    if let Ok(naive_dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        Local.from_local_datetime(&naive_dt).unwrap()
    } else if let Ok(naive_date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let naive_dt = naive_date.and_hms_opt(0, 0, 0).unwrap();
        Local.from_local_datetime(&naive_dt).unwrap()
    } else {
        panic!("Invalid format");
    }
}

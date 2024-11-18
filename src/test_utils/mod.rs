mod assert_utils;
mod test_file_builder;

pub use assert_utils::*;
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
pub use test_file_builder::TestFileBuilder;

pub(crate) fn parse_datetime(s: &str) -> DateTime<Utc> {
    if let Ok(naive_dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        Utc.from_utc_datetime(&naive_dt)
    } else if let Ok(naive_date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let naive_dt = naive_date.and_hms_opt(0, 0, 0).unwrap();
        Utc.from_utc_datetime(&naive_dt)
    } else {
        panic!("Invalid format");
    }
}

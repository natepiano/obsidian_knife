use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use chrono::{Local, TimeZone};
use tempfile::TempDir;

/// Creates test files in the specified directory from a slice of (filename, content) tuples.
///
/// # Arguments
/// * `dir` - The directory where files should be created
/// * `files` - Slice of tuples containing (filename, content)
///
/// # Returns
/// * `Result<(), Box<dyn Error>>` - Success or error if file operations fail
pub fn create_test_files<P: AsRef<Path>>(
    dir: P,
    files: &[(&str, &str)],
) -> Result<(), Box<dyn Error>> {
    for (filename, content) in files.iter() {
        let file_path = dir.as_ref().join(filename);
        let mut file = File::create(file_path)?;
        write!(file, "{}", content)?;
    }
    Ok(())
}

/// Asserts that two `Result` values (an actual result and an expected result) match,
/// with support for custom comparison logic for `Ok` values and detailed error reporting.
///
/// This function allows for a flexible test setup by accepting a custom comparison
/// function for `Ok` values. It also provides clear assertion error messages
/// for easier debugging when a test fails.
///
/// # Type Parameters
/// - `T`: The type of the `Ok` value in the `Result`.
/// - `E`: The type of the `Err` value in the `Result`. Must implement `PartialEq`
///        and `Debug` to enable comparison and formatted error output. One gotcha is that
///        if you're using an Enum of error variants, and they happen to carry string messages,
///        you might want to implement a custom PartialEq so that it's not crucial that the
///        strings match the code and the test as it's really the variant that usually matters
/// - `F`: A function or closure that defines the custom comparison logic for `Ok` values.
///
/// # Parameters
/// - `result`: The actual `Result` value obtained from the test case execution.
/// - `expected`: The expected `Result` value to compare against `result`.
/// - `test_name`: A name or description of the test case, used for more informative
///                error messages on failure.
/// - `ok_compare`: A function or closure that takes references to the `Ok` values of
///                 `result` and `expected`. It will be called to assert the equality
///                 of `Ok` values, and should panic if they do not match.
///
/// # Panics
/// - If `result` and `expected` have different `Ok` or `Err` values, a detailed assertion
///   message will be printed, showing the test name, the expected value, and the actual
///   value to facilitate debugging.
/// - If `result` is `Ok` and `expected` is `Err` (or vice versa), it panics with a
///   mismatch message.
///
/// # Example
/// ```ignore
/// use std::io;
///
/// let actual_result: Result<i32, io::Error> = Ok(42);
/// let expected_result: Result<i32, io::Error> = Ok(42);
///
/// assert_result(
///     actual_result,
///     expected_result,
///     "test equal Ok values",
///     |actual, expected| assert_eq!(actual, expected),
/// );
/// ```
pub fn assert_result<T, E, F>(
    result: Result<T, E>,
    expected: Result<T, E>,
    test_name: &str,
    ok_compare: F,
) where
    F: FnOnce(&T, &T),
    T: std::fmt::Debug + PartialEq,
    E: std::fmt::Debug + PartialEq,
{
    match (&result, &expected) {
        (Ok(actual), Ok(expected)) => ok_compare(actual, expected),
        (Err(actual_err), Err(expected_err)) => {
            assert_eq!(
                actual_err, expected_err,
                "Failed test: {} - Expected error {:?}, got {:?}",
                test_name, expected_err, actual_err
            );
        }
        _ => panic!(
            "Failed test: {} - Result mismatch. Expected {:?}, got {:?}",
            test_name, expected, result
        ),
    }
}

// similar to assert_result but in the case where
// we're not testing something that has a Result<,> format
pub fn assert_test_case<T, E, F>(actual: T, expected: E, test_name: &str, compare_fn: F)
where
    F: FnOnce(&T, &E),
    T: std::fmt::Debug,
    E: std::fmt::Debug,
{
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        compare_fn(&actual, &expected)
    }));

    if result.is_err() {
        panic!(
            "Failed test: {} - Expected {:?}, got {:?}",
            test_name, expected, actual
        );
    }
}

pub fn create_test_date_create_fix_markdown_file(
    temp_dir: &TempDir,
    date_created_fix: Option<&str>,
    filename: &str,  // Add filename parameter
) -> PathBuf {
    let file_path = temp_dir.path().join(filename);
    let mut file = File::create(&file_path).unwrap();

    // Write frontmatter with valid dates
    writeln!(file, "---").unwrap();
    writeln!(file, "date_created: \"[[2024-01-15]]\"").unwrap();
    writeln!(file, "date_modified: \"[[2024-01-15]]\"").unwrap();
    if let Some(date) = date_created_fix {
        writeln!(file, "date_created_fix: \"{}\"", date).unwrap();
    }
    writeln!(file, "title: test").unwrap();
    writeln!(file, "---").unwrap();
    writeln!(file, "Test content").unwrap();

    // Set file timestamps to match frontmatter dates
    let test_time = filetime::FileTime::from_system_time(
        Local.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap().into(),
    );
    filetime::set_file_times(&file_path, test_time, test_time).unwrap();

    file_path
}

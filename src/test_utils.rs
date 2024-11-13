use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::path::Path;

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

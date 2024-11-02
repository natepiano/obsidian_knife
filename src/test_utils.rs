use std::error::Error;
#[cfg(test)]
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

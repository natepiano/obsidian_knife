use chrono::Local;
use obsidian_knife::*;
use std::error::Error;
use std::path::PathBuf;
use std::time::Instant;

// Custom error type for main specific errors
#[derive(Debug)]
enum MainError {
    Usage(String),
}

impl std::fmt::Display for MainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MainError::Usage(msg) => write!(f, "{}", msg),
        }
    }
}

impl Error for MainError {}

// Separate error handling and reporting logic
fn handle_error(
    e: Box<dyn Error + Send + Sync>,
    writer: &ThreadSafeWriter,
    start_time: Instant,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL1, ERROR_OCCURRED)?;
    writer.writeln(LEVEL2, ERROR_TYPE)?;
    writer.writeln("", &format!("{}", std::any::type_name_of_val(&*e)))?;
    writer.writeln(ERROR_DETAILS, &format!("{}", e))?;

    if let Some(source) = e.source() {
        writer.writeln(ERROR_SOURCE, &format!("{}", source))?;
    }

    output_duration(ERROR_DURATION, writer, start_time)?;

    Err(e)
}

fn output_duration(
    prefix: &str,
    writer: &ThreadSafeWriter,
    start_time: Instant,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let duration = start_time.elapsed();
    let duration_string = &format!("{:.2}", duration.as_millis());

    writer.writeln(prefix, &format!("{} {} {}", prefix, duration_string, "ms"))?;
    Ok(())
}

// Process successful execution
fn handle_success(
    config_file: &str,
    writer: &ThreadSafeWriter,
    start_time: Instant,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    println!();
    writer.writeln(
        LEVEL1,
        &format!("{} {}", PROCESSING_FINAL_MESSAGE, config_file),
    )?;

    output_duration(PROCESSING_DURATION, writer, start_time)?;

    Ok(())
}

// Get config file name with better error handling
fn get_config_file() -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 2 {
        return Err(Box::new(MainError::Usage(USAGE.into())));
    }

    Ok(PathBuf::from(&args[1]))
}

// Initialize configuration and writer
fn initialize_config(
    config_path: PathBuf,
) -> Result<(ValidatedConfig, ThreadSafeWriter), Box<dyn Error + Send + Sync>> {
    let config = Config::from_obsidian_file(&config_path)?;
    let validated_config = config.validate()?;
    let writer = ThreadSafeWriter::new(validated_config.output_folder())?;

    Ok((validated_config, writer))
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let start_time = Instant::now();
    let config_path = get_config_file()?;

    // Store config_file string before moving config_path
    let config_file = config_path.to_string_lossy().into_owned();

    let (validated_config, writer) = initialize_config(config_path)?;
    write_execution_start(&validated_config, &writer)?;

    match process_config(validated_config, &writer) {
        Ok(_) => handle_success(&config_file, &writer, start_time),
        Err(e) => handle_error(e, &writer, start_time),
    }
}

pub fn write_execution_start(
    validated_config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let timestamp = Local::now().format(FORMAT_TIME_STAMP);
    let properties = format!(
        "{}{}\n{}{}\n",
        YAML_TIMESTAMP,
        timestamp,
        YAML_APPLY_CHANGES,
        validated_config.apply_changes(),
    );

    writer.write_properties(&properties)?;
    writer.writeln(LEVEL1, PROCESSING_START)?;

    if validated_config.apply_changes() {
        writer.writeln("", MODE_APPLY_CHANGES)?;
    } else {
        writer.writeln("", MODE_DRY_RUN)?;
    }
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_environment() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();

        // Create Obsidian vault structure with absolute path
        let obsidian_path = temp_dir.path().join("vault");
        fs::create_dir(&obsidian_path).unwrap();
        fs::create_dir(obsidian_path.join(".obsidian")).unwrap();

        // Ensure path exists and get canonical path
        assert!(
            obsidian_path.exists(),
            "Obsidian path does not exist after creation"
        );
        let canonical_path = obsidian_path
            .canonicalize()
            .expect("Failed to get canonical path");

        // Create output directory
        fs::create_dir(canonical_path.join("output")).unwrap();

        // Create config file
        let config_path = temp_dir.path().join("config.md");
        let config_content = format!(
            r#"---
obsidian_path: {}
apply_changes: false
output_folder: output
---"#,
            canonical_path.to_string_lossy()
        );

        let mut file = File::create(&config_path).unwrap();
        write!(file, "{}", config_content).unwrap();

        assert!(
            config_path.exists(),
            "Config file does not exist after creation"
        );

        (temp_dir, config_path)
    }

    #[test]
    fn test_get_config_file_no_args() {
        match get_config_file() {
            Ok(_) => panic!("Expected error for missing arguments"),
            Err(e) => assert!(e.to_string().contains("usage:")),
        }
    }

    #[test]
    fn test_initialize_config_with_valid_setup() {
        let (_temp_dir, config_path) = create_test_environment();

        match initialize_config(config_path) {
            Ok((config, _)) => {
                assert!(!config.apply_changes(), "apply_changes should be false");
                assert!(
                    config.obsidian_path().exists(),
                    "Obsidian path should exist"
                );
            }
            Err(e) => panic!(
                "Failed to initialize config: {} (Obsidian path exists: {})",
                e,
                _temp_dir.path().join("vault").exists()
            ),
        }
    }

    #[test]
    fn test_initialize_config_with_missing_obsidian_path() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.md");

        let config_content = r#"---
obsidian_path: /nonexistent/path
apply_changes: false
---"#;

        let mut file = File::create(&config_path).unwrap();
        write!(file, "{}", config_content).unwrap();

        match initialize_config(config_path) {
            Ok(_) => panic!("Expected error for missing Obsidian path"),
            Err(e) => assert!(e.to_string().contains("obsidian path does not exist")),
        }
    }

    #[test]
    fn test_initialize_config_with_invalid_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.md");

        let config_content = r#"---
invalid: yaml: content:
---"#;

        let mut file = File::create(&config_path).unwrap();
        write!(file, "{}", config_content).unwrap();

        match initialize_config(config_path) {
            Ok(_) => panic!("Expected error for invalid YAML"),
            Err(_) => (), // Any error is fine here as we just want to ensure it fails
        }
    }
}

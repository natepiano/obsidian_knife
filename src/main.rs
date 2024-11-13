use ok::*;
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
    start_time: Instant,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("error occurred");
    println!("error type:");
    println!("{}", std::any::type_name_of_val(&*e));
    println!("error details: {}", e);

    if let Some(source) = e.source() {
        println!("error source: {}", source);
    }

    output_duration("error duration:", start_time)?;

    Err(e)
}

fn output_duration(prefix: &str, start_time: Instant) -> Result<(), Box<dyn Error + Send + Sync>> {
    let duration = start_time.elapsed();
    let duration_string = &format!("{:.2}", duration.as_millis());
    println!("{} {} {}", prefix, duration_string, DURATION_MILLISECONDS);
    Ok(())
}

// Process successful execution
fn handle_success(start_time: Instant) -> Result<(), Box<dyn Error + Send + Sync>> {
    output_duration(PROCESSING_DURATION, start_time)?;
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

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let start_time = Instant::now();
    let config_path = get_config_file()?;

    match process_config(config_path) {
        Ok(_) => handle_success(start_time),
        Err(e) => handle_error(e, start_time), // Removed writer parameter
    }
}

#[cfg(test)]
mod main_tests {
    use super::*;

    #[test]
    fn test_get_config_file_no_args() {
        match get_config_file() {
            Ok(_) => panic!("Expected error for missing arguments"),
            Err(e) => assert!(e.to_string().contains("usage:")),
        }
    }
}

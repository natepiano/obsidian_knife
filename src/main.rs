use crate::constants::*;
use crate::utils::Timer;
use ok::*;
use std::error::Error;
use std::path::PathBuf;

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
fn handle_error(e: Box<dyn Error + Send + Sync>) -> Result<(), Box<dyn Error + Send + Sync>> {
    eprintln!("{ERROR_OCCURRED}");
    eprintln!("{ERROR_TYPE}");
    eprintln!("{}", std::any::type_name_of_val(&*e));
    eprintln!("{ERROR_DETAILS} {}", e);

    if let Some(source) = e.source() {
        eprintln!("{ERROR_SOURCE} {}", source);
    }
    Err(e)
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
    let _timer = Timer::new(TOTAL_TIME);

    #[cfg(debug_assertions)]
    println!("{OBSIDIAN_KNIFE}\n{DEV} v.{}", env!("CARGO_PKG_VERSION"));

    #[cfg(not(debug_assertions))]
    println!(
        "{OBSIDIAN_KNIFE}\n{RELEASE} v.{}",
        env!("CARGO_PKG_VERSION")
    );

    let config_path = get_config_file()?;

    match process_obsidian_repository(config_path) {
        Ok(_) => Ok(()),
        Err(e) => handle_error(e), // Removed writer parameter
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

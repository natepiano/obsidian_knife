mod config;
mod scan;
mod validated_config;

use crate::{config::Config, scan::scan_obsidian_folder, validated_config::ValidatedConfig};
use std::error::Error;
use std::path::Path;
use std::{env, fs};

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("\nstarting obsidian_knife");

    let config_file = match get_config_file_name() {
        Ok(value) => value,
        Err(value) => return value,
    };

    let config = read_config(&config_file)?;
    let validated_config = config.validate()?;

    match process_config(validated_config) {
        Ok(_) => {
            println!("obsidian_knife made the cut with {}", config_file);
            Ok(())
        }
        Err(e) => {
            eprintln!("Error occurred during processing:");
            eprintln!("Error type: {}", std::any::type_name_of_val(&*e));
            eprintln!("Error details: {}", e);
            if let Some(source) = e.source() {
                eprintln!("Caused by: {}", source);
            }
            Err(e)
        }
    }
}

fn process_config(config: ValidatedConfig) -> Result<(), Box<dyn Error + Send + Sync>> {
    scan_obsidian_folder(config);
    Ok(())
}

fn get_config_file_name() -> Result<String, Result<(), Box<dyn Error + Send + Sync>>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return Err(Err("Usage: obsidian_knife <config_file.yaml>".into()));
    }

    let config_file = &args[1];
    Ok(config_file.into())
}

fn read_config(config_file: &str) -> Result<Config, Box<dyn Error + Send + Sync>> {
    let path = Path::new(config_file);
    let contents = fs::read_to_string(path).map_err(|e| -> Box<dyn Error + Send + Sync> {
        if e.kind() == std::io::ErrorKind::NotFound {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Config file not found: {}", path.display()),
            ))
        } else {
            Box::new(std::io::Error::new(
                e.kind(),
                format!("Error reading config file '{}': {}", path.display(), e),
            ))
        }
    })?;

    let config: Config =
        serde_yaml::from_str(&contents).map_err(|e| -> Box<dyn Error + Send + Sync> {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Error parsing config file '{}': {}", path.display(), e),
            ))
        })?;

    Ok(config)
}

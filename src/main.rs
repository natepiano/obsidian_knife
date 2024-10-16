mod config;
mod scan;
mod validated_config;
mod constants;
mod thread_safe_output;

use crate::{config::Config, scan::scan_obsidian_folder, validated_config::ValidatedConfig};
use std::error::Error;
use std::path::Path;
use std::{env, fs};
use crate::thread_safe_output::ThreadSafeOutput;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {

    let config_file = match get_config_file_name() {
        Ok(value) => value,
        Err(value) => return value,
    };

    let config = read_config(&config_file)?;
    let validated_config = config.validate()?;

    let output = ThreadSafeOutput::new(validated_config.obsidian_path())?;
    output.write("\nstarting obsidian_knife\n")?;

    match process_config(validated_config, &output) {
        Ok(_) => {
            output.write(&format!("\nobsidian_knife made the cut using {}\n", config_file))?;
            Ok(())
        }
        Err(e) => {
            output.write("\nError occurred during processing:\n")?;
            output.write(&format!("Error type: {}\n", std::any::type_name_of_val(&*e)))?;
            output.write(&format!("Error details: {}\n", e))?;
            if let Some(source) = e.source() {
                output.write(&format!("Caused by: {}\n", source))?;
            }
            Err(e)
        }
    }
}

fn process_config(config: ValidatedConfig, output: &ThreadSafeOutput) -> Result<(), Box<dyn Error + Send + Sync>> {
    scan_obsidian_folder(config, output);
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

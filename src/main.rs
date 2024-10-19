mod cleanup_images;
mod config;
mod constants;
mod scan;
mod sha256_cache;
mod thread_safe_writer;
mod validated_config;

use crate::cleanup_images::cleanup_images;
use crate::thread_safe_writer::ThreadSafeWriter;
use crate::{config::Config, scan::scan_obsidian_folder, validated_config::ValidatedConfig};
use chrono::Local;
use std::error::Error;
use std::path::Path;
use std::time::Instant;
use std::{env, fs};

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let start_time = Instant::now();

    let config_file = match get_config_file_name() {
        Ok(value) => value,
        Err(value) => return value,
    };

    let config = read_config(&config_file)?;
    let validated_config = config.validate()?;

    let writer = ThreadSafeWriter::new(validated_config.obsidian_path())?;

    write_execution_start(&validated_config, &writer)?;

    match process_config(validated_config, &writer) {
        Ok(_) => {
            println!();
            writer.writeln(
                "# ",
                &format!("obsidian_knife made the cut using {}", config_file),
            )?;
            let duration = start_time.elapsed();
            let duration_secs = duration.as_secs_f64();
            writer.writeln(
                "",
                &format!("Total processing time: {:.2} seconds", duration_secs),
            )?;
            Ok(())
        }
        Err(e) => {
            writer.writeln("## Error Occurred", "Error occurred during processing:")?;
            writer.writeln(
                "- **Error type:** ",
                &format!("{}", std::any::type_name_of_val(&*e)),
            )?;
            writer.writeln("- **Error details:** ", &format!("{}", e))?;
            if let Some(source) = e.source() {
                writer.writeln("- **Caused by:** ", &format!("{}", source))?;
            }
            let duration = start_time.elapsed();
            let duration_secs = duration.as_secs_f64();
            writer.writeln(
                "",
                &format!("Total processing time before error: {:.2?}", duration_secs),
            )?;
            Err(e)
        }
    }
}

fn process_config(
    config: ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let collected_files = scan_obsidian_folder(&config, writer)?;

    cleanup_images(&config, collected_files, writer)?;

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

fn write_execution_start(
    validated_config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let properties = format!(
        "time_stamp: {}\napply_changes: {}\ndedupe_images: {}",
        timestamp,
        validated_config.apply_changes(),
        validated_config.cleanup_image_files()
    );

    writer.write_properties(&properties)?;

    writer.writeln("# ", "starting obsidian_knife")?;
    println!();
    Ok(())
}

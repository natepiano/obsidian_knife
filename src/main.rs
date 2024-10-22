mod cleanup_images;
mod config;
mod constants;
mod file_utils;
mod scan;
mod sha256_cache;
mod simplify_wikilinks;
mod thread_safe_writer;
mod validated_config;

use crate::cleanup_images::cleanup_images;
use crate::simplify_wikilinks::process_simplify_wikilinks;
use crate::thread_safe_writer::ThreadSafeWriter;
use crate::{config::Config, scan::scan_obsidian_folder, validated_config::ValidatedConfig};
use chrono::Local;
use std::env;
use std::error::Error;
use std::path::Path;
use std::time::Instant;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let start_time = Instant::now();

    let config_file = match get_config_file_name() {
        Ok(value) => value,
        Err(value) => return value,
    };

    let config = read_config(&config_file)?;
    let validated_config = config.validate()?;

    let writer = ThreadSafeWriter::new(validated_config.output_folder())?;

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
                &format!("total processing time: {:.2} seconds", duration_secs),
            )?;
            Ok(())
        }
        Err(e) => {
            writer.writeln("## error occurred", "error occurred during processing:")?;
            writer.writeln(
                "- **error type:** ",
                &format!("{}", std::any::type_name_of_val(&*e)),
            )?;
            writer.writeln("- **error details:** ", &format!("{}", e))?;
            if let Some(source) = e.source() {
                writer.writeln("- **caused by:** ", &format!("{}", source))?;
            }
            let duration = start_time.elapsed();
            let duration_secs = duration.as_secs_f64();
            writer.writeln(
                "",
                &format!("total processing time before error: {:.2?}", duration_secs),
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

    cleanup_images(&config, &collected_files, writer)?;

    process_simplify_wikilinks(&config, &collected_files.markdown_files, writer)?;

    Ok(())
}

fn get_config_file_name() -> Result<String, Result<(), Box<dyn Error + Send + Sync>>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return Err(Err(
            "usage: obsidian_knife <obsidian_folder/config_file.md>".into()
        ));
    }

    let config_file = &args[1];
    Ok(config_file.into())
}

fn read_config(config_file: &str) -> Result<Config, Box<dyn Error + Send + Sync>> {
    let path = Path::new(config_file);
    Config::from_obsidian_file(path)
}

fn write_execution_start(
    validated_config: &ValidatedConfig,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let properties = format!(
        "time_stamp: {}\napply_changes: {}\n",
        timestamp,
        validated_config.apply_changes(),
    );

    writer.write_properties(&properties)?;

    writer.writeln("# ", "starting obsidian_knife")?;
    if validated_config.apply_changes() {
        writer.writeln("", "apply_changes enabled: changes will be applied")?;
    } else {
        writer.writeln("", "apply_changes disabled: no changes will be applied")?;
    }
    println!();
    Ok(())
}

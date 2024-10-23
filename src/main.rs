use obsidian_knife::{process_config, write_execution_start, Config, ThreadSafeWriter};
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

    let config = Config::from_obsidian_file(Path::new(&config_file))?;
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
            writer.writeln(
                "",
                &format!(
                    "total processing time: {:.2} seconds",
                    duration.as_secs_f64()
                ),
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
            writer.writeln(
                "",
                &format!(
                    "total processing time before error: {:.2} seconds",
                    duration.as_secs_f64()
                ),
            )?;
            Err(e)
        }
    }
}

fn get_config_file_name() -> Result<String, Result<(), Box<dyn Error + Send + Sync>>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return Err(Err(
            "usage: obsidian_knife <obsidian_folder/config_file.md>".into(),
        ));
    }
    Ok(args[1].clone())
}

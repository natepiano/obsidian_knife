mod structs;

use std::{env, fs};
use std::error::Error;
use structs::Config;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return Err("Usage: {} <config_file.yaml>".into());
    }

    let config_file = &args[1];
    let config = read_config(config_file)?;


    println!("obsidian_knife made the cut with {}", config_file);
    Ok(())
}

pub fn read_config(config_file: &str) -> Result<Config, Box<dyn Error + Send + Sync>> {
    let contents = fs::read_to_string(config_file)?;
    let config: Config = serde_yaml::from_str(&contents)?;
    Ok(config)
}

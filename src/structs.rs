use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub obsidian_path: String,
}

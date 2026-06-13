#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]
mod test_support;

mod config;
mod constants;
mod description_builder;
mod frontmatter;
mod image_file;
mod markdown_file;
mod markdown_files;
mod obsidian_repository;
mod output_file_writer;
mod report;
mod run;
mod sha256_cache;
mod support;
mod timer;
mod validated_config;
mod wikilink;
mod yaml_frontmatter;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> { run::run() }

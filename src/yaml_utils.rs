use serde::de::DeserializeOwned;
use std::error::Error;

/// Extracts YAML front matter from the given content string and deserializes it into the specified structure.
///
/// # Arguments
///
/// * `content` - A string slice containing the entire file content.
///
/// # Returns
///
/// * `Ok(T)` where `T` is the deserialized structure.
/// * `Err(Box<dyn Error + Send + Sync>)` if extraction or deserialization fails.
pub fn deserialize_yaml_frontmatter<T>(content: &str) -> Result<T, Box<dyn Error + Send + Sync>>
where
    T: DeserializeOwned,
{
    let yaml_str = extract_yaml_frontmatter(content)?;
    serde_yaml::from_str(&yaml_str).map_err(|e| {
        let error = std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("error parsing yaml configuration: {}", e),
        );
        Box::new(error) as Box<dyn Error + Send + Sync>
    })
}

/// Extracts YAML front matter from the given content string.
///
/// # Arguments
///
/// * `content` - A string slice containing the entire file content.
///
/// # Returns
///
/// * `Ok(String)` containing the extracted YAML front matter.
/// * `Err(Box<dyn Error + Send + Sync>)` if extraction fails.
fn extract_yaml_frontmatter(content: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file must start with YAML frontmatter (---)",
        )));
    }

    // Find the second occurrence of "---"
    let after_first = &trimmed[3..];
    if let Some(end_index) = after_first.find("---") {
        Ok(after_first[..end_index].trim().to_string())
    } else {
        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file must have closing YAML frontmatter (---)",
        )))
    }
}

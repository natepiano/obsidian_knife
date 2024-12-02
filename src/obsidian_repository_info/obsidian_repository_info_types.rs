use std::path::PathBuf;

#[derive(Debug)]
pub enum ImageOperation {
    Delete(PathBuf),
}

#[derive(Debug)]
pub enum MarkdownOperation {
    RemoveReference {
        markdown_path: PathBuf,
        image_path: PathBuf
    },
    UpdateReference {
        markdown_path: PathBuf,
        old_image_path: PathBuf,
        new_image_path: PathBuf
    }
}

#[derive(Debug, Default)]
pub struct ImageOperations {
    pub image_ops: Vec<ImageOperation>,
    pub markdown_ops: Vec<MarkdownOperation>,
}

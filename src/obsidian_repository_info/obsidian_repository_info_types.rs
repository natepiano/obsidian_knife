use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug)]
pub enum ImageOperation {
    Delete(PathBuf),
}

#[derive(Debug)]
pub enum MarkdownOperation {
    RemoveReference {
        markdown_path: PathBuf,
        image_path: PathBuf,
    },
    UpdateReference {
        markdown_path: PathBuf,
        old_image_path: PathBuf,
        new_image_path: PathBuf,
    },
}

#[derive(Debug, Default)]
pub struct ImageOperations {
    pub image_ops: Vec<ImageOperation>,
    pub markdown_ops: Vec<MarkdownOperation>,
}

#[derive(Debug)]
pub enum FileOperation {
    Delete,
    RemoveReference(PathBuf),
    UpdateReference(PathBuf, PathBuf), // (old_path, new_path)
}

// represent different types of image groups
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ImageGroupType {
    TiffImage,
    ZeroByteImage,
    UnreferencedImage,
    DuplicateGroup(String), // String is the hash value
}

#[derive(Debug, Clone)]
pub struct ImageReferences {
    pub hash: String,
    pub markdown_file_references: Vec<String>,
}

#[derive(Clone)]
pub struct ImageGroup {
    pub path: PathBuf,
    pub info: ImageReferences,
}

#[derive(Default)]
pub struct GroupedImages {
    pub groups: HashMap<ImageGroupType, Vec<ImageGroup>>,
}

impl GroupedImages {
    pub(crate) fn new() -> Self {
        Self {
            groups: HashMap::new(),
        }
    }

    pub(crate) fn add_or_update(&mut self, group_type: ImageGroupType, image: ImageGroup) {
        self.groups.entry(group_type).or_default().push(image);
    }

    pub(crate) fn get(&self, group_type: &ImageGroupType) -> Option<&Vec<ImageGroup>> {
        self.groups.get(group_type)
    }

    pub(crate) fn get_duplicate_groups(&self) -> Vec<(&String, &Vec<ImageGroup>)> {
        self.groups
            .iter()
            .filter_map(|(key, group)| match key {
                ImageGroupType::DuplicateGroup(hash) if group.len() > 1 => Some((hash, group)),
                _ => None,
            })
            .collect()
    }
}

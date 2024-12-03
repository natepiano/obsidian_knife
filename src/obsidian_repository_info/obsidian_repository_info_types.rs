use std::collections::{HashMap, HashSet};
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

impl ImageOperations {
    // Add a method to get all markdown paths that need updating
    pub fn get_markdown_paths_to_update(&self) -> HashSet<PathBuf> {
        let mut paths = HashSet::new();

        // Add paths from markdown operations
        for op in &self.markdown_ops {
            match op {
                MarkdownOperation::RemoveReference { markdown_path, .. }
                | MarkdownOperation::UpdateReference { markdown_path, .. } => {
                    paths.insert(markdown_path.clone());
                }
            }
        }

        paths
    }
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

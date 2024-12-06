use crate::image_file_info::ImageFileInfo;
use std::path::PathBuf;
use std::slice::{Iter, IterMut};

#[derive(Default, Debug)]
pub struct ImageFiles {
    pub(crate) files: Vec<ImageFileInfo>,
}

impl ImageFiles {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    pub fn push(&mut self, file: ImageFileInfo) {
        self.files.push(file);
    }

    pub fn iter(&self) -> Iter<'_, ImageFileInfo> {
        self.files.iter()
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, ImageFileInfo> {
        self.files.iter_mut()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn get(&self, path: &PathBuf) -> Option<&ImageFileInfo> {
        self.files.iter().find(|f| f.path == *path)
    }

    pub fn get_mut(&mut self, path: &PathBuf) -> Option<&mut ImageFileInfo> {
        self.files.iter_mut().find(|f| f.path == *path)
    }

    pub fn mark_unreferenced_images(&mut self) {
        for image in self.files.iter_mut() {
            if image.references.is_empty() {
                image.mark_as_unreferenced();
            }
        }
    }
}

impl IntoIterator for ImageFiles {
    type Item = ImageFileInfo;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.files.into_iter()
    }
}

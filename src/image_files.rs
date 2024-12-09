use crate::image_file::ImageFile;
use std::path::PathBuf;
use std::slice::{Iter, IterMut};

#[derive(Default, Debug)]
pub struct ImageFiles {
    pub(crate) files: Vec<ImageFile>,
}

impl ImageFiles {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    pub fn push(&mut self, file: ImageFile) {
        self.files.push(file);
    }

    #[allow(dead_code)]
    pub fn iter(&self) -> Iter<'_, ImageFile> {
        self.files.iter()
    }
    #[allow(dead_code)]
    pub fn iter_mut(&mut self) -> IterMut<'_, ImageFile> {
        self.files.iter_mut()
    }
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.files.len()
    }
    #[allow(dead_code)]
    pub fn get(&self, path: &PathBuf) -> Option<&ImageFile> {
        self.files.iter().find(|f| f.path == *path)
    }

    #[allow(dead_code)]
    pub fn get_mut(&mut self, path: &PathBuf) -> Option<&mut ImageFile> {
        self.files.iter_mut().find(|f| f.path == *path)
    }

    #[allow(dead_code)]
    pub fn mark_unreferenced_images(&mut self) {
        for image in self.files.iter_mut() {
            if image.references.is_empty() {
                image.mark_as_unreferenced();
            }
        }
    }
}

impl IntoIterator for ImageFiles {
    type Item = ImageFile;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.files.into_iter()
    }
}

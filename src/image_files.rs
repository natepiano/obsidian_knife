use crate::image_file::{ImageFile, ImageFileState};

#[cfg(test)]
use std::path::PathBuf;

#[derive(Default, Debug)]
pub struct ImageFiles {
    pub(crate) files: Vec<ImageFile>,
}

impl FromIterator<ImageFile> for ImageFiles {
    fn from_iter<I: IntoIterator<Item = ImageFile>>(iter: I) -> Self {
        let files: Vec<ImageFile> = iter.into_iter().collect();
        ImageFiles { files }
    }
}

impl IntoIterator for ImageFiles {
    type Item = ImageFile;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.files.into_iter()
    }
}

impl ImageFiles {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    pub fn push(&mut self, file: ImageFile) {
        self.files.push(file);
    }

    // #[allow(dead_code)]
    // pub fn iter(&self) -> Iter<'_, ImageFile> {
    //     self.files.iter()
    // }
    // #[allow(dead_code)]
    // pub fn iter_mut(&mut self) -> IterMut<'_, ImageFile> {
    //     self.files.iter_mut()
    // }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.files.len()
    }
    #[cfg(test)]
    pub fn get(&self, path: &PathBuf) -> Option<&ImageFile> {
        self.files.iter().find(|f| f.path == *path)
    }

    // #[allow(dead_code)]
    // pub fn get_mut(&mut self, path: &PathBuf) -> Option<&mut ImageFile> {
    //     self.files.iter_mut().find(|f| f.path == *path)
    // }

    pub fn files_in_state<F>(&self, predicate: F) -> ImageFiles
    where
        F: Fn(&ImageFileState) -> bool,
    {
        self.files
            .iter()
            .filter(|image_file| predicate(&image_file.image_state))
            .cloned()
            .collect()
    }
}

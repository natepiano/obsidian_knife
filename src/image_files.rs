use std::error::Error;
use std::fs;

use derive_more::Deref;
use derive_more::DerefMut;
use derive_more::IntoIterator;

use crate::image_file::DeletionStatus;
use crate::image_file::ImageFile;

#[derive(Default, Debug, PartialEq, Eq, Deref, DerefMut, IntoIterator)]
pub(crate) struct ImageFiles {
    #[deref]
    #[deref_mut]
    #[into_iterator]
    pub(super) images: Vec<ImageFile>,
}

impl FromIterator<ImageFile> for ImageFiles {
    fn from_iter<I: IntoIterator<Item = ImageFile>>(iter: I) -> Self {
        Self {
            images: iter.into_iter().collect(),
        }
    }
}

impl ImageFiles {
    pub(crate) fn delete_marked(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.images
            .iter()
            .filter(|file| file.deletion_status == DeletionStatus::Delete)
            .try_for_each(|file| fs::remove_file(&file.path))?;

        Ok(())
    }
}

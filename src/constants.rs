// only enable file updates during tests for now
// pub const ENABLE_FILE_UPDATES: bool = cfg!(test);

// cache stuff
pub const CACHE_FOLDER: &str = ".obsidian_knife";
pub const CACHE_FILE: &str = "image_cache.json";

// image stuff
pub const IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "png", "jpeg", "tiff", "pdf", "gif"];
pub const IMAGE_ALT_TEXT_DEFAULT: &str = "image";
pub const MISSING_IMAGE_REFERENCES: &str = "missing image references";
pub const NO_IMAGE_ISSUES: &str = "no issues found during image analysis";
pub const SECTION_IMAGE_CLEANUP: &str = "image cleanup";
pub const TIFF_EXTENSION: &str = "tiff";
pub const TIFF_IMAGES: &str = "TIFF images";
pub const UNREFERENCED_IMAGES: &str = "unreferenced images";
pub const ZERO_BYTE_IMAGES: &str = "zero-byte images";

//markdown stuff
pub const LEVEL1: &str = "#";
pub const LEVEL2: &str = "##";
pub const LEVEL3: &str = "###";

// regex stuff
pub const CLOSING_BRACKET: char = ']';
pub const CLOSING_WIKILINK: &str = "]]";
pub const FORWARD_SLASH: char = '/';
pub const OPENING_BRACKET: char = '[';
pub const OPENING_IMAGE_LINK_BRACKET: &str = "!["; // for external style "![]()"
pub const OPENING_IMAGE_WIKILINK_BRACKET: &str = "![[";
pub const OPENING_PAREN: char = '(';
pub const OPENING_WIKILINK: &str = "[[";

// wikilink back populate
pub const BACK_POPULATE: &str = "back populate wikilinks";

#[derive(Debug, Clone, Copy)]
pub enum Phrase {
    // File-related phrases
    Files,
    InvalidDates,
    PropertyErrors,
    DateCreated,
    DateModified,

    // Image-related phrases
    MissingImageReferences,
    UnreferencedImages,
    ZeroByteImages,
    DuplicateImages,
    TiffImages,
}

/// Pluralizes a phrase based on count at compile time
pub const fn pluralize(count: usize, phrase: Phrase) -> &'static str {
    match (count, phrase) {
        // File-related phrases
        (1, Phrase::Files) => "file",
        (_, Phrase::Files) => "files",

        (1, Phrase::InvalidDates) => "file has an invalid date",
        (_, Phrase::InvalidDates) => "files have invalid dates",

        (1, Phrase::PropertyErrors) => "file has a yaml property error",
        (_, Phrase::PropertyErrors) => "files have yaml property errors",

        (1, Phrase::DateModified) => "file has an issue with date_modified",
        (_, Phrase::DateModified) => "files have issues with date_modified",

        (1, Phrase::DateCreated) => "file has an issue with date_created",
        (_, Phrase::DateCreated) => "files have issues with date_created",

        // Image-related phrases
        (1, Phrase::MissingImageReferences) => "file has missing image references",
        (_, Phrase::MissingImageReferences) => "files have missing image references",

        (1, Phrase::TiffImages) => "TIFF image will not render correctly in obsidian",
        (_, Phrase::TiffImages) => "TIFF images will not render correctly in obsidian",

        (1, Phrase::ZeroByteImages) => "image has zero bytes and is probably corrupted",
        (_, Phrase::ZeroByteImages) => "images have zero bytes and are probably corrupted",

        (1, Phrase::UnreferencedImages) => "image is not referenced by any file",
        (_, Phrase::UnreferencedImages) => "images are not referenced by any files",

        (1, Phrase::DuplicateImages) => "duplicate image",
        (_, Phrase::DuplicateImages) => "duplicate images",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_singular_phrases() {
        assert_eq!(
            pluralize(1, Phrase::InvalidDates),
            "file has an invalid date"
        );
        assert_eq!(
            pluralize(1, Phrase::UnreferencedImages),
            "image is not referenced by any file"
        );
    }

    #[test]
    fn test_plural_phrases() {
        assert_eq!(
            pluralize(0, Phrase::InvalidDates),
            "files have invalid dates"
        );
        assert_eq!(
            pluralize(2, Phrase::InvalidDates),
            "files have invalid dates"
        );
        assert_eq!(
            pluralize(5, Phrase::UnreferencedImages),
            "images are not referenced by any files"
        );
    }

    #[test]
    fn test_compile_time_constants() {
        const SINGULAR_MESSAGE: &str = pluralize(1, Phrase::InvalidDates);
        const PLURAL_MESSAGE: &str = pluralize(2, Phrase::InvalidDates);

        assert_eq!(SINGULAR_MESSAGE, "file has an invalid date");
        assert_eq!(PLURAL_MESSAGE, "files have invalid dates");
    }
}

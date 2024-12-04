// processing stuff
pub const DURATION_MILLISECONDS: &str = "ms";
pub const ERROR_DETAILS: &str = "**error details:**";
pub const ERROR_DURATION: &str = "total processing time before error:";
pub const ERROR_OCCURRED: &str = "error occurred";
pub const ERROR_SOURCE: &str = "**error source:**";
pub const ERROR_TYPE: &str = "error type:";
pub const FORMAT_TIME_STAMP: &str = "%Y-%m-%d %H:%M:%S";
pub const MODE_APPLY_CHANGES: &str = "changes will be applied";
pub const MODE_DRY_RUN: &str = "dry run - no changes will be applied";
pub const PROCESSING_DURATION: &str = "total processing time:";
pub const USAGE: &str = "usage: obsidian_knife <obsidian_folder/config_file.md>";
pub const YAML_TIMESTAMP: &str = "time_stamp: ";
pub const YAML_APPLY_CHANGES: &str = "apply_changes: ";

// config stuff
pub const DEFAULT_OUTPUT_FOLDER: &str = "obsidian_knife";
pub const DEFAULT_TIMEZONE: &str = "America/New_York";
pub const ERROR_NOT_FOUND: &str = "file not found: ";
pub const ERROR_READING: &str = "error reading config file ";
pub const ERROR_BACK_POPULATE_FILE_FILTER: &str = "back_populate_filter_filter cannot be empty";
pub const ERROR_OUTPUT_FOLDER: &str = "output_folder cannot be empty";
pub const OBSIDIAN_HIDDEN_FOLDER: &str = ".obsidian";
pub const OUTPUT_MARKDOWN_FILE: &str = "obsidian knife output.md";

// cache stuff
pub const CACHE_FOLDER: &str = ".ok";
pub const CACHE_FILE: &str = "obsidian_knife_cache.json";
pub const CACHE_INFO_CREATE_NEW: &str = "cache file missing - creating new cache:";
pub const CACHE_INFO_CORRUPTED: &str = "cache corrupted, creating new cache:";

// image stuff
// the DEFAULT_MEDIA_PATH could be a configuration parameter as it's really specific to my setup
pub const DEFAULT_MEDIA_PATH: &str = "conf/media";
pub const IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "png", "jpeg", "tiff", "pdf", "gif"];
pub const IMAGE_ALT_TEXT_DEFAULT: &str = "image";
pub const MISSING_IMAGE_REFERENCES: &str = "missing image references";
pub const SECTION_IMAGE_CLEANUP: &str = "images";
pub const TIFF_EXTENSION: &str = "tiff";
pub const TIFF_IMAGES: &str = "TIFF images";
pub const UNREFERENCED_IMAGES: &str = "unreferenced images";
pub const ZERO_BYTE_IMAGES: &str = "zero-byte images";

//markdown stuff
pub const LEVEL1: &str = "#";
pub const LEVEL2: &str = "##";
pub const LEVEL3: &str = "###";
pub const LEVEL4: &str = "####";

// regex stuff
pub const CLOSING_BRACKET: char = ']';
pub const CLOSING_WIKILINK: &str = "]]";
pub const FORWARD_SLASH: char = '/';
pub const OPENING_BRACKET: char = '[';
pub const OPENING_IMAGE_LINK_BRACKET: &str = "!["; // for external style "![]()"
pub const OPENING_IMAGE_WIKILINK_BRACKET: &str = "![[";
pub const OPENING_PAREN: char = '(';
pub const OPENING_WIKILINK: &str = "[[";

// report stuff
pub const FRONTMATTER_ISSUES: &str = "frontmatter issues";
pub const INVALID_WIKILINKS: &str = "invalid wikilinks";
pub const OF: &str = "of";
pub const THAT_NEED_UPDATES: &str = "that need updates will be saved";

// wikilink back populate
pub const BACK_POPULATE_TABLE_HEADER_MIDDLE: &str = "in";
pub const BACK_POPULATE_TABLE_HEADER_SUFFIX: &str = "will be back populated";

pub const BACK_POPULATE_COUNT_PREFIX: &str = "back populate";
pub const BACK_POPULATE_COUNT_SUFFIX: &str = "wikilinks";
pub const BACK_POPULATE_FILE_FILTER_PREFIX: &str =
    "using back_populate_file_filter config parameter: ";
pub const BACK_POPULATE_FILE_FILTER_SUFFIX: &str =
    "remove it from config if you want to process all files";
pub const COL_OCCURRENCES: &str = "occurrences";
pub const COL_SOURCE_TEXT: &str = "source text";
pub const COL_TEXT: &str = "text";
pub const COL_WILL_REPLACE_WITH: &str = "will replace with";
pub const MATCHES_AMBIGUOUS: &str = "ambiguous matches found - these will be skipped";
pub const MATCHES_UNAMBIGUOUS: &str = "matches found";

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

    // compound pluralize
    Matches,
    Times,
    TimeInFiles,
    TimesInFiles,
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

        (1, Phrase::Matches) => "match",
        (_, Phrase::Matches) => "matches",

        (1, Phrase::Times) => "time",
        (_, Phrase::Times) => "times",

        (1, Phrase::TimeInFiles) => "time in",
        (_, Phrase::TimeInFiles) => "time in", // Note: this case shouldn't occur in practice

        (1, Phrase::TimesInFiles) => "times in", // Note: this case shouldn't occur in practice
        (_, Phrase::TimesInFiles) => "times in",
    }
}

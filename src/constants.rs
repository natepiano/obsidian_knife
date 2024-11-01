// only enable file updates during tests for now
// pub const ENABLE_FILE_UPDATES: bool = cfg!(test);

// processing stuff
pub const ERROR_DETAILS: &str = "**error details:**";
pub const ERROR_DURATION: &str = "total processing time before error:";
pub const ERROR_OCCURRED: &str = "error occurred";
pub const ERROR_SOURCE: &str = "**error source:**";
pub const ERROR_TYPE: &str = "error type:";
pub const FORMAT_TIME_STAMP: &str = "%Y-%m-%d %H:%M:%S";
pub const MODE_APPLY_CHANGES: &str = "changes will be applied";
pub const MODE_DRY_RUN: &str = "dry run - no changes will be applied";
pub const PROCESSING_DURATION: &str = "total processing time:";
pub const PROCESSING_FINAL_MESSAGE: &str = "obsidian_knife made the cut using:";
pub const PROCESSING_START: &str = "starting obsidian_knife";
pub const SECONDS: &str = "seconds";
pub const USAGE: &str = "usage: obsidian_knife <obsidian_folder/config_file.md>";
pub const YAML_TIMESTAMP: &str = "time_stamp: ";
pub const YAML_APPLY_CHANGES: &str = "apply_changes: ";

// config stuff
pub const DEFAULT_OUTPUT_FOLDER: &str = "obsidian_knife";
pub const ERROR_NOT_FOUND: &str = "config file not found: ";
pub const ERROR_READING: &str = "error reading config file ";
pub const ERROR_BACK_POPULATE_FILE_FILTER: &str = "back_populate_filter_filter cannot be empty";
pub const ERROR_OUTPUT_FOLDER: &str = "output_folder cannot be empty";

// cache stuff
pub const CACHE_FOLDER: &str = ".obsidian_knife";
pub const CACHE_FILE: &str = "obsidian_knife_cache.json";

// image stuff
pub const IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "png", "jpeg", "tiff", "pdf", "gif"];
pub const IMAGE_ALT_TEXT_DEFAULT: &str = "image";
pub const MISSING_IMAGE_REFERENCES: &str = "missing image references";
pub const SECTION_IMAGE_CLEANUP: &str = "image cleanup";
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

// wikilink back populate
pub const BACK_POPULATE_TABLE_HEADER_PREFIX: &str = "the following";
pub const BACK_POPULATE_TABLE_HEADER_MIDDLE: &str = "in";
pub const BACK_POPULATE_TABLE_HEADER_SUFFIX: &str = "will be back populated";

pub const BACK_POPULATE_SECTION_PREFIX: &str = "back populate";
pub const BACK_POPULATE_SECTION_SUFFIX: &str = "wikilinks";
pub const BACK_POPULATE_FILE_FILTER_PREFIX: &str =
    "using back_populate_file_filter config parameter: ";
pub const BACK_POPULATE_FILE_FILTER_SUFFIX: &str =
    "remove it from config if you want to process all files";
pub const MATCHES_AMBIGUOUS: &str = "ambiguous matches found - these will be skipped";
pub const MATCHES_UNAMBIGUOUS: &str = "matches found to back populate";

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

        // Occurrence-related phrases
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

// Add new helper function for compound pluralization
pub fn pluralize_occurrence_in_files(occurrences: usize, file_count: usize) -> String {
    // We want "time" for 1, "times" for other numbers
    let occurrence_word = pluralize(occurrences, Phrase::Times);

    // Format as "time(s) in file(s)"
    format!("{} {} in {} {}",
            occurrences,
            occurrence_word,
            file_count,
            pluralize(file_count, Phrase::Files)
    )
}

pub fn format_back_populate_header(match_count: usize, file_count: usize) -> String {
    format!("{} {} {} {} {} {}",
            BACK_POPULATE_TABLE_HEADER_PREFIX,
            match_count,
            pluralize(match_count, Phrase::Matches),
            BACK_POPULATE_TABLE_HEADER_MIDDLE,
            file_count,
            pluralize(file_count, Phrase::Files))
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

    #[test]
    fn test_compound_pluralization() {
        let test_cases = [
            (1, 1, "1 time in 1 file"),
            (1, 2, "1 time in 2 files"),
            (2, 1, "2 times in 1 file"),
            (2, 2, "2 times in 2 files"),
            (0, 0, "0 times in 0 files"),
        ];

        for (occurrences, file_count, expected) in test_cases {
            assert_eq!(
                pluralize_occurrence_in_files(occurrences, file_count),
                expected,
                "Failed for {} occurrence(s) in {} file(s)",
                occurrences,
                file_count
            );
        }
    }

    #[test]
    fn test_back_populate_header_format() {
        let test_cases = [
            (1, 1, "the following 1 match in 1 file will be back populated"),
            (1, 2, "the following 1 match in 2 files will be back populated"),
            (2, 1, "the following 2 matches in 1 file will be back populated"),
            (2, 2, "the following 2 matches in 2 files will be back populated"),
            (0, 0, "the following 0 matches in 0 files will be back populated"),
        ];

        for (match_count, file_count, expected) in test_cases {
            let result = format!("{} {}",
                                 format_back_populate_header(match_count, file_count),
                                 BACK_POPULATE_TABLE_HEADER_SUFFIX);
            assert_eq!(result, expected,
                       "Failed for {} match(es) in {} file(s)",
                       match_count,
                       file_count);
        }
    }
}

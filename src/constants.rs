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
pub const FOUND: &str = "found";
pub const FRONTMATTER: &str = "frontmatter";
pub const FRONTMATTER_ISSUES: &str = "frontmatter issues";
pub const IN: &str = "in";
pub const INVALID: &str = "invalid";
pub const INVALID_WIKILINKS: &str = "invalid wikilinks";
pub const MISSING_IMAGE: &str = "missing image";
pub const NO_RENDER: &str = "- these won't render in obsidian";
pub const NOT_VALID: &str = "- these are probably corrupted";
pub const OF: &str = "of";
pub const THAT_NEED_UPDATES: &str = "that need updates will be saved";
pub const TIFF: &str = "TIFF";
pub const WILL_BE_BACK_POPULATED: &str = "will be back populated";
pub const ZERO_BYTE: &str = "zero-byte";

// wikilink back populate
pub const BACK_POPULATE_TABLE_HEADER_MIDDLE: &str = "in";
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
    File(usize),
    Has(usize),
    Image(usize),
    Issue(usize),
    Match(usize),
    Reference(usize),
    Wikilink(usize),
    With(usize),
}

impl Phrase {
    pub const fn pluralize(&self) -> &'static str {
        match self {
            Phrase::File(1) => "file",
            Phrase::File(_) => "files",
            Phrase::Has(1) => "has a",
            Phrase::Has(_) => "have",
            Phrase::Image(1) => "image",
            Phrase::Image(_) => "images",
            Phrase::Issue(1) => "issue",
            Phrase::Issue(_) => "issues",
            Phrase::Match(1) => "match",
            Phrase::Match(_) => "matches",
            Phrase::Reference(1) => "reference",
            Phrase::Reference(_) => "references",
            Phrase::Wikilink(1) => "wikilink",
            Phrase::Wikilink(_) => "wikilinks",
            Phrase::With(1) => "with a",
            Phrase::With(_) => "with",
        }
    }

    pub const fn value(&self) -> usize {
        match self {
            Phrase::File(value)
            | Phrase::Has(value)
            | Phrase::Image(value)
            | Phrase::Issue(value)
            | Phrase::Match(value)
            | Phrase::Reference(value)
            | Phrase::Wikilink(value)
            | Phrase::With(value) => *value,
        }
    }
}

#[derive(Default)]
pub struct DescriptionBuilder {
    parts: Vec<String>,
}

impl DescriptionBuilder {
    /// Creates a new DescriptionBuilder instance.
    pub fn new() -> Self {
        Self { parts: Vec::new() }
    }

    pub fn number(mut self, number: usize) -> Self {
        self.parts.push(number.to_string());
        self
    }

    /// Appends text to the builder.
    pub fn text(mut self, text: &str) -> Self {
        self.parts.push(text.to_string());
        self
    }

    pub fn pluralize_with_count(mut self, phrase_new: Phrase) -> Self {
        self.parts
            .push(format!("{} {}", phrase_new.value(), phrase_new.pluralize()));
        self
    }

    pub fn pluralize(mut self, phrase: Phrase) -> Self {
        self.parts.push(phrase.pluralize().to_string());
        self
    }

    /// Builds the final string with all appended parts, adding a newline at the end.
    pub fn build(self) -> String {
        let mut result = self.parts.join(" ");
        result.push('\n');
        result
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PhraseOld {
    // File-related phrases
    Files,

    // Image-related phrases
    UnreferencedImages,
    DuplicateImages,

    // compound pluralize
    Times,
}
/// Pluralizes a phrase based on count at compile time
pub const fn pluralize(count: usize, phrase: PhraseOld) -> &'static str {
    match (count, phrase) {
        (1, PhraseOld::Files) => "file",
        (_, PhraseOld::Files) => "files",

        (1, PhraseOld::UnreferencedImages) => "image is not referenced by any file",
        (_, PhraseOld::UnreferencedImages) => "images are not referenced by any files",

        (1, PhraseOld::DuplicateImages) => "duplicate image",
        (_, PhraseOld::DuplicateImages) => "duplicate images",

        (1, PhraseOld::Times) => "time",
        (_, PhraseOld::Times) => "times",
    }
}

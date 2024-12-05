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
// the DEFAULT_MEDIA_PATH could be a configuration parameter as it's really specific to my repo
pub const DEFAULT_MEDIA_PATH: &str = "conf/media";
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

//markdown outline levels
pub const LEVEL1: &str = "#";
pub const LEVEL2: &str = "##";
pub const LEVEL3: &str = "###";
pub const LEVEL4: &str = "####";

// matching stuff
pub const CLOSING_BRACKET: char = ']';
pub const CLOSING_WIKILINK: &str = "]]";
pub const EXTENSION_MARKDOWN: &str = ".md";
pub const EXTENSION_TIFF: &str = "tiff";
pub const FORWARD_SLASH: char = '/';
pub const IMAGE_ALT_TEXT_DEFAULT: &str = "image";
pub const IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "png", "jpeg", "tiff", "pdf", "gif"];
pub const OPENING_BRACKET: char = '[';
pub const OPENING_IMAGE_LINK_BRACKET: &str = "!["; // for external style "![]()"
pub const OPENING_IMAGE_WIKILINK_BRACKET: &str = "![[";
pub const OPENING_PAREN: char = '(';
pub const OPENING_WIKILINK: &str = "[[";

// report &str's
pub const AFTER: &str = "after";
pub const BACK_POPULATE: &str = "back populate";
pub const BACK_POPULATE_FILE_FILTER_PREFIX: &str =
    "using back_populate_file_filter config parameter: ";
pub const BACK_POPULATE_FILE_FILTER_SUFFIX: &str =
    "remove it from config if you want to process all files";
pub const BEFORE: &str = "before";
pub const COLON: &str = ":";
pub const CONFIG_EXPECT: &str = "ValidatedConfig required for this report";
pub const DUPLICATE: &str = "duplicate";
pub const DUPLICATES: &str = "duplicates";
pub const DUPLICATE_IMAGES_WITH_REFERENCES: &str = "duplicate images with references";
pub const FILE: &str = "file";
pub const FOUND: &str = "found";
pub const FRONTMATTER: &str = "frontmatter";
pub const FRONTMATTER_ISSUES: &str = "frontmatter issues";
pub const IMAGES: &str = "images";
pub const IN: &str = "in";
pub const INFO: &str = "info";
pub const INVALID: &str = "invalid";
pub const INVALID_WIKILINKS: &str = "invalid wikilinks";
pub const MATCHES: &str = "matches";
pub const MATCHES_AMBIGUOUS: &str = "ambiguous matches found - these will be skipped";
pub const MATCHES_UNAMBIGUOUS: &str = "matches found";
pub const MISSING_IMAGE: &str = "missing image";
pub const MISSING_IMAGE_REFERENCES: &str = "missing image references";
pub const NOT_REFERENCED_BY_ANY_FILE: &str = "not referenced by any file";
pub const NOT_VALID: &str = "- these are probably corrupted";
pub const NO_RENDER: &str = "- these won't render in obsidian";
pub const OCCURRENCES: &str = "occurrences";
pub const OF: &str = "of";
pub const PATH: &str = "path";
pub const PERSIST_REASON: &str = "persist reason";
pub const REFERENCED_BY: &str = "referenced by";
pub const REFERENCE_REMOVED: &str = " - reference removed";
pub const REFERENCE_WILL_BE_REMOVED: &str = " - reference will be removed";
pub const SAMPLE: &str = "sample";
pub const SOURCE_TEXT: &str = "source text";
pub const TEXT: &str = "text";
pub const THAT_NEED_UPDATES: &str = "that need updates will be saved";
pub const TIFF: &str = "TIFF";
pub const TIFF_IMAGES: &str = "TIFF images";
pub const UNREFERENCED_IMAGES: &str = "unreferenced images";
pub const UPDATED: &str = " - updated";
pub const WIKILINKS: &str = "wikilinks";
pub const WILL_BE_BACK_POPULATED: &str = "will be back populated";
pub const WILL_BE_UPDATED: &str = " - will be updated";
pub const WILL_REPLACE_WITH: &str = "will replace with";
pub const ZERO_BYTE: &str = "zero-byte";

#[derive(Debug, Clone, Copy)]
pub enum Phrase {
    File(usize),
    Has(usize),
    Image(usize),
    Issue(usize),
    Is(usize),
    Match(usize),
    Reference(usize),
    Target(usize),
    Time(usize),
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
            Phrase::Is(1) => "is",
            Phrase::Is(_) => "are",
            Phrase::Match(1) => "match",
            Phrase::Match(_) => "matches",
            Phrase::Reference(1) => "reference",
            Phrase::Reference(_) => "references",
            Phrase::Target(1) => "target",
            Phrase::Target(_) => "targets",
            Phrase::Time(1) => "time",
            Phrase::Time(_) => "times",
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
            | Phrase::Is(value)
            | Phrase::Match(value)
            | Phrase::Reference(value)
            | Phrase::Target(value)
            | Phrase::Time(value)
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

    pub fn no_space(mut self, text: &str) -> Self {
        if !self.parts.is_empty() {
            // If we have previous parts, directly append to the last one
            let last = self.parts.last_mut().unwrap();
            last.push_str(text);
        } else {
            // If this is the first part, just push it normally
            self.parts.push(text.to_string());
        }
        self
    }

    pub fn quoted_text(mut self, text: &str) -> Self {
        let quoted = format!("\"{}\"", text);
        self.parts.push(quoted);
        self
    }

    pub fn parenthetical_text(mut self, text: &str) -> Self {
        let parenthesized = format!("({})", text);
        self.parts.push(parenthesized);
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
        self.parts.join(" ")
    }
}

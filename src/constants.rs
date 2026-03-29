// processing stuff
#[cfg(debug_assertions)]
pub const DEV: &str = "dev";
pub const ERROR_DETAILS: &str = "error details:";
pub const ERROR_OCCURRED: &str = "error occurred";
pub const ERROR_SOURCE: &str = "error source:";
pub const ERROR_TYPE: &str = "error type:";
pub const FORMAT_TIME_STAMP: &str = "%Y-%m-%d %H:%M:%S";
pub const MILLISECONDS: &str = "ms";
pub const MODE_APPLY_CHANGES: &str = "apply changes is on - changes will be applied";
pub const MODE_APPLY_CHANGES_OFF: &str = "apply changes is off - no changes will be applied";
pub const OBSIDIAN_KNIFE: &str = "obsidian knife - aka \"ok\"";
#[cfg(not(debug_assertions))]
pub const RELEASE: &str = "release";
pub const TOTAL_TIME: &str = "total time";
pub const USAGE: &str = "usage: obsidian_knife <obsidian_folder/config_file.md>";

// config stuff
// the DEFAULT_MEDIA_PATH could be a configuration parameter as it's really specific to my repo
pub const DEFAULT_MEDIA_PATH: &str = "conf/media";
pub const DEFAULT_OUTPUT_FOLDER: &str = "obsidian_knife";
pub const DEFAULT_TIMEZONE: &str = "America/New_York";
pub const ERROR_NOT_FOUND: &str = "file not found: ";
pub const ERROR_READING: &str = "error reading config file ";
pub const OUTPUT_MARKDOWN_FILE: &str = "obsidian knife output.md";

// cache stuff
pub const CACHE_FOLDER: &str = ".ok";
pub const CACHE_FILE: &str = "obsidian_knife_cache.json";

//markdown outline levels
pub const LEVEL1: &str = "#";
pub const LEVEL2: &str = "##";
pub const LEVEL3: &str = "###";

// files and extensions
pub const DS_STORE: &str = ".DS_Store";
pub const IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "png", "jpeg", "tiff", "pdf", "gif"];
pub const MARKDOWN_EXTENSION: &str = "md";
pub const MARKDOWN_SUFFIX: &str = ".md";

// matching stuff
pub const CLOSING_WIKILINK: &str = "]]";
pub const FORWARD_SLASH: char = '/';
pub const OPENING_BRACKET: char = '[';
pub const OPENING_PAREN: char = '(';
pub const OPENING_WIKILINK: &str = "[[";

// report &str's
pub const ACTION: &str = "action";
pub const ADD_FRONTMATTER: &str = "add frontmatter";
pub const AFTER: &str = "after";
pub const BACK_POPULATE: &str = "back populate";
pub const BACK_POPULATE_FILE_FILTER_PREFIX: &str =
    "using back_populate_file_filter config parameter: ";
pub const BACK_POPULATE_FILE_FILTER_SUFFIX: &str =
    "remove it from config if you want to process all files";
pub const BEFORE: &str = "before";
pub const COLON: &str = ":";
pub const CONFIG_EXPECT: &str = "ValidatedConfig required for this report";
pub const DELETED: &str = "deleted";
pub const DUPLICATE: &str = "duplicate";
pub const DUPLICATE_IMAGES: &str = "duplicate images";
pub const FILE: &str = "file";
pub const FOUND: &str = "found";
pub const FRONTMATTER: &str = "frontmatter";
pub const FRONTMATTER_ISSUES: &str = "frontmatter issues";
pub const IMAGES: &str = "images";
pub const IMAGE_FILE: &str = "image file";
pub const IMAGE_FILE_HASH: &str = "image file hash";
pub const IN: &str = "in";
pub const INFO: &str = "info";
pub const INVALID: &str = "invalid";
pub const INVALID_WIKILINKS: &str = "invalid wikilinks";
pub const IN_CHANGESET: &str = "in changeset";
pub const LINE: &str = "line";
pub const MATCHES: &str = "matches";
pub const MATCHES_AMBIGUOUS: &str = "ambiguous matches";
pub const MISSING_IMAGE: &str = "missing image";
pub const MISSING_IMAGE_REFERENCES: &str = "files that refer to images that don't exist";
pub const NOT_REFERENCED: &str = "not referenced";
pub const NO_CHANGE: &str = "no change";
pub const OCCURRENCES: &str = "occurrences";
pub const OF: &str = "of";
pub const PATH: &str = "path";
pub const POSITION: &str = "position";
pub const REASON: &str = "reason";
pub const REFERENCED_BY: &str = "referenced by";
pub const REFERENCE_CHANGE: &str = "reference change";
pub const REFERENCE_REMOVED: &str = " - reference removed";
pub const REFERENCE_WILL_BE_REMOVED: &str = "reference will be removed";
pub const SOURCE_TEXT: &str = "source text";
pub const TEXT: &str = "text";
pub const THUMBNAIL: &str = "thumbnail";
pub const TIFF: &str = "TIFF";
pub const TYPE: &str = "type";
pub const UNKNOWN: &str = "unknown";
pub const UNREFERENCED_IMAGES: &str = "unreferenced images";
pub const UPDATE: &str = "update";
pub const WIKILINKS: &str = "wikilinks";
pub const WILL_BE_BACK_POPULATED: &str = "will be back populated";
pub const WILL_DELETE: &str = "will delete";
pub const WILL_REPLACE_WITH: &str = "will replace with";
pub const YAML_APPLY_CHANGES: &str = "apply_changes: ";
pub const YAML_FILE_LIMIT: &str = "file_limit: ";
pub const YAML_TIMESTAMP_LOCAL: &str = "local_time: ";
pub const YAML_TIMESTAMP_UTC: &str = "utc_time: ";
pub const YOU_HAVE_TO_FIX_THESE_YOURSELF: &str = "you have to fix these yourself";
pub const ZERO_BYTE: &str = "zero-byte";

// report image handling
pub const THUMBNAIL_WIDTH: usize = 50;

#[derive(Debug, Clone, Copy)]
pub enum Phrase {
    File(usize),
    Has(usize),
    Image(usize),
    Issue(usize),
    Is(usize),
    Match(usize),
    Reason(usize),
    Reference(usize),
    Target(usize),
    Time(usize),
    Wikilink(usize),
    With(usize),
}

impl Phrase {
    pub const fn pluralize(&self) -> &'static str {
        match self {
            Self::File(1) => "file",
            Self::File(_) => "files",
            Self::Has(1) => "has a",
            Self::Has(_) => "have",
            Self::Image(1) => "image",
            Self::Image(_) => "images",
            Self::Issue(1) => "issue",
            Self::Issue(_) => "issues",
            Self::Is(1) => "is",
            Self::Is(_) => "are",
            Self::Match(1) => "match",
            Self::Match(_) => "matches",
            Self::Reason(1) => "reason",
            Self::Reason(_) => "reasons",
            Self::Reference(1) => "reference",
            Self::Reference(_) => "references",
            Self::Target(1) => "target",
            Self::Target(_) => "targets",
            Self::Time(1) => "time",
            Self::Time(_) => "times",
            Self::Wikilink(1) => "wikilink",
            Self::Wikilink(_) => "wikilinks",
            Self::With(1) => "with a",
            Self::With(_) => "with",
        }
    }

    pub const fn value(&self) -> usize {
        match self {
            Self::File(value)
            | Self::Has(value)
            | Self::Image(value)
            | Self::Issue(value)
            | Self::Is(value)
            | Self::Match(value)
            | Self::Reason(value)
            | Self::Reference(value)
            | Self::Target(value)
            | Self::Time(value)
            | Self::Wikilink(value)
            | Self::With(value) => *value,
        }
    }
}

#[derive(Default)]
pub struct DescriptionBuilder {
    parts: Vec<String>,
}

impl DescriptionBuilder {
    /// Creates a new `DescriptionBuilder` instance.
    pub const fn new() -> Self { Self { parts: Vec::new() } }

    pub fn text_with_newline(mut self, text: &str) -> Self {
        let new_text = format!("{}{}", text, "\n");
        self.parts.push(new_text);
        self
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
        if self.parts.is_empty() {
            // If this is the first part, just push it normally
            self.parts.push(text.to_string());
        } else if let Some(last) = self.parts.last_mut() {
            // If we have previous parts, directly append to the last one
            last.push_str(text);
        }
        self
    }

    pub fn quoted_text(mut self, text: &str) -> Self {
        let quoted = format!("\"{text}\"");
        self.parts.push(quoted);
        self
    }

    pub fn parenthetical_text(mut self, text: &str) -> Self {
        let parenthesized = format!("({text})");
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
    pub fn build(self) -> String { self.parts.join(" ") }
}

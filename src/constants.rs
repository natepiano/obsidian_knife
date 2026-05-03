// CLI invocation
/// Program name plus a single config-file argument.
pub(crate) const EXPECTED_ARG_COUNT: usize = 2;
/// Index of the config-file argument in `std::env::args()`.
pub(crate) const CONFIG_ARG_INDEX: usize = 1;
/// Exit code used when regex compilation fails at startup.
pub(crate) const INVALID_REGEX_EXIT_CODE: i32 = 1;

// Cache
pub(crate) const CACHE_FILE: &str = "obsidian_knife_cache.json";
pub(crate) const CACHE_FOLDER: &str = ".ok";
pub(crate) const SHA256_BUFFER_SIZE: usize = 1024;

// Config
/// the `DEFAULT_MEDIA_PATH` could be a configuration parameter as it's really specific to my repo
pub(crate) const DEFAULT_MEDIA_PATH: &str = "conf/media";
pub(crate) const DEFAULT_OUTPUT_FOLDER: &str = "obsidian_knife";
pub(crate) const DEFAULT_TIMEZONE: &str = "America/New_York";
pub(crate) const ERROR_NOT_FOUND: &str = "file not found: ";
pub(crate) const ERROR_READING: &str = "error reading config file ";
pub(crate) const OUTPUT_MARKDOWN_FILE: &str = "obsidian knife output.md";

// Date handling
pub(crate) const FORMAT_DATE: &str = "%Y-%m-%d";
pub(crate) const NOON_HOUR: u32 = 12;

// Files and extensions
pub(crate) const DS_STORE: &str = ".DS_Store";
pub(crate) const IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "png", "jpeg", "tiff", "pdf", "gif"];
pub(crate) const MARKDOWN_EXTENSION: &str = "md";
pub(crate) const MARKDOWN_SUFFIX: &str = ".md";

// Folders
pub(crate) const OBSIDIAN_FOLDER: &str = ".obsidian";

// Frontmatter delimiters
pub(crate) const YAML_CLOSING_DELIMITER: &str = "---\n";
pub(crate) const YAML_CLOSING_DELIMITER_EOF: &str = "\n---";
pub(crate) const YAML_CLOSING_DELIMITER_NEWLINE: &str = "\n---\n";
pub(crate) const YAML_OPENING_DELIMITER: &str = "---\n";

// Markdown link syntax
pub(crate) const IMAGE_LINK_PREFIX: &str = "![";
pub(crate) const MARKDOWN_LINK_SEPARATOR: &str = "](";

// Markdown outline levels
pub(crate) const LEVEL1: &str = "#";
pub(crate) const LEVEL2: &str = "##";
pub(crate) const LEVEL3: &str = "###";

// Matching
pub(crate) const CLOSING_WIKILINK: &str = "]]";
pub(crate) const FORWARD_SLASH: char = '/';
pub(crate) const OPENING_BRACKET: char = '[';
pub(crate) const OPENING_PAREN: char = '(';
pub(crate) const OPENING_WIKILINK: &str = "[[";

// Processing
#[cfg(debug_assertions)]
pub(crate) const DEV: &str = "dev";
pub(crate) const ERROR_DETAILS: &str = "error details:";
pub(crate) const ERROR_OCCURRED: &str = "error occurred";
pub(crate) const ERROR_SOURCE: &str = "error source:";
pub(crate) const ERROR_TYPE: &str = "error type:";
pub(crate) const FORMAT_TIME_STAMP: &str = "%Y-%m-%d %H:%M:%S";
pub(crate) const MILLISECONDS: &str = "ms";
pub(crate) const MODE_APPLY_CHANGES: &str = "apply changes is on - changes will be applied";
pub(crate) const MODE_APPLY_CHANGES_OFF: &str = "apply changes is off - no changes will be applied";
pub(crate) const OBSIDIAN_KNIFE: &str = "obsidian knife - aka \"ok\"";
#[cfg(not(debug_assertions))]
pub(crate) const RELEASE: &str = "release";
pub(crate) const TOTAL_TIME: &str = "total time";
pub(crate) const USAGE: &str = "usage: obsidian_knife <obsidian_folder/config_file.md>";

// Report image handling
pub(crate) const REPORT_CHUNK_SIZE: usize = 500;
pub(crate) const THUMBNAIL_WIDTH: usize = 50;

// Report strings
pub(crate) const ACTION: &str = "action";
pub(crate) const ADD_FRONTMATTER: &str = "add frontmatter";
pub(crate) const AFTER: &str = "after";
pub(crate) const BACK_POPULATE: &str = "back populate";
pub(crate) const BACK_POPULATE_FILE_FILTER_PREFIX: &str =
    "using back_populate_file_filter config parameter: ";
pub(crate) const BACK_POPULATE_FILE_FILTER_SUFFIX: &str =
    "remove it from config if you want to process all files";
pub(crate) const BEFORE: &str = "before";
pub(crate) const COLON: &str = ":";
pub(crate) const DELETED: &str = "deleted";
pub(crate) const DUPLICATE: &str = "duplicate";
pub(crate) const DUPLICATE_IMAGES: &str = "duplicate images";
pub(crate) const FILE: &str = "file";
pub(crate) const FOUND: &str = "found";
pub(crate) const FRONTMATTER: &str = "frontmatter";
pub(crate) const FRONTMATTER_ISSUES: &str = "frontmatter issues";
pub(crate) const IMAGE_FILE: &str = "image file";
pub(crate) const IMAGE_FILE_HASH: &str = "image file hash";
pub(crate) const IMAGES: &str = "images";
pub(crate) const IN: &str = "in";
pub(crate) const IN_CHANGESET: &str = "in changeset";
pub(crate) const INCOMPATIBLE_IMAGES: &str = "incompatible images";
pub(crate) const INFO: &str = "info";
pub(crate) const INVALID: &str = "invalid";
pub(crate) const INVALID_WIKILINKS: &str = "invalid wikilinks";
pub(crate) const KEEPER: &str = "keeper";
pub(crate) const LINE: &str = "line";
pub(crate) const MATCHES: &str = "matches";
pub(crate) const MATCHES_AMBIGUOUS: &str = "ambiguous matches";
pub(crate) const MISSING_IMAGE: &str = "missing image";
pub(crate) const MISSING_IMAGE_REFERENCES: &str = "files that refer to images that don't exist";
pub(crate) const NO_CHANGE: &str = "no change";
pub(crate) const NOT_REFERENCED: &str = "not referenced";
pub(crate) const OCCURRENCES: &str = "occurrences";
pub(crate) const OF: &str = "of";
pub(crate) const PATH: &str = "path";
pub(crate) const POSITION: &str = "position";
pub(crate) const REASON: &str = "reason";
pub(crate) const REFERENCED_BY: &str = "referenced by";
pub(crate) const REFERENCE_CHANGE: &str = "reference change";
pub(crate) const REFERENCE_REMOVED: &str = " - reference removed";
pub(crate) const REFERENCE_WILL_BE_REMOVED: &str = "reference will be removed";
pub(crate) const SOURCE_TEXT: &str = "source text";
pub(crate) const TEXT: &str = "text";
pub(crate) const THUMBNAIL: &str = "thumbnail";
pub(crate) const TIFF: &str = "TIFF";
pub(crate) const TYPE: &str = "type";
pub(crate) const UNKNOWN: &str = "unknown";
pub(crate) const UNREFERENCED_IMAGES: &str = "unreferenced images";
pub(crate) const UPDATE: &str = "update";
pub(crate) const WIKILINKS: &str = "wikilinks";
pub(crate) const WILL_BE_BACK_POPULATED: &str = "will be back populated";
pub(crate) const WILL_DELETE: &str = "will delete";
pub(crate) const WILL_REPLACE_WITH: &str = "will replace with";
pub(crate) const YAML_APPLY_CHANGES: &str = "apply_changes: ";
pub(crate) const YAML_FILE_LIMIT: &str = "file_limit: ";
pub(crate) const YAML_TIMESTAMP_LOCAL: &str = "local_time: ";
pub(crate) const YAML_TIMESTAMP_UTC: &str = "utc_time: ";
pub(crate) const YOU_HAVE_TO_FIX_THESE_YOURSELF: &str = "you have to fix these yourself";
pub(crate) const ZERO_BYTE: &str = "zero-byte";

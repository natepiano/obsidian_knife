// macos file dates
#[cfg(target_os = "macos")]
pub(super) const SET_FILE_CREATED_DATE_FLAG: &str = "-d";
#[cfg(target_os = "macos")]
pub(super) const SET_FILE_CREATED_DATE_FORMAT: &str = "%m/%d/%Y %H:%M:%S";
#[cfg(target_os = "macos")]
pub(super) const SET_FILE_EXECUTABLE: &str = "SetFile";

// matching
pub(super) const ESCAPED_BRACKET_CLOSE: &str = r"\]";
pub(super) const ESCAPED_BRACKET_OPEN: &str = r"\[";

// messages
#[cfg(target_os = "macos")]
pub(super) const FAILED_TO_SET_CREATION_DATE_WITH_SETFILE: &str =
    "Failed to set creation date with SetFile";
pub(super) const IMAGE_FILE_COLLECTION_LOCK_POISONED: &str = "image file collection lock poisoned";
pub(super) const INVALID_REGEX_PATTERN: &str = "invalid regex pattern";

// paths
pub(super) const HOME_ENV_VAR: &str = "HOME";
pub(super) const TILDE: &str = "~";
pub(super) const TILDE_SLASH: &str = "~/";

// regex
pub(super) const CASE_INSENSITIVE_WORD_PATTERN_PREFIX: &str = r"(?i)\b";
pub(super) const CASE_INSENSITIVE_WORD_PATTERN_SUFFIX: &str = r"\b";
pub(super) const EMAIL_PATTERN: &str = r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}";
pub(super) const IMAGE_EXTENSIONS_SEPARATOR: &str = "|";
pub(super) const MARKDOWN_LINK_PATTERN: &str = r"\[.*?\]\(.*?\)";
pub(super) const RAW_HTTP_PATTERN: &str = r"https?://[^\s]+";
pub(super) const TAG_PATTERN: &str = r"(?:^|\s)(#[a-zA-Z0-9_-]+)";

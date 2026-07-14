// highlighting
pub(super) const HIGHLIGHT_CLOSE_TAG: &str = "</span>";
pub(super) const HIGHLIGHT_EXTRA_TAG_CAPACITY_MULTIPLIER: usize = 2;
pub(super) const HIGHLIGHT_OPEN_TAG: &str = "<span style=\"color: red;\">";

// messages
pub(super) const AMBIGUOUS_MATCH_GROUP_EMPTY: &str =
    "ambiguous match group must contain at least one match";
pub(super) const BACK_POPULATE_MATCH_GROUP_EMPTY: &str =
    "back-populate match group must contain at least one match";
pub(super) const DUPLICATE_IMAGES_REPORT_CONFIG_REQUIRED: &str =
    "ValidatedConfig required for duplicate-images report";
pub(super) const FILES_TO_BE_UPDATED: &str = "files to be updated";
pub(super) const INCOMPATIBLE_IMAGES_REPORT_CONFIG_REQUIRED: &str =
    "ValidatedConfig required for incompatible-images report";
pub(super) const INCOMPATIBLE_IMAGES_REPORT_INVARIANT: &str =
    "Only incompatible images should be in this report";
pub(super) const INVALID_UTF8_BOUNDARY_DETECTED: &str =
    "Invalid UTF-8 boundary detected at position";
pub(super) const MISSING_REFERENCES_REPORT_CONFIG_REQUIRED: &str =
    "ValidatedConfig required for missing-references report";

// table columns
pub(super) const FILE_COLUMN_INDEX: usize = 0;
pub(super) const IMAGE_PATH_COLUMN_INDEX: usize = 1;
pub(super) const LINE_NUMBER_COLUMN_INDEX: usize = 1;
pub(super) const UNPARSABLE_LINE_NUMBER_SORT_KEY: usize = 0;

// table headers
pub(super) const TABLE_HEADER_ERROR_MESSAGE: &str = "error message";
pub(super) const TABLE_HEADER_FILE_NAME: &str = "file name";
pub(super) const TABLE_HEADER_INVALID_REASON: &str = "invalid reason";
pub(super) const TABLE_HEADER_LINE: &str = "line";
pub(super) const TABLE_HEADER_LINE_TEXT: &str = "line text";
pub(super) const TABLE_HEADER_SOURCE_TEXT: &str = "source text";

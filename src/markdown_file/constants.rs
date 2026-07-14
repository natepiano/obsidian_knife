// code delimiters
pub(super) const FENCED_CODE_DELIMITER: &str = "```";
pub(super) const INLINE_CODE_DELIMITER: char = '`';

// image links
pub(super) const HTTP_URL_PREFIX: &str = "http://";
pub(super) const HTTPS_URL_PREFIX: &str = "https://";
pub(super) const IMAGE_LINK_SIZE_PARAMETER_INDEX: usize = 1;
pub(super) const IMAGE_LINK_WHOLE_MATCH_CAPTURE_INDEX: usize = 0;
pub(super) const INVALID_IMAGE_LINK_FORMAT_PREFIX: &str =
    "invalid image link format passed to ImageLink::new: ";

// matching
pub(super) const APOSTROPHE: char = '\'';
pub(super) const MAX_OBSIDIAN_LINK_PIPE_COUNT: usize = 2;
pub(super) const RIGHT_SINGLE_QUOTATION_MARK: char = '\u{2019}';
pub(super) const T_LOWER: char = 't';
pub(super) const T_UPPER: char = 'T';
pub(super) const UNDERSCORE: char = '_';

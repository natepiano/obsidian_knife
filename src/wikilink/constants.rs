// invalid wikilink reasons
pub(super) const INVALID_WIKILINK_DOUBLE_ALIAS: &str = "contains multiple alias separators";
pub(super) const INVALID_WIKILINK_EMAIL_ADDRESS: &str =
    "ignore email addresses for back population";
pub(super) const INVALID_WIKILINK_EMPTY: &str = "contains empty wikilink";
pub(super) const INVALID_WIKILINK_NESTED_OPENING: &str = "contains a nested opening";
pub(super) const INVALID_WIKILINK_PREFIX: &str = "Invalid wikilink at line";
pub(super) const INVALID_WIKILINK_RAW_HTTP_LINK: &str = "ignore raw web links";
pub(super) const INVALID_WIKILINK_TAG: &str = "ignore tags for back population";
pub(super) const INVALID_WIKILINK_UNCLOSED_INLINE_CODE: &str = "contains unclosed inline code";
pub(super) const INVALID_WIKILINK_UNMATCHED_CLOSING: &str =
    "contains unmatched closing brackets ']]'";
pub(super) const INVALID_WIKILINK_UNMATCHED_MARKDOWN_LINK_OPENING: &str =
    "'[' without following match";
pub(super) const INVALID_WIKILINK_UNMATCHED_OPENING: &str =
    "contains unmatched opening brackets '[['";
pub(super) const INVALID_WIKILINK_UNMATCHED_SINGLE: &str = "contains unmatched bracket '[' or ']'";

// syntax
pub(super) const EMPTY_WIKILINK: &str = "[[]]";
pub(super) const MARKDOWN_CLICKABLE_IMAGE_PREFIX: &str = "[!";
pub(super) const WIKILINK_FINDER_PATTERN: &str = r"\[\[.*?\]\]";

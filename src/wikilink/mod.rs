mod constants;
mod link;
mod wikilink_parser;

pub use link::InvalidWikilink;
pub use link::InvalidWikilinkReason;
pub use link::ToWikilink;
pub use link::Wikilink;
pub use wikilink_parser::create_filename_wikilink;
pub use wikilink_parser::extract_wikilinks;
pub use wikilink_parser::is_wikilink;
pub use wikilink_parser::is_within_wikilink;

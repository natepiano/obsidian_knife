mod constants;
mod link;
mod parser;

pub use link::InvalidWikilink;
pub use link::InvalidWikilinkReason;
pub use link::ToWikilink;
pub use link::Wikilink;
pub use parser::SpannedWikilink;
pub use parser::create_filename_wikilink;
pub use parser::extract_wikilinks;
pub use parser::is_wikilink;
pub use parser::is_within_wikilink;

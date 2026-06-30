use std::cmp::Ordering;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;

use serde::Deserialize;
use serde::Serialize;

use crate::constants::PIPE;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Wikilink {
    pub display_text: String,
    pub target:       String,
}

impl Wikilink {
    pub fn is_alias(&self) -> bool { self.display_text != self.target }
}

impl PartialOrd for Wikilink {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

impl Ord for Wikilink {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .display_text
            .len()
            .cmp(&self.display_text.len())
            .then(self.display_text.cmp(&other.display_text))
            .then_with(|| match (self.is_alias(), other.is_alias()) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => self.target.cmp(&other.target),
            })
    }
}

impl Display for Wikilink {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.is_alias() {
            write!(f, "{}{PIPE}{}", self.target, self.display_text)
        } else {
            f.write_str(&self.target)
        }
    }
}

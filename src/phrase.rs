#[derive(Debug, Clone, Copy)]
pub(crate) enum Phrase {
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
    pub(crate) const fn pluralize(&self) -> &'static str {
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

    pub(crate) const fn value(&self) -> usize {
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

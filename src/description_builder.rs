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

#[derive(Default)]
pub(crate) struct DescriptionBuilder {
    parts: Vec<String>,
}

impl DescriptionBuilder {
    /// Creates a new `DescriptionBuilder` instance.
    pub(crate) const fn new() -> Self { Self { parts: Vec::new() } }

    pub(crate) fn text_with_newline(mut self, text: &str) -> Self {
        let new_text = format!("{text}\n");
        self.parts.push(new_text);
        self
    }

    pub(crate) fn number(mut self, number: usize) -> Self {
        self.parts.push(number.to_string());
        self
    }

    /// Appends text to the builder.
    pub(crate) fn text(mut self, text: &str) -> Self {
        self.parts.push(text.to_string());
        self
    }

    pub(crate) fn no_space(mut self, text: &str) -> Self {
        if self.parts.is_empty() {
            // If this is the first part, just push it normally
            self.parts.push(text.to_string());
        } else if let Some(last) = self.parts.last_mut() {
            // If we have previous parts, directly append to the last one
            last.push_str(text);
        }
        self
    }

    pub(crate) fn quoted_text(mut self, text: &str) -> Self {
        let quoted = format!("\"{text}\"");
        self.parts.push(quoted);
        self
    }

    pub(crate) fn parenthetical_text(mut self, text: &str) -> Self {
        let parenthesized = format!("({text})");
        self.parts.push(parenthesized);
        self
    }

    pub(crate) fn pluralize_with_count(mut self, phrase_new: Phrase) -> Self {
        self.parts
            .push(format!("{} {}", phrase_new.value(), phrase_new.pluralize()));
        self
    }

    pub(crate) fn pluralize(mut self, phrase: Phrase) -> Self {
        self.parts.push(phrase.pluralize().to_string());
        self
    }

    /// Builds the final string with all appended parts, adding a newline at the end.
    pub(crate) fn build(self) -> String { self.parts.join(" ") }
}

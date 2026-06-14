use crate::phrase::Phrase;

#[derive(Default)]
pub(crate) struct DescriptionBuilder {
    parts: Vec<String>,
}

impl DescriptionBuilder {
    /// Initializes an empty `DescriptionBuilder`.
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
            // Empty `DescriptionBuilder.parts` stores `text` as the first part.
            self.parts.push(text.to_string());
        } else if let Some(last) = self.parts.last_mut() {
            // Existing `DescriptionBuilder.parts` append `text` to the last part.
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

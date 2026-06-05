mod escaping;

pub(super) fn escape_brackets(text: &str) -> String { escaping::escape_brackets(text) }

pub(super) fn escape_pipe(text: &str) -> String { escaping::escape_pipe(text) }

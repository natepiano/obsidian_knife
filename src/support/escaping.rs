use crate::constants::BACKSLASH;
use crate::constants::BACKSLASH_PARITY_DIVISOR;
use crate::constants::CLOSING_BRACKET;
use crate::constants::ESCAPED_BRACKET_CLOSE;
use crate::constants::ESCAPED_BRACKET_OPEN;
use crate::constants::ESCAPED_PIPE_CAPACITY_MULTIPLIER;
use crate::constants::OPENING_BRACKET;
use crate::constants::PIPE;

// Helper function to escape pipes in Markdown strings
pub fn escape_pipe(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() * ESCAPED_PIPE_CAPACITY_MULTIPLIER);
    let chars: Vec<char> = text.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == PIPE {
            // Count the number of consecutive backslashes before '|'
            let mut backslash_count = 0;
            let mut j = i;
            while j > 0 && chars[j - 1] == BACKSLASH {
                backslash_count += 1;
                j -= 1;
            }

            // If even number of backslashes, '|' is not escaped
            if backslash_count % BACKSLASH_PARITY_DIVISOR == 0 {
                escaped.push(BACKSLASH);
            }
            escaped.push(PIPE);
        } else {
            escaped.push(ch);
        }
        i += 1;
    }

    escaped
}

// Helper function to escape pipes and brackets for visual verification
pub fn escape_brackets(text: &str) -> String {
    text.replace(OPENING_BRACKET, ESCAPED_BRACKET_OPEN)
        .replace(CLOSING_BRACKET, ESCAPED_BRACKET_CLOSE)
}

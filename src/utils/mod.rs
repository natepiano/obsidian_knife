// mod vec_enum_filter;
mod file_utils;
mod output_file_writer;
mod regex_utils;
mod sha256_cache;
mod timer;

pub use file_utils::*;
pub use output_file_writer::*;
pub use regex_utils::*;
pub use sha256_cache::*;
pub use timer::Timer;

// Helper function to escape pipes in Markdown strings
pub fn escape_pipe(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() * 2);
    let chars: Vec<char> = text.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '|' {
            // Count the number of consecutive backslashes before '|'
            let mut backslash_count = 0;
            let mut j = i;
            while j > 0 && chars[j - 1] == '\\' {
                backslash_count += 1;
                j -= 1;
            }

            // If even number of backslashes, '|' is not escaped
            if backslash_count % 2 == 0 {
                escaped.push('\\');
            }
            escaped.push('|');
        } else {
            escaped.push(ch);
        }
        i += 1;
    }

    escaped
}

// Helper function to escape pipes and brackets for visual verification
pub fn escape_brackets(text: &str) -> String {
    text.replace('[', r"\[").replace(']', r"\]")
}

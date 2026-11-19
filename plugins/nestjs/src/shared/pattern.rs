//! Regular expressions read from options.

use regex_automata::meta::Regex;

/// Compiles the pattern of the option `option`.
///
/// # Errors
/// When the pattern is no regular expression.
pub fn compile(option: &str, pattern: &str) -> gate_sdk::Result<Regex> {
    Regex::new(pattern).map_err(|failure| {
        gate_sdk::Error::config(format!(
            "the {option} {pattern:?} is no regular expression ({failure}); fix the option"
        ))
    })
}

/// `text` with every character that has a meaning in a regular expression escaped.
#[must_use]
pub fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());

    for character in text.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }

    escaped
}

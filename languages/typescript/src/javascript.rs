//! JavaScript text semantics: white space as JavaScript's `\s` and `trim` define it.

/// Whether JavaScript's `\s` and `String.prototype.trim` treat `character` as white space. It
/// differs from Rust's `char::is_whitespace` in U+FEFF (white space here) and U+0085 (not).
#[must_use]
pub fn is_space(character: char) -> bool {
    matches!(
        character,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// JavaScript `String.prototype.trim`.
#[must_use]
pub fn trim(text: &str) -> &str {
    text.trim_matches(is_space)
}

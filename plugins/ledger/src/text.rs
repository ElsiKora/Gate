//! Text semantics the ledger rules share: the white space set, the pattern dialect, fenced-code
//! blanking that keeps offsets, line numbers, and lexical path joins.

use std::fmt::Write as _;

use regex::Regex;

/// Whether `character` is white space for the ledger: tab, line feed, U+000B, U+000C, carriage
/// return, space, U+00A0, U+1680, U+2000 to U+200A, U+2028, U+2029, U+202F, U+205F, U+3000 and
/// U+FEFF. It differs from Rust's `char::is_whitespace` in U+FEFF (white space here) and U+0085
/// (not).
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

/// `text` without leading and trailing white space ([`is_space`]).
#[must_use]
pub fn trim(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// The members of [`is_space`] as a Rust class body: what `\s` matches in a pattern.
const SPACE_CLASS: &str = r"\t\n\x0B\x0C\r \u{A0}\u{1680}\u{2000}-\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}\u{FEFF}";

/// A pattern of the plugin or of an option. Its classes are ASCII `\d`, `\w` and `\b`, and `\s` is
/// [`is_space`]. Inline flags such as `(?i)` and `(?m)` are accepted.
///
/// # Errors
/// Fails, with a one-line reason, when the pattern does not compile.
pub fn pattern(source: &str) -> Result<Regex, String> {
    let translated = translate(source, false)?;
    Regex::new(&translated).map_err(|error| one_line(&error.to_string()))
}

/// `text` as a pattern that matches it literally, escaping only the syntax characters [`pattern`]
/// reads as escapes.
#[must_use]
pub fn literal(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());

    for character in text.chars() {
        if r"^$\.*+?()[]{}|/".contains(character) {
            escaped.push('\\');
        }

        escaped.push(character);
    }

    escaped
}

/// Compiles a manifest pattern: the classes of [`pattern`] with the `u` and `m` flags, so `^` and
/// `$` match at every line, `\r` included as a line end, and an escape of a character that is not
/// pattern syntax is an error. Inline flag groups, look-around and back-references are not
/// supported.
///
/// # Errors
/// Fails, with a one-line reason, when the pattern uses what the dialect does not have or does not
/// compile.
pub fn multiline_pattern(source: &str) -> Result<Regex, String> {
    let translated = translate(source, true)?;
    Regex::new(&format!("(?mR){translated}")).map_err(|error| one_line(&error.to_string()))
}

/// The last line of a regex error, without its `error: ` prefix.
fn one_line(error: &str) -> String {
    error
        .lines()
        .last()
        .unwrap_or_default()
        .trim_start_matches("error: ")
        .to_owned()
}

/// Rewrites the classes of the dialect for the Rust engine: ASCII `\d`, `\w`, `\b` and `\B`,
/// [`is_space`] for `\s`, and a backspace for `\b` inside a class. Nested classes keep each rewrite
/// valid inside a character class.
///
/// Rejects an escape of a character that is not pattern syntax and, for a `multiline` source, a
/// `(?` group other than non-capturing, look-around or named.
fn translate(source: &str, multiline: bool) -> Result<String, String> {
    let mut translated = String::new();
    let mut characters = source.chars().peekable();
    let mut in_class = false;

    while let Some(character) = characters.next() {
        match character {
            '\\' => escape(&mut characters, &mut translated, in_class)?,
            '(' if multiline && !in_class && characters.peek() == Some(&'?') => {
                characters.next();
                translated.push_str("(?");
                let mut flags = String::new();

                while let Some(flag) = characters.next_if(|flag| "ims-".contains(*flag)) {
                    flags.push(flag);
                }

                match characters.peek() {
                    Some(':') => translated.push_str(&flags),
                    Some('=' | '!' | '<') if flags.is_empty() => {}
                    _ => return Err("invalid group".to_owned()),
                }
            }
            '[' if !in_class => {
                in_class = true;

                if characters.next_if_eq(&'^').is_some() {
                    if characters.next_if_eq(&']').is_some() {
                        // `[^]` matches any character.
                        in_class = false;
                        translated.push_str("(?s:.)");
                    } else {
                        translated.push_str("[^");
                    }
                } else if characters.next_if_eq(&']').is_some() {
                    // `[]` matches nothing.
                    in_class = false;
                    translated.push_str("[a&&b]");
                } else {
                    translated.push('[');
                }
            }
            ']' if in_class => {
                in_class = false;
                translated.push(character);
            }
            ']' if multiline => return Err("lone ]".to_owned()),
            // Inside a class of the dialect these are literals; the Rust engine reads them as nested
            // classes and set operators.
            '[' | '&' | '~' if in_class => {
                translated.push('\\');
                translated.push(character);
            }
            '-' if in_class && characters.peek() == Some(&'-') => translated.push_str(r"\-"),
            other => translated.push(other),
        }
    }

    Ok(translated)
}

/// Translates the escape after a backslash, accepting only the escapes of the `u` flag.
fn escape(
    characters: &mut std::iter::Peekable<std::str::Chars<'_>>,
    translated: &mut String,
    in_class: bool,
) -> Result<(), String> {
    let Some(escaped) = characters.next() else {
        translated.push('\\');
        return Ok(());
    };

    match escaped {
        'd' => translated.push_str("[0-9]"),
        'D' => translated.push_str("[^0-9]"),
        'w' => translated.push_str("[0-9A-Za-z_]"),
        'W' => translated.push_str("[^0-9A-Za-z_]"),
        's' => {
            let _ = write!(translated, "[{SPACE_CLASS}]");
        }
        'S' => {
            let _ = write!(translated, "[^{SPACE_CLASS}]");
        }
        'b' if in_class => translated.push_str(r"\x08"),
        'b' => translated.push_str(r"(?-u:\b)"),
        'B' if !in_class => translated.push_str(r"(?-u:\B)"),
        '0' if !characters.peek().is_some_and(char::is_ascii_digit) => {
            translated.push_str(r"\x00");
        }
        'c' if characters.peek().is_some_and(char::is_ascii_alphabetic) => {
            let letter = characters.next().map_or(0, u32::from);
            let _ = write!(translated, r"\x{{{:02X}}}", letter % 32);
        }
        'x' => {
            let digits: String = (0..2)
                .filter_map(|_| characters.next_if(char::is_ascii_hexdigit))
                .collect();

            if digits.len() != 2 {
                return Err(r"invalid escape \x".to_owned());
            }

            let _ = write!(translated, r"\x{digits}");
        }
        'u' if characters.peek() == Some(&'{') => {
            translated.push_str(r"\u");

            for next in characters.by_ref() {
                translated.push(next);

                if next == '}' {
                    break;
                }
            }
        }
        'u' => {
            let digits: String = (0..4)
                .filter_map(|_| characters.next_if(char::is_ascii_hexdigit))
                .collect();

            if digits.len() != 4 {
                return Err(r"invalid escape \u".to_owned());
            }

            let _ = write!(translated, r"\u{{{digits}}}");
        }
        'p' | 'P' if characters.peek() == Some(&'{') => {
            translated.push('\\');
            translated.push(escaped);
        }
        // Control and syntax escapes pass through; back-references reach Rust, which rejects them.
        'f'
        | 'n'
        | 'r'
        | 't'
        | 'v'
        | '1'..='9'
        | 'k'
        | '^'
        | '$'
        | '\\'
        | '.'
        | '*'
        | '+'
        | '?'
        | '('
        | ')'
        | '['
        | ']'
        | '{'
        | '}'
        | '|'
        | '/' => {
            translated.push('\\');
            translated.push(escaped);
        }
        '-' if in_class => translated.push_str(r"\-"),
        other => return Err(format!("invalid escape \\{other}")),
    }

    Ok(())
}

/// Relative path segments joined with `/`, lexically: empty and `.` parts dropped, a `..` removing
/// the part before it (a leading `..` stays), and the trailing `/` of the last segment kept, so a
/// file path written with one does not name the file.
#[must_use]
pub fn join(segments: &[&str]) -> String {
    let mut parts: Vec<&str> = Vec::new();

    for part in segments.iter().flat_map(|segment| segment.split('/')) {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }

    let mut joined = parts.join("/");

    if !joined.is_empty() && segments.last().is_some_and(|last| last.ends_with('/')) {
        joined.push('/');
    }

    joined
}

/// `path` in normal form, when it is relative: [`join`] of the path alone, without a trailing `/`.
/// An option path must be written in this form, since the tree names each file one way only.
/// `None` for an absolute path.
#[must_use]
pub fn normal_path(path: &str) -> Option<String> {
    if path.starts_with('/') {
        return None;
    }
    Some(join(&[path.trim_end_matches('/')]))
}

/// The extension of the last path component, from its last dot; empty when the name has no dot
/// or starts with its only one.
#[must_use]
pub fn extension(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);

    match name.rfind('.') {
        Some(index) if index > 0 => &name[index..],
        _ => "",
    }
}

/// Maps byte offsets to 1-based line numbers.
#[derive(Debug)]
pub struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    /// The index of `text`.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(offset, _)| offset + 1));
        Self { starts }
    }

    /// The 1-based line of a byte offset.
    #[must_use]
    pub fn line(&self, offset: usize) -> u32 {
        let index = self.starts.partition_point(|start| *start <= offset);
        u32::try_from(index).unwrap_or(u32::MAX)
    }
}

/// Blanks fenced code blocks, and optionally HTML comments, with spaces of the same byte length,
/// so offsets and line numbers stay those of the original text.
#[must_use]
pub fn blank_fenced(content: &str, blank_comments: bool) -> String {
    let mut fence: Option<(u8, usize)> = None;
    let mut in_comment = false;
    let mut lines = Vec::new();

    for original in content.split('\n') {
        if let Some((character, length)) = fence {
            if let Some(closing) = closing_marker(original)
                && closing.as_bytes().first() == Some(&character)
                && closing.len() >= length
            {
                fence = None;
            }

            lines.push(" ".repeat(original.len()));
            continue;
        }

        let mut line = original.to_owned();

        if blank_comments {
            let mut offset = 0;

            while offset < line.len() {
                let start = if in_comment {
                    Some(offset)
                } else {
                    line[offset..].find("<!--").map(|found| offset + found)
                };
                let Some(start) = start else {
                    break;
                };
                let search = if in_comment { start } else { start + 4 };
                let end = line
                    .get(search..)
                    .and_then(|rest| rest.find("-->"))
                    .map(|found| search + found);
                let next = end.map_or(line.len(), |end| end + 3);
                line.replace_range(start..next, &" ".repeat(next - start));
                in_comment = end.is_none();
                offset = next;
            }
        }

        if let Some((character, length)) = opening_marker(&line) {
            fence = Some((character, length));
            lines.push(" ".repeat(line.len()));
            continue;
        }

        lines.push(line);
    }

    lines.join("\n")
}

/// The marker of a closing fence line: up to three spaces, a run of backticks or tildes, and
/// only spaces or tabs after it.
fn closing_marker(line: &str) -> Option<&str> {
    let rest = strip_indent(line)?;
    let character = *rest.as_bytes().first()?;

    if character != b'`' && character != b'~' {
        return None;
    }

    let length = rest.bytes().take_while(|byte| *byte == character).count();
    rest[length..]
        .bytes()
        .all(|byte| byte == b' ' || byte == b'\t')
        .then(|| &rest[..length])
}

/// The marker character and length of an opening fence line: up to three spaces and a run of at
/// least three backticks or tildes.
fn opening_marker(line: &str) -> Option<(u8, usize)> {
    let rest = strip_indent(line)?;
    let character = *rest.as_bytes().first()?;

    if character != b'`' && character != b'~' {
        return None;
    }

    let length = rest.bytes().take_while(|byte| *byte == character).count();
    (length >= 3).then_some((character, length))
}

fn strip_indent(line: &str) -> Option<&str> {
    let spaces = line.bytes().take_while(|byte| *byte == b' ').count();
    (spaces <= 3).then(|| &line[spaces..])
}

#[cfg(test)]
mod tests {
    use super::{blank_fenced, extension, join, multiline_pattern, trim};

    #[test]
    fn blanking_keeps_offsets_and_closes_only_a_matching_fence() {
        let text = "a\n````md\n```\nx\n````\nb <!-- c\nd --> e\n";
        let blanked = blank_fenced(text, true);
        assert_eq!(blanked.len(), text.len());
        assert_eq!(blanked, "a\n      \n   \n \n    \nb       \n      e\n");
    }

    #[test]
    fn multiline_patterns_read_ascii_classes_per_line() {
        let pattern = multiline_pattern(r"^\| B-\d{3} \|").expect("pattern compiles");
        assert_eq!(
            pattern
                .find_iter("| B-001 |\r\n| B-\u{660}\u{660}\u{662} |\n| B-003 |")
                .count(),
            2
        );
        for rejected in [
            "[", r"^B\:\d+", "^B-(?i)x", r"^B\z", r"^\AB", r"\a", r"^\pL", r"^\x{42}", "B]",
            "(?i)b",
        ] {
            assert!(multiline_pattern(rejected).is_err(), "{rejected}");
        }
        for (accepted, text) in [
            ("^[[(]B", "(B"),
            ("^(?i:b)", "B"),
            ("^[^]", "x"),
            (r"^\0", "\0"),
            (r"^\cJ", "\n"),
            ("^[a&&b]", "&"),
            (r"^\u0042\u{43}", "BC"),
            (r"^(?:\| B-[\d-]\.\*)$", "| B-1.*"),
        ] {
            let pattern = multiline_pattern(accepted).expect(accepted);
            assert!(pattern.is_match(text), "{accepted}");
        }
        assert!(
            !multiline_pattern("[]")
                .expect("an empty class compiles")
                .is_match("x")
        );
        let boundary = multiline_pattern(r"\bB-\d{3}\b").expect("pattern compiles");
        assert!(boundary.is_match("| \u{e9}B-001 |"));
        assert!(!boundary.is_match("| B-0012 |"));
        let space = multiline_pattern(r"^C-\d{3}[\s]x").expect("pattern compiles");
        assert!(space.is_match("C-001\u{feff}x"));
        assert!(!space.is_match("C-001\u{85}x"));
    }

    #[test]
    fn paths_join_lexically_and_trim_drops_white_space() {
        assert_eq!(join(&["docs/ledger", "../a.md/"]), "docs/a.md/");
        assert_eq!(join(&["../../docs", "a.md"]), "../../docs/a.md");
        assert_eq!(extension("docs/a.md"), ".md");
        assert_eq!(extension("docs/.md"), "");
        assert_eq!(extension("docs/README"), "");
        assert_eq!(trim("\u{feff} a\u{85}\u{a0}"), "a\u{85}");
    }
}

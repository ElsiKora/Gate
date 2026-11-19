//! Regular expressions of `commitlint`'s and `git-branch-lint`'s configuration, read as JavaScript
//! reads one without flags (with the web-compatibility grammar of Annex B), and written for the
//! `regex` crate. Every pattern a git rule takes from a JavaScript tool's policy is read here.
//!
//! Each construct is translated to the one that means the same here, so that no construct of the
//! `regex` crate's own syntax (`\A`, `\z`, `\<`, `\pL`, `[[:alpha:]]`, `&&` and `--` in a class, a
//! nested class) can take effect: JavaScript reads those as literal characters, and so does the
//! translation. A construct JavaScript has and the `regex` crate cannot match is refused by name.

use std::fmt::Write as _;

/// `.` of JavaScript, which matches no line terminator.
pub(crate) const ANY: &str = r"[^\n\r\x{2028}\x{2029}]";
const DIGIT: &str = "[0-9]";
const NOT_DIGIT: &str = "[^0-9]";
const WORD: &str = "[0-9A-Za-z_]";
const NOT_WORD: &str = "[^0-9A-Za-z_]";
/// `\s` of JavaScript: Unicode white space without U+0085, and U+FEFF.
const SPACE: &str = r"[[\s&&[^\x{85}]]\x{FEFF}]";
const NOT_SPACE: &str = r"[^[\s&&[^\x{85}]]\x{FEFF}]";

/// `pattern`, a JavaScript regular expression without flags, written for the `regex` crate.
///
/// # Errors
/// Names the first construct the translation does not read: a lookaround, a backreference, an
/// octal escape, an inline flag group, an empty class, a lone UTF-16 surrogate, a class that is
/// not closed, a trailing `\` or a `\c` without a letter.
pub(crate) fn translate(pattern: &str) -> Result<String, String> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len() * 2);
    let mut at = 0;
    while let Some(&character) = chars.get(at) {
        at += 1;
        match character {
            '\\' => at = escape(&chars, at, &mut out)?,
            '[' => at = class(&chars, at, &mut out)?,
            '(' => at = group(&chars, at, &mut out)?,
            '{' => at = brace(&chars, at, &mut out),
            '.' => out.push_str(ANY),
            ']' | '}' => literal(&mut out, character),
            _ => out.push(character),
        }
    }
    Ok(out)
}

/// The escape whose `\` is just before `at`, outside a class; returns where the pattern goes on.
fn escape(chars: &[char], at: usize, out: &mut String) -> Result<usize, String> {
    let set = match chars.get(at) {
        None => return Err(r"a trailing \".to_owned()),
        Some('b') => r"(?-u:\b)",
        Some('B') => r"(?-u:\B)",
        Some(&escaped) => {
            if let Some(set) = class_escape(escaped) {
                set
            } else {
                let (character, next) = character_escape(chars, at, false)?;
                literal(out, character);
                return Ok(next);
            }
        }
    };
    out.push_str(set);
    Ok(at + 1)
}

/// The class `\d`, `\D`, `\w`, `\W`, `\s` or `\S` that `escaped` names, as JavaScript reads it.
fn class_escape(escaped: char) -> Option<&'static str> {
    Some(match escaped {
        'd' => DIGIT,
        'D' => NOT_DIGIT,
        'w' => WORD,
        'W' => NOT_WORD,
        's' => SPACE,
        'S' => NOT_SPACE,
        _ => return None,
    })
}

/// The one character an escape stands for, its letter at `at`, and where the pattern goes on. An
/// escape JavaScript gives no meaning stands for its own character (`\A` is `A`, `\<` is `<`).
fn character_escape(chars: &[char], at: usize, in_class: bool) -> Result<(char, usize), String> {
    let Some(&escaped) = chars.get(at) else {
        return Err(r"a trailing \".to_owned());
    };
    let next = at + 1;
    let followed_by_digit = chars.get(next).is_some_and(char::is_ascii_digit);
    Ok(match escaped {
        'n' => ('\n', next),
        'r' => ('\r', next),
        't' => ('\t', next),
        'v' => ('\u{b}', next),
        'f' => ('\u{c}', next),
        'b' if in_class => ('\u{8}', next),
        '0' if !followed_by_digit => ('\0', next),
        '0'..='9' => return Err(format!(r"the backreference or octal escape \{escaped}")),
        'k' => return Err(r"the named backreference \k".to_owned()),
        'c' => match chars.get(next).copied().filter(char::is_ascii_alphabetic) {
            Some(letter) => (control(letter), next + 1),
            None => return Err(r"\c without a letter A to Z after it".to_owned()),
        },
        'x' => match hex(chars, next, 2).and_then(char::from_u32) {
            Some(character) => (character, next + 2),
            None => ('x', next),
        },
        'u' => match hex(chars, next, 4) {
            Some(unit) => unicode(chars, next + 4, unit)?,
            None => ('u', next),
        },
        other => (other, next),
    })
}

/// The control character `\c` and `letter` stand for.
fn control(letter: char) -> char {
    char::from_u32(u32::from(letter) % 32).unwrap_or('\0')
}

/// The character `\u` and the four hex digits `unit` stand for, the low half of a surrogate pair
/// read from the `\u` at `next` when `unit` is the high half.
fn unicode(chars: &[char], next: usize, unit: u32) -> Result<(char, usize), String> {
    if let Some(character) = char::from_u32(unit) {
        return Ok((character, next));
    }
    let escaped_u = chars.get(next) == Some(&'\\') && chars.get(next + 1) == Some(&'u');
    let low = if escaped_u {
        hex(chars, next + 2, 4).filter(|low| (0xDC00..=0xDFFF).contains(low))
    } else {
        None
    };
    match low {
        Some(low) if (0xD800..=0xDBFF).contains(&unit) => {
            let code = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
            char::from_u32(code)
                .map(|character| (character, next + 6))
                .ok_or_else(|| format!(r"the UTF-16 surrogate \u{unit:04X}"))
        }
        _ => Err(format!(r"the lone UTF-16 surrogate \u{unit:04X}")),
    }
}

/// The value of the `count` hex digits at `at`, when there are that many.
fn hex(chars: &[char], at: usize, count: usize) -> Option<u32> {
    let mut value = 0;
    for digit in chars.get(at..at + count)? {
        value = value * 16 + digit.to_digit(16)?;
    }
    Some(value)
}

/// One member of a class before ranges are read.
enum Atom {
    Character(char),
    /// A `-` as written, which can join a range.
    Dash,
    /// `\d`, `\w`, `\s` or a negation, which cannot.
    Set(&'static str),
}

impl Atom {
    fn character(&self) -> Option<char> {
        match self {
            Self::Character(character) => Some(*character),
            Self::Dash => Some('-'),
            Self::Set(_) => None,
        }
    }

    fn write(&self, out: &mut String) {
        match self {
            Self::Set(set) => out.push_str(set),
            _ => literal(out, self.character().unwrap_or('-')),
        }
    }
}

/// The class whose `[` is just before `at`; returns where the pattern goes on. Every character in
/// it is a literal, as in JavaScript, so `[`, `&&`, `--`, `~~` and `[:alpha:]` are characters.
fn class(chars: &[char], mut at: usize, out: &mut String) -> Result<usize, String> {
    let negated = chars.get(at) == Some(&'^');
    if negated {
        at += 1;
    }
    let mut atoms = Vec::new();
    loop {
        let Some(&character) = chars.get(at) else {
            return Err("a [ whose class is not closed".to_owned());
        };
        at += 1;
        match character {
            ']' => break,
            '-' => atoms.push(Atom::Dash),
            '\\' => {
                if let Some(set) = chars.get(at).copied().and_then(class_escape) {
                    atoms.push(Atom::Set(set));
                    at += 1;
                } else {
                    let (escaped, next) = character_escape(chars, at, true)?;
                    atoms.push(Atom::Character(escaped));
                    at = next;
                }
            }
            other => atoms.push(Atom::Character(other)),
        }
    }
    if atoms.is_empty() {
        return Err("the empty class [] or [^]".to_owned());
    }
    out.push('[');
    if negated {
        out.push('^');
    }
    let mut index = 0;
    while let Some(first) = atoms.get(index) {
        let (Some(Atom::Dash), Some(last)) = (atoms.get(index + 1), atoms.get(index + 2)) else {
            first.write(out);
            index += 1;
            continue;
        };
        match (first.character(), last.character()) {
            (Some(low), Some(high)) if low > high => {
                return Err(format!(
                    "the range {low}-{high}, whose ends are out of order"
                ));
            }
            (Some(low), Some(high)) => {
                literal(out, low);
                out.push('-');
                literal(out, high);
            }
            // Annex B: a class escape at either end makes the `-` a character.
            _ => {
                first.write(out);
                literal(out, '-');
                last.write(out);
            }
        }
        index += 3;
    }
    out.push(']');
    Ok(at)
}

/// The group whose `(` is just before `at`; returns where the pattern goes on.
fn group(chars: &[char], at: usize, out: &mut String) -> Result<usize, String> {
    if chars.get(at) != Some(&'?') {
        out.push('(');
        return Ok(at);
    }
    match (chars.get(at + 1), chars.get(at + 2)) {
        (Some(':'), _) => {
            out.push_str("(?:");
            Ok(at + 2)
        }
        (Some('<'), Some(next)) if *next != '=' && *next != '!' => {
            out.push_str("(?<");
            Ok(at + 2)
        }
        (Some('=' | '!'), _) | (Some('<'), Some('=' | '!')) => {
            Err("a lookaround (?=, (?!, (?<= or (?<!".to_owned())
        }
        _ => Err("a group (? other than (?: and (?<name>".to_owned()),
    }
}

/// The `{` just before `at`: a quantifier `{n}`, `{n,}` or `{n,m}` stays one; any other `{` is a
/// character, as JavaScript reads it.
fn brace(chars: &[char], at: usize, out: &mut String) -> usize {
    let low = digits(chars, at);
    if low > 0 {
        let mut end = at + low;
        if chars.get(end) == Some(&',') {
            end += 1 + digits(chars, end + 1);
        }
        if let (Some('}'), Some(quantifier)) = (chars.get(end), chars.get(at - 1..=end)) {
            out.extend(quantifier);
            return end + 1;
        }
    }
    literal(out, '{');
    at
}

/// How many ASCII digits start at `at`.
fn digits(chars: &[char], at: usize) -> usize {
    let mut count = 0;
    while chars.get(at + count).is_some_and(char::is_ascii_digit) {
        count += 1;
    }
    count
}

/// `character` as a literal of the `regex` crate, inside a class or outside one.
fn literal(out: &mut String, character: char) {
    if character.is_ascii_alphanumeric() || character == ' ' {
        out.push(character);
    } else {
        let _ = write!(out, "\\x{{{:X}}}", u32::from(character));
    }
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use super::translate;

    /// Whether `pattern`, translated, matches `text`.
    fn matches(pattern: &str, text: &str) -> bool {
        let translated = translate(pattern).expect("translates");
        Regex::new(&translated)
            .unwrap_or_else(|error| panic!("{pattern} as {translated}: {error}"))
            .is_match(text)
    }

    #[test]
    fn angle_bracket_escapes_are_characters_not_word_boundaries() {
        assert!(matches(r"^\<a\>$", "<a>"));
        assert!(!matches(r"\<a\>", "a"));
    }

    #[test]
    fn backslash_a_and_z_escapes_are_letters_not_assertions() {
        assert!(matches(r"^\Aa\z\Z$", "AazZ"));
        assert!(!matches(r"\Aa\z", "a"));
    }

    #[test]
    fn backslash_p_escapes_are_letters_not_unicode_classes() {
        assert!(matches(r"^\pL\PL$", "pLPL"));
        assert!(!matches(r"^\pL$", "a"));
    }

    #[test]
    fn x_and_u_escapes_without_their_digits_are_letters() {
        // `{2}` after them is then a quantifier, as JavaScript reads it.
        assert!(matches(r"^\x{2}$", "xx"));
        assert!(matches(r"^\u{2}$", "uu"));
        assert!(matches(r"^\x41\u0042$", "AB"));
        assert!(matches(r"^\uD83D\uDE00$", "\u{1F600}"));
    }

    #[test]
    fn a_posix_class_is_characters_of_a_class() {
        // `[[:alpha:]]` is the class `[:alph` followed by `]`.
        assert!(matches(r"^[[:alpha:]]$", ":]"));
        assert!(matches(r"^[[:alpha:]]$", "[]"));
        assert!(!matches(r"^[[:alpha:]]$", "b"));
    }

    #[test]
    fn a_nested_bracket_is_a_character_of_the_class() {
        assert!(matches(r"^[[a]]$", "[]"));
        assert!(!matches(r"^[[a]]$", "a"));
    }

    #[test]
    fn a_double_ampersand_in_a_class_is_characters_not_an_intersection() {
        assert!(matches(r"^[a&&b]$", "&"));
        assert!(matches(r"^[a&&b]$", "b"));
    }

    #[test]
    fn a_double_tilde_in_a_class_is_characters_not_a_symmetric_difference() {
        assert!(matches(r"^[a~~b]$", "~"));
        assert!(matches(r"^[a~~b]$", "a"));
    }

    #[test]
    fn a_double_dash_in_a_class_is_a_range_not_a_difference() {
        // `%--` is the range `%` to `-`; `a--b` is a range out of order, refused.
        assert!(matches(r"^[%--]$", "+"));
        assert!(translate(r"[a--b]").is_err());
        // A class escape next to `-` makes the `-` a character (Annex B).
        assert!(matches(r"^[\w-.]+$", "a-."));
        assert!(!matches(r"^[\w-.]+$", "/"));
        // `\b` in a class is a backspace.
        assert!(matches(r"^[\b]$", "\u{8}"));
    }

    #[test]
    fn the_classes_and_dot_are_javascripts() {
        assert!(matches(r"^\w\d$", "a1"));
        assert!(!matches(r"^\w$", "\u{e9}"));
        assert!(!matches(r"^\d$", "\u{663}"));
        assert!(matches(r"^\s$", "\u{feff}"));
        assert!(!matches(r"^\s$", "\u{85}"));
        assert!(matches(r"^.$", "\u{85}"));
        assert!(!matches(r"^.$", "\u{2028}"));
        assert!(matches(r"\bx\b", "\u{e9}x\u{e9}"));
        // A `{` that is no quantifier is a character.
        assert!(matches(r"^a{,2}$", "a{,2}"));
        assert!(matches(r"^a{1,2}$", "aa"));
        assert!(matches(r"^{x}$", "{x}"));
    }

    #[test]
    fn what_the_regex_crate_cannot_match_is_refused_by_name() {
        let refused = |pattern: &str| translate(pattern).expect_err("refused");
        assert!(refused("a(?=b)").contains("lookaround"));
        assert!(refused("a(?<!b)").contains("lookaround"));
        assert!(refused(r"(a)\1").contains(r"\1"));
        assert!(refused(r"(?<x>a)\k<x>").contains(r"\k"));
        assert!(refused("(?i)a").contains("(?:"));
        assert!(refused("(?P<x>a)").contains("(?:"));
        assert!(refused("[]").contains("empty class"));
        assert!(refused("[^]").contains("empty class"));
        assert!(refused("[a").contains("not closed"));
        assert!(refused(r"a\").contains("trailing"));
        assert!(refused(r"\c1").contains(r"\c"));
        assert!(refused(r"\uD83D").contains("surrogate"));
    }
}

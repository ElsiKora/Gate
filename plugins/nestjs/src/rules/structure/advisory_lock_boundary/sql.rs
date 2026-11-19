//! The PostgreSQL lexer and executable-SQL analysis of the advisory-lock boundary.
//!
//! SQL is read as JavaScript strings, so text here is UTF-16 code units: positions,
//! character classes and lengths follow that representation, unpaired surrogates included. Strings
//! follow PostgreSQL's default `standard_conforming_strings = on`; `E'…'` strings decode their
//! escapes, byte escapes as UTF-8 bytes, and `U&'…'` and `U&"…"` lexemes their Unicode escapes with
//! an optional `UESCAPE`. A quoted string that follows another across a line break continues it.
//! The analysis finds the `pg_(try_)advisory_*` calls of a statement text and of the bodies it
//! executes (`DO` blocks, `CREATE FUNCTION` and `CREATE PROCEDURE` bodies in SQL or PL/pgSQL, and
//! PL/pgSQL `EXECUTE` of a literal), and the evidence the contract rejects.

/// UTF-16 code units, the representation of a JavaScript string.
pub type Text = Vec<u16>;

/// How deep executable bodies nest before the analysis stops.
const DEPTH_LIMIT: usize = 64;
/// How many bodies one analysis reads before it stops.
const WORK_LIMIT: i64 = 32_768;
/// The marker for an unresolved JavaScript template interpolation.
const INTERPOLATION: &str = "${dynamic}";

/// The kind of a lexeme.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Word,
    Identifier,
    String,
    Symbol,
}

/// One lexeme: its kind, its value (a word in lower case, a quoted lexeme decoded) and its span.
#[derive(Clone, Debug)]
pub struct Lexeme {
    pub kind: Kind,
    pub value: Text,
    pub start: usize,
    pub end: usize,
    /// The decoded bytes of an escape string with byte escapes.
    bytes: Option<Vec<u8>>,
    unicode: bool,
    escaped: bool,
    continuation: bool,
}

/// An advisory-lock call: its function name, whether `pg_catalog` qualifies it, and the source of
/// each argument.
#[derive(Clone, Debug)]
pub struct Call {
    pub name: Text,
    pub qualified: bool,
    pub arguments: Vec<Text>,
}

/// The advisory-lock calls of a SQL text and its executed bodies, whether it derives a key in
/// another way (`hashtext`, `md5` or a `::bit(64)` cast) and whether it holds an unresolved
/// interpolation.
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    pub calls: Vec<Call>,
    pub alternate_derivation: bool,
    pub interpolation: bool,
}

/// Why a SQL text cannot be read or is rejected; "invalid code point" stands for an escape above
/// U+10FFFF, which `String.fromCodePoint` rejects with a `RangeError`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqlError(pub Text);

impl SqlError {
    fn new(message: &str) -> Self {
        Self(units(message))
    }
}

impl std::fmt::Display for SqlError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&String::from_utf16_lossy(&self.0))
    }
}

impl std::error::Error for SqlError {}

type Result<T> = std::result::Result<T, SqlError>;

/// The UTF-16 code units of a string.
#[must_use]
pub fn units(text: &str) -> Text {
    text.encode_utf16().collect()
}

/// Whether a code unit is white space to a JavaScript `\s`, which is also what `trim` removes.
pub(crate) fn is_space(unit: u16) -> bool {
    matches!(
        unit,
        0x09..=0x0d
            | 0x20
            | 0xa0
            | 0x1680
            | 0x2000..=0x200a
            | 0x2028
            | 0x2029
            | 0x202f
            | 0x205f
            | 0x3000
            | 0xfeff
    )
}

fn is_ascii(unit: u16, predicate: fn(&u8) -> bool) -> bool {
    u8::try_from(unit).is_ok_and(|byte| predicate(&byte))
}

/// Whether a code unit starts a word: `[A-Za-z_0-9\u0080-￿]`.
fn is_word(unit: u16) -> bool {
    unit >= 0x80 || unit == u16::from(b'_') || is_ascii(unit, u8::is_ascii_alphanumeric)
}

fn is_high(unit: u16) -> bool {
    (0xd800..=0xdbff).contains(&unit)
}

fn is_low(unit: u16) -> bool {
    (0xdc00..=0xdfff).contains(&unit)
}

fn equals(text: &[u16], ascii: &str) -> bool {
    text.len() == ascii.len()
        && text
            .iter()
            .zip(ascii.bytes())
            .all(|(&unit, byte)| unit == u16::from(byte))
}

fn starts_with(text: &[u16], at: usize, ascii: &str) -> bool {
    text.get(at..at + ascii.len())
        .is_some_and(|slice| equals(slice, ascii))
}

/// The position of `needle` in `text` at or after `from`.
fn find(text: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(from.min(text.len()));
    }

    let last = text.len().checked_sub(needle.len())?;

    (from..=last).find(|&start| text[start..start + needle.len()] == *needle)
}

fn contains(text: &[u16], needle: &str) -> bool {
    find(text, &units(needle), 0).is_some()
}

/// Appends a code point as UTF-16, an unpaired surrogate as itself.
fn push_code_point(text: &mut Text, code_point: u32) -> Result<()> {
    if let Some(character) = char::from_u32(code_point) {
        let mut buffer = [0; 2];
        text.extend_from_slice(character.encode_utf16(&mut buffer));
    } else if let Ok(unit) = u16::try_from(code_point) {
        // An unpaired surrogate, which a JavaScript string may hold.
        text.push(unit);
    } else {
        return Err(SqlError::new("invalid code point"));
    }

    Ok(())
}

/// The UTF-8 bytes of a string, an unpaired surrogate as U+FFFD, as `Buffer.from` writes them.
fn utf8(text: &[u16]) -> Vec<u8> {
    char::decode_utf16(text.iter().copied())
        .map(|character| character.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect::<String>()
        .into_bytes()
}

/// The string of UTF-8 bytes, an invalid sequence as U+FFFD, as `Buffer.toString` reads them.
fn from_utf8(bytes: &[u8]) -> Text {
    String::from_utf8_lossy(bytes).encode_utf16().collect()
}

/// A word in lower case, as JavaScript's `toLowerCase` writes it; unpaired surrogates stay.
fn lower(text: &[u16]) -> Text {
    let mut lowered = Text::with_capacity(text.len());
    let mut run = Vec::new();

    for decoded in char::decode_utf16(text.iter().copied()) {
        match decoded {
            Ok(character) => run.push(character),
            Err(error) => {
                lowered.extend(
                    run.drain(..)
                        .collect::<String>()
                        .to_lowercase()
                        .encode_utf16(),
                );
                lowered.push(error.unpaired_surrogate());
            }
        }
    }

    lowered.extend(
        run.into_iter()
            .collect::<String>()
            .to_lowercase()
            .encode_utf16(),
    );
    lowered
}

fn hex(text: &[u16]) -> Option<u32> {
    let text = String::from_utf16(text).ok()?;

    (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(&text, 16).ok())
        .flatten()
}

/// The decoded value of a `U&` lexeme with its escape character.
fn decode_unicode(value: &[u16], escape: &[u16]) -> Result<Text> {
    let [escape] = escape else {
        return Err(SqlError::new("Unsupported SQL Unicode escape delimiter."));
    };

    if is_space(*escape)
        || is_ascii(*escape, u8::is_ascii_hexdigit)
        || b"+'\"".iter().any(|&byte| *escape == u16::from(byte))
    {
        return Err(SqlError::new("Unsupported SQL Unicode escape delimiter."));
    }

    let mut decoded = Text::new();
    let mut index = 0;

    while index < value.len() {
        if value[index] != *escape {
            decoded.push(value[index]);
        } else if value.get(index + 1) == Some(escape) {
            decoded.push(*escape);
            index += 1;
        } else {
            let wide = value.get(index + 1) == Some(&u16::from(b'+'));
            let width = if wide { 6 } else { 4 };
            let start = index + if wide { 2 } else { 1 };
            let digits = value.get(start..start + width).and_then(hex);
            let Some(code_point) = digits else {
                return Err(SqlError::new("Unresolved SQL Unicode escape."));
            };

            push_code_point(&mut decoded, code_point)?;
            index = start + width - 1;
        }

        index += 1;
    }

    Ok(decoded)
}

/// The length of the escape after a backslash in an `E'…'` string:
/// `[0-7]{1,3}|x[0-9a-fA-F]{1,2}|u[0-9a-fA-F]{4}|U[0-9a-fA-F]{8}|.` over code points.
fn escape_length(sql: &[u16], at: usize) -> Option<usize> {
    let run = |from: usize, most: usize, predicate: fn(&u8) -> bool| {
        sql[from.min(sql.len())..]
            .iter()
            .take(most)
            .take_while(|&&unit| is_ascii(unit, predicate))
            .count()
    };
    let first = *sql.get(at)?;
    let is_octal = |byte: &u8| (b'0'..=b'7').contains(byte);

    if is_ascii(first, is_octal) {
        return Some(run(at, 3, is_octal));
    }

    for (letter, digits) in [(b'x', 2), (b'u', 4), (b'U', 8)] {
        if first == u16::from(letter) {
            let count = run(at + 1, digits, u8::is_ascii_hexdigit);

            if letter == b'x' && count > 0 {
                return Some(1 + count);
            }

            if letter != b'x' && count == digits {
                return Some(1 + count);
            }
        }
    }

    Some(
        if is_high(first) && sql.get(at + 1).copied().is_some_and(is_low) {
            2
        } else {
            1
        },
    )
}

/// Reads one quoted lexeme from its prefix; `index` points after it on success.
fn quoted(
    sql: &[u16],
    start: usize,
    prefix: usize,
    previous: Option<&Lexeme>,
    index: &mut usize,
) -> Result<Lexeme> {
    let quote = sql[start + prefix];
    let is_string = quote == u16::from(b'\'');
    let continuation = is_string
        && previous.is_some_and(|previous| {
            previous.kind == Kind::String
                && sql[previous.end..start]
                    .iter()
                    .any(|&unit| unit == u16::from(b'\r') || unit == u16::from(b'\n'))
        });
    let letter = (prefix > 0).then(|| sql[start] | 0x20);
    let escaped = letter == Some(u16::from(b'e'))
        || (continuation && previous.is_some_and(|previous| previous.escaped));
    let unicode = letter == Some(u16::from(b'u'));
    let mut value = Text::new();
    let mut parts: Vec<u8> = Vec::new();
    let mut has_bytes = false;
    let mut closed = false;
    *index = start + prefix + 1;

    while *index < sql.len() {
        let current = sql[*index];
        *index += 1;

        if current == quote {
            if sql.get(*index) == Some(&quote) {
                value.push(quote);
                *index += 1;
            } else {
                closed = true;
                break;
            }
        } else if escaped && current == u16::from(b'\\') {
            let Some(length) = escape_length(sql, *index) else {
                return Err(SqlError::new("Unterminated SQL escape string."));
            };
            let escape = &sql[*index..*index + length];
            *index += length;
            let first = escape[0];

            if is_ascii(first, |byte| (b'0'..=b'7').contains(byte))
                || (first == u16::from(b'x') && length > 1)
            {
                parts.extend(utf8(&value));
                value.clear();
                has_bytes = true;
                let byte = if first == u16::from(b'x') {
                    hex(&escape[1..]).unwrap_or_default()
                } else {
                    let digits = String::from_utf16_lossy(escape);
                    u32::from_str_radix(&digits, 8).unwrap_or_default() & 0xff
                };
                parts.push(u8::try_from(byte & 0xff).unwrap_or_default());
            } else if (first == u16::from(b'u') || first == u16::from(b'U')) && length > 1 {
                push_code_point(&mut value, hex(&escape[1..]).unwrap_or_default())?;
            } else {
                let mapped = match u8::try_from(first).ok().filter(|_| length == 1) {
                    Some(b'b') => Some(0x08),
                    Some(b'f') => Some(0x0c),
                    Some(b'n') => Some(0x0a),
                    Some(b'r') => Some(0x0d),
                    Some(b't') => Some(0x09),
                    _ => None,
                };

                match mapped {
                    Some(unit) => value.push(unit),
                    None => value.extend_from_slice(escape),
                }
            }
        } else {
            value.push(current);
        }
    }

    if !closed {
        return Err(SqlError::new("Unterminated SQL quoted lexeme."));
    }

    let bytes = has_bytes.then(|| {
        parts.extend(utf8(&value));
        parts
    });

    Ok(Lexeme {
        kind: if is_string {
            Kind::String
        } else {
            Kind::Identifier
        },
        value: bytes.as_deref().map_or(value, from_utf8),
        start,
        end: *index,
        bytes,
        unicode,
        escaped,
        continuation,
    })
}

/// The length of a dollar-quote tag at `at`: `\$(?:[A-Za-z_\u0080-￿][A-Za-z_0-9\u0080-￿]*)?\$`
/// over code points, so a surrogate pair ends it and an unpaired surrogate belongs to it.
fn dollar_tag(sql: &[u16], at: usize) -> Option<usize> {
    let tag_unit = |position: usize, first: bool| {
        let unit = *sql.get(position)?;
        let pair = is_high(unit) && sql.get(position + 1).copied().is_some_and(is_low);
        let allowed = !pair
            && (unit >= 0x80
                || unit == u16::from(b'_')
                || is_ascii(
                    unit,
                    if first {
                        u8::is_ascii_alphabetic
                    } else {
                        u8::is_ascii_alphanumeric
                    },
                ));

        allowed.then_some(())
    };
    let mut position = at + 1;

    if tag_unit(position, true).is_some() {
        position += 1;

        while tag_unit(position, false).is_some() {
            position += 1;
        }
    }

    (sql.get(position) == Some(&u16::from(b'$'))).then_some(position + 1 - at)
}

/// The lexemes of a SQL text, without white space and comments.
///
/// # Errors
/// Fails on an unterminated comment, quoted lexeme, escape or dollar string, and on an unreadable
/// Unicode escape.
pub fn tokens(sql: &[u16]) -> Result<Vec<Lexeme>> {
    let mut lexemes: Vec<Lexeme> = Vec::new();
    let mut index = 0;

    while index < sql.len() {
        let start = index;
        let character = sql[index];

        if is_space(character) {
            index += 1;
            continue;
        }

        if starts_with(sql, index, "--") {
            while index < sql.len()
                && sql[index] != u16::from(b'\r')
                && sql[index] != u16::from(b'\n')
            {
                index += 1;
            }
            continue;
        }

        if starts_with(sql, index, "/*") {
            let mut depth = 1;
            index += 2;

            while index < sql.len() && depth > 0 {
                if starts_with(sql, index, "/*") {
                    depth += 1;
                    index += 2;
                } else if starts_with(sql, index, "*/") {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }

            if depth != 0 {
                return Err(SqlError::new("Unterminated SQL block comment."));
            }
            continue;
        }

        let quote_at = |offset: usize, quotes: &[u8]| {
            sql.get(index + offset)
                .is_some_and(|&unit| quotes.iter().any(|&quote| unit == u16::from(quote)))
        };
        let prefix = if (character | 0x20) == u16::from(b'e') && quote_at(1, b"'") {
            1
        } else if (character | 0x20) == u16::from(b'u')
            && sql.get(index + 1) == Some(&u16::from(b'&'))
            && quote_at(2, b"'\"")
        {
            2
        } else {
            0
        };

        if quote_at(prefix, b"'\"") {
            let lexeme = quoted(sql, start, prefix, lexemes.last(), &mut index)?;
            lexemes.push(lexeme);
            continue;
        }

        if starts_with(sql, index, "$${dynamic}") {
            index += "$${dynamic}".len();
            lexemes.push(simple(Kind::Word, units(INTERPOLATION), start, index));
            continue;
        }

        let dollar = (character == u16::from(b'$'))
            .then(|| dollar_tag(sql, index))
            .flatten();

        if let Some(length) = dollar {
            let tag = &sql[index..index + length];
            let Some(end) = find(sql, tag, index + length) else {
                return Err(SqlError::new("Unterminated SQL dollar string."));
            };
            index = end + length;
            lexemes.push(simple(
                Kind::String,
                sql[start + length..end].to_vec(),
                start,
                index,
            ));
        } else if is_word(character) {
            while index < sql.len() && (is_word(sql[index]) || sql[index] == u16::from(b'$')) {
                index += 1;
            }
            lexemes.push(simple(Kind::Word, lower(&sql[start..index]), start, index));
        } else {
            index += 1;
            lexemes.push(simple(Kind::Symbol, vec![character], start, index));
        }
    }

    decode(&lexemes)
}

fn simple(kind: Kind, value: Text, start: usize, end: usize) -> Lexeme {
    Lexeme {
        kind,
        value,
        start,
        end,
        bytes: None,
        unicode: false,
        escaped: false,
        continuation: false,
    }
}

/// Decodes `U&` lexemes with their `UESCAPE` and joins continued strings.
fn decode(lexemes: &[Lexeme]) -> Result<Vec<Lexeme>> {
    let mut decoded: Vec<Lexeme> = Vec::new();
    let mut position = 0;

    while position < lexemes.len() {
        let mut lexeme = lexemes[position].clone();

        if lexeme.unicode {
            let mut escape = units("\\");

            if lexemes
                .get(position + 1)
                .is_some_and(|next| next.kind == Kind::Word && equals(&next.value, "uescape"))
            {
                let delimiter = lexemes
                    .get(position + 2)
                    .filter(|delimiter| delimiter.kind == Kind::String)
                    .ok_or_else(|| SqlError::new("Unresolved SQL UESCAPE delimiter."))?;
                escape.clone_from(&delimiter.value);
                lexeme.end = delimiter.end;
                position += 2;
            }

            lexeme.value = decode_unicode(&lexeme.value, &escape)?;
        }

        match decoded.last_mut() {
            Some(previous) if lexeme.continuation && previous.kind == Kind::String => {
                if previous.bytes.is_some() || lexeme.bytes.is_some() {
                    let mut bytes = previous
                        .bytes
                        .take()
                        .unwrap_or_else(|| utf8(&previous.value));
                    bytes.extend(lexeme.bytes.unwrap_or_else(|| utf8(&lexeme.value)));
                    previous.value = from_utf8(&bytes);
                    previous.bytes = Some(bytes);
                } else {
                    previous.value.extend_from_slice(&lexeme.value);
                }
                previous.end = lexeme.end;
            }
            _ => decoded.push(lexeme),
        }

        position += 1;
    }

    Ok(decoded)
}

/// Whether a text holds `pg_advisory_` or `pg_try_advisory_`, ignoring case as a JavaScript
/// `/iu` pattern does (`ſ` folds to `s`).
fn advisory_evidence(text: &[u16]) -> bool {
    let folded: Text = text
        .iter()
        .map(|&unit| match unit {
            0x41..=0x5a => unit | 0x20,
            0x017f => u16::from(b's'),
            _ => unit,
        })
        .collect();

    contains(&folded, "pg_advisory_") || contains(&folded, "pg_try_advisory_")
}

/// Whether a value is a `pg_(try_)advisory_<suffix>` function name in lower case.
fn is_advisory_name(value: &[u16]) -> bool {
    let rest = if starts_with(value, 0, "pg_try_advisory_") {
        &value["pg_try_advisory_".len()..]
    } else if starts_with(value, 0, "pg_advisory_") {
        &value["pg_advisory_".len()..]
    } else {
        return false;
    };

    !rest.is_empty()
        && rest.iter().all(|&unit| {
            unit == u16::from(b'_')
                || is_ascii(unit, |byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit()
                })
        })
}

/// The arguments of a call's parenthesised source, split at top-level commas and trimmed.
fn split_arguments(source: &[u16]) -> Result<Vec<Text>> {
    let mut arguments = Vec::new();
    let mut depth = 0i64;
    let mut segment = 0;

    for lexeme in tokens(source)? {
        if lexeme.kind != Kind::Symbol {
            continue;
        }

        if equals(&lexeme.value, "(") {
            depth += 1;
        } else if equals(&lexeme.value, ")") {
            depth -= 1;
        } else if equals(&lexeme.value, ",") && depth == 0 {
            arguments.push(trim(&source[segment..lexeme.start]));
            segment = lexeme.end;
        }
    }

    let last = trim(&source[segment..]);

    if !last.is_empty() {
        arguments.push(last);
    }

    Ok(arguments)
}

fn trim(text: &[u16]) -> Text {
    let start = text
        .iter()
        .position(|&unit| !is_space(unit))
        .unwrap_or(text.len());
    let end = text
        .iter()
        .rposition(|&unit| !is_space(unit))
        .map_or(start, |end| end + 1);

    text[start..end].to_vec()
}

/// The start of the parenthesis that closes the one at `opening`.
fn closing_parenthesis(lexemes: &[Lexeme], opening: usize) -> Option<usize> {
    let mut depth = 0i64;

    for lexeme in lexemes
        .iter()
        .filter(|lexeme| lexeme.start >= opening && lexeme.kind == Kind::Symbol)
    {
        if equals(&lexeme.value, "(") {
            depth += 1;
        } else if equals(&lexeme.value, ")") {
            depth -= 1;

            if depth == 0 {
                return Some(lexeme.start);
            }
        }
    }

    None
}

/// A body to analyse after its parent text, with its language.
struct Child {
    value: Text,
    plpgsql: bool,
}

/// The advisory-lock calls and evidence of a SQL text and the bodies it executes.
///
/// # Errors
/// Fails where the text cannot be read, executes a dynamic command with advisory evidence in
/// PL/pgSQL, changes `standard_conforming_strings`, holds advisory SQL in an unsupported
/// procedural language, has an unbalanced call or nests bodies beyond the analysis budget.
pub fn analysis(sql: &[u16]) -> Result<Analysis> {
    let mut budget = WORK_LIMIT;

    analyse(sql, false, 0, &mut budget)
}

#[expect(
    clippy::too_many_lines,
    reason = "one pass reads a statement and keeps the order of its checks"
)]
fn analyse(sql: &[u16], plpgsql: bool, depth: usize, budget: &mut i64) -> Result<Analysis> {
    if depth > DEPTH_LIMIT {
        return Err(SqlError::new(
            "Executable SQL body analysis exceeded its recursion budget.",
        ));
    }

    *budget -= 1;

    if *budget < 0 {
        return Err(SqlError::new(
            "Executable SQL body analysis exceeded its recursion budget.",
        ));
    }

    let lexemes = tokens(sql)?;
    let mut result = Analysis::default();
    let mut children = Vec::new();
    let value_is = |lexeme: Option<&Lexeme>, text: &str| {
        lexeme.is_some_and(|lexeme| equals(&lexeme.value, text))
    };
    let word = |lexeme: Option<&Lexeme>, text: &str| {
        lexeme.is_some_and(|lexeme| lexeme.kind == Kind::Word && equals(&lexeme.value, text))
    };
    let identifier = |lexeme: Option<&Lexeme>| {
        lexeme.is_some_and(|lexeme| matches!(lexeme.kind, Kind::Word | Kind::Identifier))
    };
    let symbol = |lexeme: Option<&Lexeme>, text: &str| {
        lexeme.is_some_and(|lexeme| lexeme.kind == Kind::Symbol && equals(&lexeme.value, text))
    };
    // A quoted lexeme stands as one space, so the lexemes around it never join.
    let space = [u16::from(b' ')];
    let executable: Text = lexemes
        .iter()
        .flat_map(|lexeme| match lexeme.kind {
            Kind::String | Kind::Identifier => space.as_slice(),
            Kind::Word | Kind::Symbol => lexeme.value.as_slice(),
        })
        .copied()
        .collect();
    result.interpolation = contains(&executable, INTERPOLATION);
    let dynamic_evidence = plpgsql
        && advisory_evidence(
            &lexemes
                .iter()
                .filter(|lexeme| lexeme.kind == Kind::String)
                .flat_map(|lexeme| lexeme.value.iter().copied())
                .collect::<Text>(),
        );
    let at = |index: usize, offset: isize| {
        index
            .checked_add_signed(offset)
            .and_then(|position| lexemes.get(position))
    };

    for (index, lexeme) in lexemes.iter().enumerate() {
        let next = at(index, 1);

        if identifier(Some(lexeme)) && is_advisory_name(&lexeme.value) && symbol(next, "(") {
            let opening = next.map_or(0, |next| next.start);
            let Some(closing) = closing_parenthesis(&lexemes, opening) else {
                let mut message = units("Unbalanced advisory SQL near ");
                message.extend_from_slice(&lexeme.value);
                message.push(u16::from(b'.'));
                return Err(SqlError(message));
            };
            let arguments_start = next.map_or(0, |next| next.end);
            result.calls.push(Call {
                name: lexeme.value.clone(),
                qualified: value_is(at(index, -1), ".")
                    && identifier(at(index, -2))
                    && value_is(at(index, -2), "pg_catalog"),
                arguments: split_arguments(&sql[arguments_start..closing])?,
            });
        }

        if identifier(Some(lexeme))
            && (equals(&lexeme.value, "hashtext") || equals(&lexeme.value, "md5"))
            && symbol(next, "(")
        {
            result.alternate_derivation = true;
        }

        if equals(&lexeme.value, ":")
            && value_is(next, ":")
            && word(at(index, 2), "bit")
            && value_is(at(index, 3), "(")
            && value_is(at(index, 4), "64")
            && value_is(at(index, 5), ")")
        {
            result.alternate_derivation = true;
        }

        if plpgsql && word(Some(lexeme), "execute") && !value_is(at(index, -1), ".") {
            let after = at(index, 2);

            if let Some(next) = next.filter(|next| next.kind == Kind::String)
                && (after.is_none()
                    || value_is(after, ";")
                    || word(after, "into")
                    || word(after, "using"))
            {
                children.push(Child {
                    value: next.value.clone(),
                    plpgsql: false,
                });
            } else if dynamic_evidence {
                return Err(SqlError::new(
                    "Unresolved executable advisory SQL in a dynamic command.",
                ));
            }
        }

        let setting = if word(next, "local") || word(next, "session") {
            at(index, 2)
        } else {
            next
        };

        if (word(Some(lexeme), "set") || word(Some(lexeme), "reset"))
            && identifier(setting)
            && value_is(setting, "standard_conforming_strings")
        {
            return Err(SqlError::new(
                "Unsupported SQL standard_conforming_strings change.",
            ));
        }

        if identifier(Some(lexeme))
            && equals(&lexeme.value, "set_config")
            && symbol(next, "(")
            && at(index, 2).is_some_and(|argument| {
                argument.kind == Kind::String
                    && equals(&argument.value, "standard_conforming_strings")
            })
        {
            return Err(SqlError::new(
                "Unsupported SQL standard_conforming_strings change.",
            ));
        }
    }

    // Statement-level context distinguishes a stored or executed body from a value.
    for statement in
        lexemes.split(|lexeme| lexeme.kind == Kind::Symbol && equals(&lexeme.value, ";"))
    {
        let mut top: Vec<&Lexeme> = Vec::new();
        let mut parentheses = 0i64;

        for lexeme in statement {
            if symbol(Some(lexeme), "(") {
                parentheses += 1;
            }

            if parentheses == 0 {
                top.push(lexeme);
            }

            if symbol(Some(lexeme), ")") {
                parentheses -= 1;
            }
        }

        let top_at = |index: usize| top.get(index).copied();
        let is_do = word(top_at(0), "do");
        let routine_index = if word(top_at(1), "or") && word(top_at(2), "replace") {
            3
        } else {
            1
        };
        let is_routine = word(top_at(0), "create")
            && (word(top_at(routine_index), "function")
                || word(top_at(routine_index), "procedure"));

        if !is_do && !is_routine {
            continue;
        }

        let language_index = top.iter().position(|lexeme| word(Some(lexeme), "language"));
        let language: Option<Text> = match language_index {
            None => Some(units(if is_do { "plpgsql" } else { "sql" })),
            Some(index) => top_at(index + 1).map(|lexeme| lexeme.value.clone()),
        };
        let body_index = if is_do {
            let skipped = language_index.map_or(0, |index| index + 1);
            top.iter()
                .enumerate()
                .position(|(index, lexeme)| lexeme.kind == Kind::String && index != skipped)
        } else {
            Some(
                top.iter()
                    .position(|lexeme| word(Some(lexeme), "as"))
                    .map_or(0, |index| index + 1),
            )
        };
        let body = body_index
            .and_then(top_at)
            .filter(|body| body.kind == Kind::String);
        let Some(body) = body else {
            continue;
        };

        match language.as_deref() {
            Some(language) if equals(language, "c") || equals(language, "internal") => {}
            Some(language) if equals(language, "sql") || equals(language, "plpgsql") => children
                .push(Child {
                    value: body.value.clone(),
                    plpgsql: equals(language, "plpgsql"),
                }),
            _ if advisory_evidence(&body.value) => {
                return Err(SqlError::new(
                    "Unresolved advisory SQL in an unsupported procedural language.",
                ));
            }
            _ => {}
        }
    }

    for child in children {
        let analysis = analyse(&child.value, child.plpgsql, depth + 1, budget)?;
        result.calls.extend(analysis.calls);
        result.alternate_derivation |= analysis.alternate_derivation;
        result.interpolation |= analysis.interpolation;
    }

    Ok(result)
}

/// An argument with its white space and comments removed and its quoted lexemes as written.
///
/// # Errors
/// Fails where the argument cannot be read.
pub fn normalize_argument(argument: &[u16]) -> Result<Text> {
    Ok(tokens(argument)?
        .into_iter()
        .flat_map(|lexeme| match lexeme.kind {
            Kind::String | Kind::Identifier => argument[lexeme.start..lexeme.end].to_vec(),
            Kind::Word | Kind::Symbol => lexeme.value,
        })
        .collect())
}

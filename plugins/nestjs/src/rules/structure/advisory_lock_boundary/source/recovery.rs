//! The registered identifiers a text oxc cannot parse may hold where the TypeScript compiler
//! recovers a syntax tree from it.
//!
//! The text is read as the TypeScript scanner tokenizes it: a string ends at its quote or before a
//! line break, a template hands its `${…}` expressions back to code, comments are single tokens,
//! a number ends where its digits do, whatever follows, and a `/` where no operand ends starts a
//! regular-expression literal. Every number token counts, and one program is evaluated that holds
//! the lines that parse alone, the blocks of up to [`BLOCK_LIMIT`] lines that parse only together,
//! and the arithmetic runs of the text (see [`recovered`]).

use gate_typescript::{Allocator, parse_source};

use super::numeric::{evaluated, is_space, literal_value};
use super::{Fail, Outcome};

/// A token of the TypeScript scanner, as far as the recovery reads it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Token {
    Number,
    Identifier,
    /// One of the operators and parentheses of an arithmetic expression.
    Operator,
    Other,
}

/// Whether a code point ends a line, as the TypeScript scanner reads it.
fn is_line_break(character: char) -> bool {
    matches!(character, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// The end of a `'` or `"` string that starts at `start`: after its quote or, unterminated, before
/// a line break.
fn string_end(characters: &[char], start: usize) -> usize {
    let quote = characters[start];
    let mut index = start + 1;

    while let Some(&inner) = characters.get(index) {
        if is_line_break(inner) {
            break;
        }

        index += if inner == '\\' { 2 } else { 1 };

        if inner == quote {
            break;
        }
    }

    index.min(characters.len())
}

/// The end of a regular-expression literal that starts at `start`: after its closing `/` outside
/// a character class and its flags or, unterminated, before a line break.
fn regex_end(characters: &[char], start: usize) -> usize {
    let mut index = start + 1;
    let mut class = false;

    while let Some(&inner) = characters.get(index) {
        match inner {
            '\\' => index += 2,
            _ if is_line_break(inner) => return index,
            '[' => {
                class = true;
                index += 1;
            }
            ']' => {
                class = false;
                index += 1;
            }
            '/' if !class => {
                index += 1;

                while characters.get(index).is_some_and(char::is_ascii_alphabetic) {
                    index += 1;
                }

                return index;
            }
            _ => index += 1,
        }
    }

    characters.len()
}

/// The end of template text that starts after `start` (a backtick, or the `}` of an expression),
/// and whether it opened a `${` expression; `None` where it runs to the end of the text unclosed.
fn template_end(characters: &[char], start: usize) -> Option<(usize, bool)> {
    let mut index = start + 1;

    while let Some(&inner) = characters.get(index) {
        match inner {
            '\\' => index += 2,
            '`' => return Some((index + 1, false)),
            '$' if characters.get(index + 1) == Some(&'{') => return Some((index + 2, true)),
            _ => index += 1,
        }
    }

    None
}

/// The end of a number token that starts at `start`: a radix literal, or decimal digits with a
/// fraction and an exponent, with separators and a big-integer `n`.
fn number_end(characters: &[char], start: usize) -> usize {
    let at = |index: usize| characters.get(index).copied();
    let digits = |mut index: usize, radix: u32| {
        while at(index).is_some_and(|digit| digit.is_digit(radix) || digit == '_') {
            index += 1;
        }

        index
    };
    let radix = at(start + 1)
        .filter(|_| characters[start] == '0')
        .and_then(|mark| match mark {
            'x' | 'X' => Some(16),
            'b' | 'B' => Some(2),
            'o' | 'O' => Some(8),
            _ => None,
        });
    let mut index = if let Some(radix) = radix {
        digits(start + 2, radix)
    } else {
        let mut index = digits(start, 10);

        if at(index) == Some('.') {
            index = digits(index + 1, 10);
        }

        let signed = matches!(at(index + 1), Some('+' | '-'));
        let exponent = index + 1 + usize::from(signed);

        if matches!(at(index), Some('e' | 'E'))
            && at(exponent).is_some_and(|digit| digit.is_ascii_digit())
        {
            index = digits(exponent, 10);
        }

        index
    };

    if at(index) == Some('n') {
        index += 1;
    }

    index
}

/// The tokens of a text with their character ranges, a `/` read as the start of a regular
/// expression where it cannot divide when `regexes` is set and always as a division otherwise,
/// and whether the template tracking lost its place: a template that never closes, or a `${…}`
/// expression still open at the end.
fn tokens(characters: &[char], regexes: bool) -> (Vec<(usize, usize, Token)>, bool) {
    let mut tokens = Vec::new();
    let mut astray = false;
    let mut index = 0;
    // The open braces, and the brace depth of each template expression a `}` returns from.
    let mut braces = 0_usize;
    let mut templates: Vec<usize> = Vec::new();
    // Whether the last token can end an operand, after which a `/` divides.
    let mut operand = false;

    while let Some(&character) = characters.get(index) {
        let start = index;
        let next = characters.get(index + 1).copied();
        let mut regex = false;
        let comment = character == '/' && matches!(next, Some('/' | '*'));

        if is_space(character) {
            index += 1;
            continue;
        }

        let token = if character == '/' && next == Some('/') {
            index = characters[index..]
                .iter()
                .position(|&found| is_line_break(found))
                .map_or(characters.len(), |offset| index + offset);
            Token::Other
        } else if character == '/' && next == Some('*') {
            index = (index + 2..characters.len().saturating_sub(1))
                .find(|&found| characters[found] == '*' && characters[found + 1] == '/')
                .map_or(characters.len(), |found| found + 2);
            Token::Other
        } else if regexes && character == '/' && !operand {
            index = regex_end(characters, index);
            regex = true;
            Token::Other
        } else if matches!(character, '"' | '\'') {
            index = string_end(characters, index);
            Token::Other
        } else if character == '`' || (character == '}' && templates.last() == Some(&braces)) {
            if character == '}' {
                templates.pop();
                braces -= 1;
            }

            let ended = template_end(characters, index);
            astray |= ended.is_none();
            let (end, expression) = ended.unwrap_or((characters.len(), false));
            index = end;

            if expression {
                braces += 1;
                templates.push(braces);
            }

            Token::Other
        } else if character == '{' || character == '}' {
            index += 1;
            braces = if character == '{' {
                braces + 1
            } else {
                braces.saturating_sub(1)
            };
            Token::Other
        } else if character.is_ascii_digit()
            || (character == '.' && next.is_some_and(|digit| digit.is_ascii_digit()))
        {
            index = number_end(characters, index);
            Token::Number
        } else if character == '$' || character == '_' || character.is_alphabetic() {
            while characters
                .get(index)
                .is_some_and(|&inner| inner == '$' || inner == '_' || inner.is_alphanumeric())
            {
                index += 1;
            }

            Token::Identifier
        } else {
            index += 1;

            if "+-*/%~|^<>()!&".contains(character) {
                Token::Operator
            } else {
                Token::Other
            }
        };

        // A comment leaves the last operand as it was.
        if !comment {
            let last = characters[index - 1];
            let word: String = characters[start..index].iter().collect();
            let keyword = token == Token::Identifier && OPERATOR_KEYWORDS.contains(&word.as_str());
            operand = regex
                || token == Token::Number
                || (token == Token::Identifier && !keyword)
                || matches!(last, ')' | ']' | '"' | '\'' | '`');
        }
        tokens.push((start, index, token));
    }

    (tokens, astray || !templates.is_empty())
}

/// The keywords after which an expression starts, so a `/` after them starts a regular expression.
const OPERATOR_KEYWORDS: [&str; 15] = [
    "await",
    "case",
    "delete",
    "do",
    "else",
    "extends",
    "in",
    "instanceof",
    "new",
    "of",
    "return",
    "throw",
    "typeof",
    "void",
    "yield",
];

/// The longest run of lines the recovery tries as one block where each line alone fails.
const BLOCK_LIMIT: usize = 8;

/// Whether a text parses as TypeScript without an error.
fn parses(text: &str) -> bool {
    let allocator = Allocator::default();

    parse_source(&allocator, "recovered.ts", text)
        .is_ok_and(|file| !file.fatal() && file.errors().is_empty())
}

/// The lines of a text that parse on their own, and the blocks of up to [`BLOCK_LIMIT`] lines
/// that parse only together, from a line that fails alone; every other line is dropped.
fn units(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut units = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];

        if line.trim_matches(is_space).is_empty() {
            index += 1;
        } else if parses(line) {
            units.push(line.to_owned());
            index += 1;
        } else if let Some(end) = (index + 2..=(index + BLOCK_LIMIT).min(lines.len()))
            .find(|&end| parses(&lines[index..end].join("\n")))
        {
            units.push(lines[index..end].join("\n"));
            index = end;
        } else {
            index += 1;
        }
    }

    units
}

/// Whether a text parses as TypeScript, with errors the parser recovered from or without.
fn recovers(text: &str) -> bool {
    let allocator = Allocator::default();

    parse_source(&allocator, "recovered.ts", text).is_ok_and(|file| !file.fatal())
}

/// The runs of a text: maximal token sequences in which every token is `kept`, ended by any other
/// token and, where `split` is set, between two operands.
fn runs(
    characters: &[char],
    tokens: &[(usize, usize, Token)],
    kept: fn(Token) -> bool,
    split: bool,
) -> Vec<Vec<(usize, usize, Token)>> {
    let mut runs: Vec<Vec<(usize, usize, Token)>> = vec![Vec::new()];
    let mut previous: Option<(usize, Token)> = None;

    for &(start, end, token) in tokens {
        let operand = matches!(token, Token::Number | Token::Identifier);
        let follows_operand = previous.is_some_and(|(end, token)| {
            matches!(token, Token::Number | Token::Identifier) || characters[end - 1] == ')'
        });

        if !kept(token) || (split && operand && follows_operand) {
            runs.push(Vec::new());
        }

        if kept(token)
            && let Some(run) = runs.last_mut()
        {
            run.push((start, end, token));
        }

        previous = Some((end, token));
    }

    runs
}

/// The token indices of a run's unmatched parentheses, in order.
fn unmatched(characters: &[char], run: &[(usize, usize, Token)]) -> Vec<usize> {
    let mut open: Vec<usize> = Vec::new();
    let mut unmatched: Vec<usize> = Vec::new();

    for (index, &(start, _, token)) in run.iter().enumerate() {
        match (token, characters[start]) {
            (Token::Operator, '(') => open.push(index),
            (Token::Operator, ')') if open.pop().is_none() => unmatched.push(index),
            _ => {}
        }
    }

    unmatched.extend(open);
    unmatched.sort_unstable();
    unmatched
}

/// The text of a run with the parentheses at `dropped` blanked.
fn text_of(characters: &[char], run: &[(usize, usize, Token)], dropped: &[usize]) -> String {
    let (Some(first), Some(last)) = (run.first(), run.last()) else {
        return String::new();
    };
    let mut text: Vec<char> = characters[first.0..last.1].to_vec();

    for &index in dropped {
        text[run[index].0 - first.0] = ' ';
    }

    text.into_iter().collect()
}

/// The arithmetic expressions of a text, each read as one parenthesised expression that parses,
/// recovered errors allowed: the runs of operand and operator tokens, ended by any other token
/// and between two operands, both split at their unmatched parentheses and with those blanked,
/// and the runs of number and operator tokens alone with their unmatched parentheses blanked.
fn expressions(characters: &[char], tokens: &[(usize, usize, Token)]) -> Vec<String> {
    let arithmetic = |run: &[(usize, usize, Token)]| {
        run.iter().any(|&(start, _, token)| {
            token == Token::Operator && !matches!(characters[start], '(' | ')')
        }) && run
            .iter()
            .any(|(_, _, token)| matches!(token, Token::Number | Token::Identifier))
    };
    let mut texts: Vec<String> = Vec::new();

    for run in runs(characters, tokens, |token| token != Token::Other, true) {
        let unmatched = unmatched(characters, &run);
        let mut from = 0;

        for &index in unmatched.iter().chain(std::iter::once(&run.len())) {
            let piece = &run[from..index];

            if arithmetic(piece) {
                texts.push(text_of(characters, piece, &[]));
            }

            from = index + 1;
        }

        if !unmatched.is_empty() && arithmetic(&run) {
            texts.push(text_of(characters, &run, &unmatched));
        }
    }

    let numeric = |token| matches!(token, Token::Number | Token::Operator);

    for run in runs(characters, tokens, numeric, false) {
        if arithmetic(&run) {
            texts.push(text_of(characters, &run, &unmatched(characters, &run)));
        }
    }

    texts.sort_unstable();
    texts.dedup();
    texts
        .into_iter()
        .map(|text| format!("({text})"))
        .filter(|expression| recovers(expression))
        .collect()
}

/// Every value the recovery reads in a text oxc cannot parse: every number token, also one glued
/// to the letters after it, and every value of one program that holds the text's lines and blocks
/// that parse and its arithmetic runs, so its variables are shared across lines. A JSON text is
/// read as one parenthesised expression. The text is tokenized twice, once reading a `/` as a
/// regular expression where it cannot divide and once always as a division, and the recovery
/// reads the union of both, so it reads at least what either reading alone reads. What it does
/// not reach: a construct of more than [`BLOCK_LIMIT`] lines none of which parses alone, and one that parses only with lines
/// before the line that fails alone. A text whose regex-aware reading loses the template tracking
/// fails the recovery as unresolved, as an unbalanced backtick.
pub(super) fn recovered(text: &str) -> Outcome<Vec<f64>> {
    let characters: Vec<char> = text.chars().collect();
    let (aware, lost) = tokens(&characters, true);

    // Where the template tracking loses its place, the text is not read as passing.
    if lost {
        return Err(Fail::new(
            "The text has an unbalanced backtick, so its registered advisory-lock identifiers cannot be read.",
        ));
    }

    let (plain, _) = tokens(&characters, false);
    let mut values: Vec<f64> = aware
        .iter()
        .chain(&plain)
        .filter(|(_, _, token)| *token == Token::Number)
        .map(|&(start, end, _)| literal_value(&characters[start..end].iter().collect::<String>()))
        .collect();
    let json = format!("({text}\n)");
    let parts = if parses(&json) {
        vec![json]
    } else {
        let mut expressions = expressions(&characters, &aware);
        expressions.extend(self::expressions(&characters, &plain));
        expressions.sort_unstable();
        expressions.dedup();

        units(text).into_iter().chain(expressions).collect()
    };
    let program = parts.join("\n;\n");
    let allocator = Allocator::default();

    match parse_source(&allocator, "recovered.ts", &program) {
        Ok(file) if !file.fatal() => values.extend(evaluated(&file)?),
        // Every part parses alone; should their joint program not, each is read alone.
        _ => {
            for part in &parts {
                let allocator = Allocator::default();

                if let Ok(file) = parse_source(&allocator, "recovered.ts", part)
                    && !file.fatal()
                {
                    values.extend(evaluated(&file)?);
                }
            }
        }
    }

    Ok(values)
}

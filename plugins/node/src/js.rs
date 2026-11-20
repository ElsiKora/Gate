//! The module specifiers of a `JavaScript` or `TypeScript` module, read from its tokens: comments,
//! strings, template literals and regular expressions are skipped as the language reads them, so
//! a commented-out import or a string that only looks like one is not counted.

/// One token, as far as the specifiers need it.
#[derive(Debug, PartialEq, Eq)]
enum Token {
    /// An identifier or keyword.
    Word(String),
    /// A string literal, or a template literal without `${`, with its line.
    Text(String, usize),
    /// One punctuation character.
    Mark(char),
    /// Anything else: a number, a regular expression, a template literal with substitutions.
    Other,
}

/// The module specifiers of `text`'s static imports and re-exports (`from "x"`, `import "x"`),
/// dynamic imports (`import("x")`) and `require("x")` calls, with their lines.
pub(crate) fn specifiers(text: &str) -> Vec<(String, usize)> {
    let tokens = tokens(text);
    let mut found = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let Token::Word(word) = token else {
            continue;
        };
        if index > 0 && tokens[index - 1] == Token::Mark('.') {
            continue;
        }
        let next = tokens.get(index + 1);
        let called = || match (next, tokens.get(index + 2)) {
            (Some(Token::Mark('(')), Some(Token::Text(specifier, line))) => {
                Some((specifier.clone(), *line))
            }
            _ => None,
        };
        let specifier = match (word.as_str(), next) {
            ("from" | "import", Some(Token::Text(specifier, line))) => {
                Some((specifier.clone(), *line))
            }
            ("import" | "require", _) => called(),
            _ => None,
        };
        found.extend(specifier);
    }
    found
}

/// The tokens of `text`.
fn tokens(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;
    let mut line = 1;
    while let Some(&character) = chars.get(at) {
        let next = chars.get(at + 1).copied();
        match character {
            '\n' => {
                line += 1;
                at += 1;
            }
            _ if character.is_whitespace() || character == '\u{feff}' => at += 1,
            '/' if next == Some('/') => {
                while chars.get(at).is_some_and(|&character| character != '\n') {
                    at += 1;
                }
            }
            '/' if next == Some('*') => {
                at += 2;
                while at < chars.len() && chars.get(at..at + 2) != Some(&['*', '/'][..]) {
                    if chars[at] == '\n' {
                        line += 1;
                    }
                    at += 1;
                }
                at += 2;
            }
            '/' if starts_expression(tokens.last()) => {
                at = regular_expression(&chars, at + 1);
                tokens.push(Token::Other);
            }
            '"' | '\'' => {
                let start = line;
                let (value, end) = quoted(&chars, at + 1, character, &mut line);
                at = end;
                tokens.push(Token::Text(value, start));
            }
            '`' => {
                let start = line;
                let (value, end, plain) = template(&chars, at + 1, &mut line);
                at = end;
                tokens.push(if plain {
                    Token::Text(value, start)
                } else {
                    Token::Other
                });
            }
            _ if character.is_alphabetic() || character == '_' || character == '$' => {
                let start = at;
                while chars.get(at).is_some_and(|&character| {
                    character.is_alphanumeric() || character == '_' || character == '$'
                }) {
                    at += 1;
                }
                tokens.push(Token::Word(chars[start..at].iter().collect()));
            }
            _ if character.is_ascii_digit() => {
                while chars
                    .get(at)
                    .is_some_and(|&character| character.is_alphanumeric() || character == '.')
                {
                    at += 1;
                }
                tokens.push(Token::Other);
            }
            _ => {
                tokens.push(Token::Mark(character));
                at += 1;
            }
        }
    }
    tokens
}

/// Whether a `/` after `previous` starts a regular expression rather than a division.
fn starts_expression(previous: Option<&Token>) -> bool {
    match previous {
        None => true,
        Some(Token::Mark(mark)) => !matches!(mark, ')' | ']' | '}'),
        Some(Token::Word(word)) => matches!(
            word.as_str(),
            "return"
                | "typeof"
                | "instanceof"
                | "in"
                | "of"
                | "new"
                | "delete"
                | "void"
                | "throw"
                | "case"
                | "do"
                | "else"
                | "yield"
                | "await"
        ),
        Some(Token::Text(..) | Token::Other) => false,
    }
}

/// The end of a regular expression whose body starts at `at`: past its closing `/` and flags.
fn regular_expression(chars: &[char], mut at: usize) -> usize {
    let mut class = false;
    while let Some(&character) = chars.get(at) {
        match character {
            '\\' => at += 1,
            '[' => class = true,
            ']' => class = false,
            '/' if !class => break,
            '\n' => return at,
            _ => {}
        }
        at += 1;
    }
    at += 1;
    while chars
        .get(at)
        .is_some_and(|character| character.is_alphanumeric())
    {
        at += 1;
    }
    at
}

/// A string literal's value from `at` to its closing `quote`, and the index past it.
fn quoted(chars: &[char], mut at: usize, quote: char, line: &mut usize) -> (String, usize) {
    let mut value = String::new();
    while let Some(&character) = chars.get(at) {
        at += 1;
        match character {
            '\\' => {
                if let Some(&escaped) = chars.get(at) {
                    if escaped == '\n' {
                        *line += 1;
                    }
                    value.push(escaped);
                    at += 1;
                }
            }
            '\n' => {
                *line += 1;
                break;
            }
            _ if character == quote => break,
            _ => value.push(character),
        }
    }
    (value, at)
}

/// A template literal's text from `at` to its closing backtick, the index past it, and whether it
/// has no `${` substitution. A substitution is skipped by its braces.
fn template(chars: &[char], mut at: usize, line: &mut usize) -> (String, usize, bool) {
    let mut value = String::new();
    let mut plain = true;
    while let Some(&character) = chars.get(at) {
        at += 1;
        match character {
            '\\' => {
                if let Some(&escaped) = chars.get(at) {
                    value.push(escaped);
                    at += 1;
                }
            }
            '`' => break,
            '$' if chars.get(at) == Some(&'{') => {
                plain = false;
                let mut depth = 0_usize;
                while let Some(&inner) = chars.get(at) {
                    at += 1;
                    match inner {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        '\n' => *line += 1,
                        _ => {}
                    }
                }
            }
            _ => {
                if character == '\n' {
                    *line += 1;
                }
                value.push(character);
            }
        }
    }
    (value, at, plain)
}

#[cfg(test)]
mod tests {
    use super::specifiers;

    fn names(text: &str) -> Vec<(String, usize)> {
        specifiers(text)
    }

    #[test]
    fn every_import_form_with_its_line() {
        let text = "import a from \"x\";\nimport \"side\";\nconst b = await import('dyn');\nconst c = require( \"req\" );\nexport { d } from 're';\nconst reimport = 1;\nconst t = import(`tpl`);\n";
        let found = names(text);
        let found: Vec<(&str, usize)> = found
            .iter()
            .map(|(name, line)| (name.as_str(), *line))
            .collect();
        assert_eq!(
            found,
            [
                ("x", 1),
                ("side", 2),
                ("dyn", 3),
                ("req", 4),
                ("re", 5),
                ("tpl", 7)
            ]
        );
    }

    #[test]
    fn comments_strings_and_regular_expressions_hold_no_import() {
        let text = "// import a from \"line\";\n/* import b from 'block';\n   require(\"block2\") */\nconst s = \"import c from 'inside'\";\nconst r = /from \"re\"/u;\nconst q = `${x} from \"tpl\"`;\nconst y = Array.from('z');\nimport real from \"real\";\n";
        assert_eq!(names(text), [("real".to_owned(), 8)]);
    }
}

//! The value exports of the packages the rule knows by name, read from the `package-exports-file`
//! option: a Rust source table that a consumer's own step generates from the installed packages.
//!
//! The table has the shape
//!
//! ```text
//! pub const PACKAGES: [(&str, &[&str]); 2] = [
//!     ("@scope/automator", &AUTOMATOR),
//!     ("@nestjs/common", &NEST_COMMON),
//! ];
//!
//! const AUTOMATOR: [&str; 2] = ["ErrorString", "EErrorStringAction"];
//! const NEST_COMMON: [&str; 1] = ["BadRequestException"];
//! ```
//!
//! `//` comments are ignored. Without the option, the known packages are the exception packages and
//! the message factory's package, and each of them exports every name it is asked for.

use std::collections::{BTreeMap, BTreeSet};

/// The packages the rule resolves by name instead of following their files.
#[derive(Debug)]
pub struct Packages {
    /// Each package with its value exports; `None` when every name is taken as exported.
    entries: BTreeMap<String, Option<BTreeSet<String>>>,
}

/// What a package import resolves to.
pub enum Export {
    /// The specifier is not a known package.
    Unknown,
    /// The package does not export the name.
    Missing,
    /// The package exports the name.
    Found,
}

impl Packages {
    /// The known packages without an export table: each of `names` exports every name.
    pub fn open<'n>(names: impl IntoIterator<Item = &'n str>) -> Self {
        Self {
            entries: names
                .into_iter()
                .map(|name| (name.to_owned(), None))
                .collect(),
        }
    }

    /// The table of the file `path` with the contents `text`.
    ///
    /// # Errors
    /// When the text is not a table of the documented shape; the message names the file and the
    /// problem.
    pub fn parse(path: &str, text: &str) -> Result<Self, String> {
        let fail = |problem: &str| {
            format!(
                "the package-exports-file {path} {problem}; regenerate it as `pub const PACKAGES: [(&str, &[&str]); N]` naming one `const NAME: [&str; M]` list per package"
            )
        };
        let tokens = tokenize(text).ok_or_else(|| fail("has an unterminated string"))?;
        let mut lists: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut packages: Option<Vec<(String, String)>> = None;
        let mut index = 0;

        while index < tokens.len() {
            if tokens[index] != Token::Word("const".to_owned()) {
                index += 1;
                continue;
            }

            let Some(Token::Word(name)) = tokens.get(index + 1) else {
                return Err(fail("has a const without a name"));
            };
            let name = name.clone();
            let open = (index..tokens.len())
                .find(|at| {
                    tokens[*at] == Token::Punct('=')
                        && tokens.get(at + 1) == Some(&Token::Punct('['))
                })
                .ok_or_else(|| fail(&format!("does not initialize {name} with a list")))?;
            let (items, next) = list(&tokens, open + 2)
                .ok_or_else(|| fail(&format!("has a malformed list {name}")))?;

            if name == "PACKAGES" {
                let mut entries = Vec::new();

                for item in items {
                    match item.as_slice() {
                        [
                            Token::Punct('('),
                            Token::Str(package),
                            Token::Punct(','),
                            Token::Punct('&'),
                            Token::Word(list),
                            Token::Punct(')'),
                        ] => entries.push((package.clone(), list.clone())),
                        _ => {
                            return Err(fail(
                                "has a PACKAGES entry other than (\"package\", &LIST)",
                            ));
                        }
                    }
                }

                packages = Some(entries);
            } else {
                let mut names = BTreeSet::new();

                for item in items {
                    match item.as_slice() {
                        [Token::Str(export)] => {
                            names.insert(export.clone());
                        }
                        _ => {
                            return Err(fail(&format!(
                                "has an entry of {name} that is not a string"
                            )));
                        }
                    }
                }

                lists.insert(name, names);
            }

            index = next;
        }

        let packages = packages.ok_or_else(|| fail("declares no PACKAGES table"))?;
        let mut entries = BTreeMap::new();

        for (package, list) in packages {
            let names = lists.get(&list).ok_or_else(|| {
                fail(&format!("names the list {list}, which it does not declare"))
            })?;
            entries.insert(package, Some(names.clone()));
        }

        Ok(Self { entries })
    }

    /// Whether `package` exports `name`.
    pub fn export(&self, package: &str, name: &str) -> Export {
        match self.entries.get(package) {
            None => Export::Unknown,
            Some(None) => Export::Found,
            Some(Some(names)) if names.contains(name) => Export::Found,
            Some(Some(_)) => Export::Missing,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Token {
    Word(String),
    Str(String),
    Punct(char),
}

/// The tokens of Rust source text: words, string literals with their escapes read, and single
/// punctuation characters; `//` comments are skipped. `None` when a string is not closed.
fn tokenize(text: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut characters = text.chars().peekable();

    while let Some(character) = characters.next() {
        if character.is_whitespace() {
            continue;
        }

        if character == '/' && characters.peek() == Some(&'/') {
            while characters.next_if(|next| *next != '\n').is_some() {}
            continue;
        }

        if character == '"' {
            let mut value = String::new();

            loop {
                match characters.next()? {
                    '"' => break,
                    '\\' => match characters.next()? {
                        'n' => value.push('\n'),
                        't' => value.push('\t'),
                        'r' => value.push('\r'),
                        '0' => value.push('\0'),
                        other => value.push(other),
                    },
                    other => value.push(other),
                }
            }

            tokens.push(Token::Str(value));
            continue;
        }

        if character.is_alphanumeric() || character == '_' {
            let mut word = character.to_string();

            while let Some(next) = characters.next_if(|next| next.is_alphanumeric() || *next == '_')
            {
                word.push(next);
            }

            tokens.push(Token::Word(word));
            continue;
        }

        tokens.push(Token::Punct(character));
    }

    Some(tokens)
}

/// The comma-separated items of a list whose `[` precedes `start`, each as its tokens, with the
/// index after its closing `];`. Brackets and parentheses nest; a trailing comma is allowed.
fn list(tokens: &[Token], start: usize) -> Option<(Vec<Vec<Token>>, usize)> {
    let mut items = Vec::new();
    let mut item = Vec::new();
    let mut depth = 0usize;
    let mut index = start;

    loop {
        let token = tokens.get(index)?;
        index += 1;

        match token {
            Token::Punct(']') if depth == 0 => {
                if !item.is_empty() {
                    items.push(item);
                }

                if tokens.get(index) == Some(&Token::Punct(';')) {
                    index += 1;
                }

                return Some((items, index));
            }
            Token::Punct(',') if depth == 0 => items.push(std::mem::take(&mut item)),
            Token::Punct('(' | '[') => {
                depth += 1;
                item.push(token.clone());
            }
            Token::Punct(')' | ']') => {
                depth = depth.checked_sub(1)?;
                item.push(token.clone());
            }
            token => item.push(token.clone()),
        }
    }
}

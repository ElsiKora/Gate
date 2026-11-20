//! JSON as the toolchain files write it: `package.json`, `.prettierrc.json` and `tsconfig*.json`,
//! which TypeScript reads with `//` and `/* */` comments and trailing commas. Each value keeps the
//! one-based line where it starts, so a finding points at the line that holds the key.

/// A value. Numbers keep their text: no rule computes with them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Value {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Node>),
    Object(Vec<Member>),
}

/// One value and the line where it starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Node {
    pub(crate) value: Value,
    pub(crate) line: usize,
}

/// One key of an object, the line of the key, and its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Member {
    pub(crate) key: String,
    pub(crate) line: usize,
    pub(crate) value: Node,
}

impl Node {
    /// The first member named `key`, when this is an object.
    pub(crate) fn member(&self, key: &str) -> Option<&Member> {
        self.members().iter().find(|member| member.key == key)
    }

    /// The value of the first member named `key`.
    pub(crate) fn get(&self, key: &str) -> Option<&Node> {
        self.member(key).map(|member| &member.value)
    }

    /// The members of an object; none for any other value.
    pub(crate) fn members(&self) -> &[Member] {
        match &self.value {
            Value::Object(members) => members,
            _ => &[],
        }
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match &self.value {
            Value::String(text) => Some(text),
            _ => None,
        }
    }

    pub(crate) fn as_bool(&self) -> Option<bool> {
        match self.value {
            Value::Bool(value) => Some(value),
            _ => None,
        }
    }

    /// The value as compact JSON, for messages and for comparing with a standard value.
    pub(crate) fn render(&self) -> String {
        match &self.value {
            Value::Null => "null".to_owned(),
            Value::Bool(value) => value.to_string(),
            Value::Number(text) => text.clone(),
            Value::String(text) => quote(text),
            Value::Array(items) => {
                let items: Vec<String> = items.iter().map(Self::render).collect();
                format!("[{}]", items.join(","))
            }
            Value::Object(members) => {
                let members: Vec<String> = members
                    .iter()
                    .map(|member| format!("{}:{}", quote(&member.key), member.value.render()))
                    .collect();
                format!("{{{}}}", members.join(","))
            }
        }
    }
}

/// `text` as a JSON string.
pub(crate) fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Parses `text`.
///
/// # Errors
/// A message naming the line where the text stops being JSON.
pub(crate) fn parse(text: &str) -> Result<Node, String> {
    let mut parser = Parser {
        chars: text.chars().collect(),
        at: 0,
        line: 1,
    };
    parser.skip_blank()?;
    let node = parser.value(0)?;
    parser.skip_blank()?;
    if parser.at < parser.chars.len() {
        return Err(format!("line {}: text after the value", parser.line));
    }
    Ok(node)
}

/// How deep values may nest; deeper text is refused rather than overflowing the stack.
const MAX_DEPTH: usize = 128;

struct Parser {
    chars: Vec<char>,
    at: usize,
    line: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.at += 1;
        if character == '\n' {
            self.line += 1;
        }
        Some(character)
    }

    fn error(&self, what: &str) -> String {
        format!("line {}: {what}", self.line)
    }

    /// Skips white space and comments.
    fn skip_blank(&mut self) -> Result<(), String> {
        loop {
            match (self.peek(), self.chars.get(self.at + 1).copied()) {
                (Some(character), _) if character.is_whitespace() || character == '\u{feff}' => {
                    self.bump();
                }
                (Some('/'), Some('/')) => {
                    while self.peek().is_some_and(|character| character != '\n') {
                        self.bump();
                    }
                }
                (Some('/'), Some('*')) => {
                    self.bump();
                    self.bump();
                    loop {
                        match self.bump() {
                            None => return Err(self.error("a comment is not closed")),
                            Some('*') if self.peek() == Some('/') => {
                                self.bump();
                                break;
                            }
                            Some(_) => {}
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn value(&mut self, depth: usize) -> Result<Node, String> {
        if depth > MAX_DEPTH {
            return Err(self.error("values nest too deeply"));
        }
        let line = self.line;
        let value = match self.peek() {
            Some('{') => self.object(depth)?,
            Some('[') => self.array(depth)?,
            Some('"') => Value::String(self.string()?),
            Some(character) if character == '-' || character.is_ascii_digit() => {
                Value::Number(self.number())
            }
            Some(character) if character.is_ascii_alphabetic() => self.word()?,
            Some(_) => return Err(self.error("a value is expected")),
            None => return Err(self.error("the text ends where a value is expected")),
        };
        Ok(Node { value, line })
    }

    fn object(&mut self, depth: usize) -> Result<Value, String> {
        self.bump();
        let mut members = Vec::new();
        loop {
            self.skip_blank()?;
            if self.peek() == Some('}') {
                self.bump();
                return Ok(Value::Object(members));
            }
            if self.peek() != Some('"') {
                return Err(self.error("a key in double quotes is expected"));
            }
            let line = self.line;
            let key = self.string()?;
            self.skip_blank()?;
            if self.bump() != Some(':') {
                return Err(self.error("a colon is expected after a key"));
            }
            self.skip_blank()?;
            let value = self.value(depth + 1)?;
            members.push(Member { key, line, value });
            self.skip_blank()?;
            match self.bump() {
                Some(',') => {}
                Some('}') => return Ok(Value::Object(members)),
                _ => return Err(self.error("a comma or a closing brace is expected")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, String> {
        self.bump();
        let mut items = Vec::new();
        loop {
            self.skip_blank()?;
            if self.peek() == Some(']') {
                self.bump();
                return Ok(Value::Array(items));
            }
            items.push(self.value(depth + 1)?);
            self.skip_blank()?;
            match self.bump() {
                Some(',') => {}
                Some(']') => return Ok(Value::Array(items)),
                _ => return Err(self.error("a comma or a closing bracket is expected")),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.bump();
        let mut out = String::new();
        loop {
            match self.bump() {
                None | Some('\n') => return Err(self.error("a string is not closed")),
                Some('"') => return Ok(out),
                Some('\\') => {
                    let escaped = match self.bump() {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('r') => '\r',
                        Some('b') => '\u{8}',
                        Some('f') => '\u{c}',
                        Some('u') => self.unicode()?,
                        Some(character @ ('"' | '\\' | '/')) => character,
                        _ => return Err(self.error("an unknown escape in a string")),
                    };
                    out.push(escaped);
                }
                Some(character) => out.push(character),
            }
        }
    }

    fn unicode(&mut self) -> Result<char, String> {
        let mut code = 0_u32;
        for _ in 0..4 {
            let digit = self
                .bump()
                .and_then(|character| character.to_digit(16))
                .ok_or_else(|| self.error("a \\u escape needs four hex digits"))?;
            code = code * 16 + digit;
        }
        Ok(char::from_u32(code).unwrap_or('\u{fffd}'))
    }

    fn number(&mut self) -> String {
        let mut out = String::new();
        while let Some(character) = self.peek() {
            if character.is_ascii_digit() || matches!(character, '-' | '+' | '.' | 'e' | 'E') {
                out.push(character);
                self.bump();
            } else {
                break;
            }
        }
        out
    }

    fn word(&mut self) -> Result<Value, String> {
        let mut word = String::new();
        while let Some(character) = self.peek().filter(char::is_ascii_alphabetic) {
            word.push(character);
            self.bump();
        }
        match word.as_str() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            "null" => Ok(Value::Null),
            _ => Err(self.error("a value is expected")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Value, parse};

    #[test]
    fn comments_and_trailing_commas_parse_and_keys_keep_their_lines() {
        let text = "{\n  // a comment\n  \"a\": 1, /* inline */\n  \"b\": {\n    \"c\": [true, null,],\n  },\n}\n";
        let root = parse(text).expect("parses");
        assert_eq!(root.member("a").map(|member| member.line), Some(3));
        let b = root.member("b").expect("b");
        assert_eq!(b.line, 4);
        assert_eq!(
            b.value.get("c").map(super::Node::render).as_deref(),
            Some("[true,null]")
        );
        assert_eq!(
            root.get("a").map(|node| node.value.clone()),
            Some(Value::Number("1".to_owned()))
        );
    }

    #[test]
    fn broken_text_names_its_line() {
        assert_eq!(
            parse("{\n\"a\": }").expect_err("broken"),
            "line 2: a value is expected"
        );
        assert_eq!(
            parse("{\"a\": \"x\\q\"}").expect_err("broken"),
            "line 1: an unknown escape in a string"
        );
        assert_eq!(
            parse("/* open").expect_err("broken"),
            "line 1: a comment is not closed"
        );
        assert_eq!(
            parse("{} {}").expect_err("broken"),
            "line 1: text after the value"
        );
    }

    #[test]
    fn escapes_and_unicode_read_back() {
        let root = parse("{\"k\": \"a\\\"b\\u0041\"}").expect("parses");
        assert_eq!(root.get("k").and_then(super::Node::as_str), Some("a\"bA"));
    }
}

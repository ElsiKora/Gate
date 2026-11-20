//! A Dockerfile as instructions and stages: continuation lines joined, comments and heredoc bodies
//! skipped, each instruction with the line where it starts.

/// One instruction: its keyword in upper case, its arguments, and its one-based line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Instruction {
    pub(crate) keyword: String,
    pub(crate) arguments: String,
    pub(crate) line: usize,
}

/// One stage: its `FROM` and the instructions after it, up to the next `FROM`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Stage {
    /// The image or stage it starts from, as written (variables not expanded).
    pub(crate) base: String,
    /// The name after `AS`, in lower case, as the builder compares it.
    pub(crate) name: Option<String>,
    pub(crate) line: usize,
    pub(crate) instructions: Vec<Instruction>,
}

/// A build argument declared before the first `FROM`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Argument {
    pub(crate) name: String,
    pub(crate) default: Option<String>,
    pub(crate) line: usize,
}

/// A parsed Dockerfile: the `ARG`s before the first `FROM`, with their defaults, and the stages.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Dockerfile {
    pub(crate) arguments: Vec<Argument>,
    pub(crate) stages: Vec<Stage>,
}

/// The instructions of `text`, in order.
pub(crate) fn instructions(text: &str) -> Vec<Instruction> {
    let mut instructions = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((index, line)) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut joined = String::new();
        let mut current = trimmed.to_owned();
        while let Some(head) = current.strip_suffix('\\') {
            joined.push_str(head);
            joined.push(' ');
            let Some((_, next)) = lines
                .by_ref()
                .find(|(_, next)| !next.trim().starts_with('#'))
            else {
                current.clear();
                break;
            };
            next.trim().clone_into(&mut current);
        }
        joined.push_str(&current);
        let (keyword, arguments) = joined
            .split_once(char::is_whitespace)
            .map_or((joined.as_str(), ""), |(keyword, rest)| {
                (keyword, rest.trim())
            });
        if HEREDOC_KEYWORDS
            .iter()
            .any(|taker| keyword.eq_ignore_ascii_case(taker))
        {
            // Each body runs to a line that is its delimiter, one after another; nothing in them
            // is an instruction.
            for delimiter in heredocs(arguments) {
                let _ = lines.by_ref().find(|(_, body)| body.trim() == delimiter);
            }
        }
        instructions.push(Instruction {
            keyword: keyword.to_ascii_uppercase(),
            arguments: arguments.to_owned(),
            line: index + 1,
        });
    }
    instructions
}

/// The instructions that take heredocs.
const HEREDOC_KEYWORDS: [&str; 3] = ["RUN", "COPY", "ADD"];

/// The delimiters of the heredocs the arguments open, in order: `<<` or `<<-` at the start of a
/// word, an optional quote, then a word that starts with a letter or `_` (`<<EOF`, `<<-"EOF"`).
/// `$((1<<2))` and the here-string `<<<` open none.
fn heredocs(arguments: &str) -> Vec<String> {
    let mut delimiters = Vec::new();
    let mut from = 0;
    while let Some(found) = arguments[from..].find("<<") {
        let start = from + found;
        from = start + 2;
        if arguments[..start]
            .chars()
            .next_back()
            .is_some_and(|before| !before.is_whitespace())
        {
            continue;
        }
        let rest = &arguments[start + 2..];
        let rest = rest.strip_prefix('-').unwrap_or(rest);
        let rest = rest.strip_prefix(['"', '\'']).unwrap_or(rest);
        if !rest
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        {
            continue;
        }
        let delimiter: String = rest
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect();
        delimiters.push(delimiter);
    }
    delimiters
}

/// The arguments split at white space, quotes kept together and removed.
pub(crate) fn words(arguments: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    for character in arguments.chars() {
        match (quote, character) {
            (None, '"' | '\'') => {
                quote = Some(character);
                started = true;
            }
            (Some(open), _) if open == character => quote = None,
            (None, _) if character.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            _ => {
                word.push(character);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// The flags (`--name=value`) before the first other word, and the other words.
pub(crate) fn flags(arguments: &str) -> (Vec<(String, String)>, Vec<String>) {
    let mut flags = Vec::new();
    let mut rest = Vec::new();
    for word in words(arguments) {
        match word.strip_prefix("--") {
            Some(flag) if rest.is_empty() => {
                let (name, value) = flag.split_once('=').unwrap_or((flag, ""));
                flags.push((name.to_ascii_lowercase(), value.to_owned()));
            }
            _ => rest.push(word),
        }
    }
    (flags, rest)
}

/// The names an `ARG` or `ENV` declares, with their values when given: `NAME`, `NAME=value`, and
/// for `ENV` the old `NAME value` form.
pub(crate) fn variables(keyword: &str, arguments: &str) -> Vec<(String, Option<String>)> {
    let words = words(arguments);
    if keyword == "ENV" && words.len() >= 2 && !words[0].contains('=') {
        return vec![(words[0].clone(), Some(words[1..].join(" ")))];
    }
    words
        .into_iter()
        .map(|word| match word.split_once('=') {
            Some((name, value)) => (name.to_owned(), Some(value.to_owned())),
            None => (word, None),
        })
        .collect()
}

/// The labels a `LABEL` sets: `key=value` pairs, or the old `key value` form.
pub(crate) fn labels(arguments: &str) -> Vec<String> {
    let words = words(arguments);
    if words.len() >= 2 && !words[0].contains('=') {
        return vec![words[0].clone()];
    }
    words
        .into_iter()
        .filter_map(|word| word.split_once('=').map(|(key, _)| key.to_owned()))
        .collect()
}

/// The Dockerfile of `text`.
pub(crate) fn parse(text: &str) -> Dockerfile {
    let mut dockerfile = Dockerfile::default();
    for instruction in instructions(text) {
        if instruction.keyword == "FROM" {
            let (_, words) = flags(&instruction.arguments);
            let base = words.first().cloned().unwrap_or_default();
            let name = match words.as_slice() {
                [_, keyword, name, ..] if keyword.eq_ignore_ascii_case("as") => {
                    Some(name.to_ascii_lowercase())
                }
                _ => None,
            };
            dockerfile.stages.push(Stage {
                base,
                name,
                line: instruction.line,
                instructions: Vec::new(),
            });
        } else if let Some(stage) = dockerfile.stages.last_mut() {
            stage.instructions.push(instruction);
        } else if instruction.keyword == "ARG" {
            dockerfile
                .arguments
                .extend(variables("ARG", &instruction.arguments).into_iter().map(
                    |(name, default)| Argument {
                        name,
                        default,
                        line: instruction.line,
                    },
                ));
        }
    }
    dockerfile
}

impl Dockerfile {
    /// The index of the earlier stage `reference` names (by name or by index) from stage `before`.
    pub(crate) fn stage(&self, reference: &str, before: usize) -> Option<usize> {
        let reference = reference.to_ascii_lowercase();
        if let Ok(index) = reference.parse::<usize>() {
            return (index < before).then_some(index);
        }
        self.stages[..before]
            .iter()
            .rposition(|stage| stage.name.as_deref() == Some(reference.as_str()))
    }

    /// `text` with `$NAME` and `${NAME}` replaced by the defaults of the global `ARG`s; `None`
    /// when it uses a variable without a default.
    pub(crate) fn expand(&self, text: &str) -> Option<String> {
        let mut expanded = String::new();
        let mut rest = text;
        while let Some(start) = rest.find('$') {
            expanded.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            let (name, length) = if let Some(braced) = after.strip_prefix('{') {
                let end = braced.find('}')?;
                (&braced[..end], end + 2)
            } else {
                let end = after
                    .find(|character: char| {
                        !(character.is_ascii_alphanumeric() || character == '_')
                    })
                    .unwrap_or(after.len());
                (&after[..end], end)
            };
            let value = self
                .arguments
                .iter()
                .rev()
                .find(|argument| argument.name == name)
                .and_then(|argument| argument.default.clone())?;
            expanded.push_str(&value);
            rest = &after[length..];
        }
        expanded.push_str(rest);
        Some(expanded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuations_comments_and_heredocs_are_one_instruction_each() {
        let text = "# syntax=docker/dockerfile:1\nARG BASE=node@sha256:x\nFROM ${BASE} AS build\nRUN a \\\n  # inside\n  b\nRUN <<EOF\nUSER root\nEOF\nUSER node\n";
        let found: Vec<(String, usize)> = instructions(text)
            .into_iter()
            .map(|instruction| (instruction.keyword, instruction.line))
            .collect();
        let expected = [
            ("ARG", 2),
            ("FROM", 3),
            ("RUN", 4),
            ("RUN", 7),
            ("USER", 10),
        ];
        assert_eq!(
            found,
            expected.map(|(keyword, line)| (keyword.to_owned(), line))
        );
        let parsed = parse(text);
        assert_eq!(parsed.stages[0].name.as_deref(), Some("build"));
        assert_eq!(parsed.expand("${BASE}").as_deref(), Some("node@sha256:x"));
        assert_eq!(parsed.expand("$OTHER"), None);
    }

    #[test]
    fn a_heredoc_opens_only_at_a_word_start_in_an_instruction_that_takes_one() {
        let keywords = |text: &str| -> Vec<(String, usize)> {
            instructions(text)
                .into_iter()
                .map(|instruction| (instruction.keyword, instruction.line))
                .collect()
        };
        let expected = |pairs: &[(&str, usize)]| -> Vec<(String, usize)> {
            pairs
                .iter()
                .map(|(keyword, line)| ((*keyword).to_owned(), *line))
                .collect()
        };
        // Shell arithmetic and a here-string open no heredoc: the USER after them is read.
        assert_eq!(
            keywords("FROM a\nRUN echo $((1<<2))\nUSER root\nRUN cat <<<\"x\"\nUSER node\n"),
            expected(&[
                ("FROM", 1),
                ("RUN", 2),
                ("USER", 3),
                ("RUN", 4),
                ("USER", 5)
            ])
        );
        // A quoted, dashed delimiter, then two heredocs in one RUN whose bodies follow in order.
        assert_eq!(
            keywords(
                "FROM a\nRUN <<-\"END\"\n\tUSER root\n\tEND\nRUN <<ONE cat - <<TWO\nUSER one\nONE\nUSER two\nTWO\nUSER node\n"
            ),
            expected(&[("FROM", 1), ("RUN", 2), ("RUN", 5), ("USER", 10)])
        );
        // COPY takes a heredoc; LABEL does not, so a value that looks like one is not.
        assert_eq!(
            keywords("FROM a\nCOPY <<EOF /app/x\nUSER root\nEOF\nLABEL a=\"<<EOF\"\nUSER node\n"),
            expected(&[("FROM", 1), ("COPY", 2), ("LABEL", 5), ("USER", 6)])
        );
    }

    #[test]
    fn variables_and_labels_take_both_forms() {
        assert_eq!(
            variables("ENV", "A=1 B=\"two words\""),
            [
                ("A".to_owned(), Some("1".to_owned())),
                ("B".to_owned(), Some("two words".to_owned()))
            ]
        );
        assert_eq!(
            variables("ENV", "A one"),
            [("A".to_owned(), Some("one".to_owned()))]
        );
        assert_eq!(variables("ARG", "TOKEN"), [("TOKEN".to_owned(), None)]);
        assert_eq!(labels("\"a.b\"=1 c=\"x y\""), ["a.b", "c"]);
        assert_eq!(labels("a value"), ["a"]);
    }
}

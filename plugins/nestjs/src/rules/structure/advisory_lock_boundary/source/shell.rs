//! Advisory SQL in a shell command, for the shell scripts, JSON files and package scripts the rule
//! reads.
//!
//! A command is split at `;`, newlines, `|` and `&` outside quotes, also inside `$(…)` and
//! backticks; its static `NAME="value"` assignments are expanded; `then`, `do`, `else`, `command`,
//! `exec`, `env` and leading assignments are dropped; and a command that runs one of the shell
//! executables with a `pg_` and an advisory lock or unlock in its letters is advisory SQL. The
//! regular expressions read JavaScript whitespace and code points.

use gate_sdk::serde_json::{self, Value};
use regex_automata::meta::Regex;
use rustc_hash::FxHashMap as HashMap;

use super::super::sql::units;
use super::text::{DEPTH_LIMIT, validate};
use super::{Fail, Outcome};

/// JavaScript's `\s`: white space and line terminators.
const SPACE: &str = r"\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";

/// The regular expressions of the shell reader, built once per run.
#[derive(Debug)]
pub struct Patterns {
    /// `/(?:^|[;\n])\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(["'])(.*?)\2/gsu`, with
    /// the backreference spelled out per quote.
    assignment: Regex,
    /// `/\$(?:\{([A-Za-z_][A-Za-z0-9_]*)\}|([A-Za-z_][A-Za-z0-9_]*))/gu`.
    reference: Regex,
    /// A command prefix: a keyword, `command`, `exec`, `env` or a leading assignment.
    prefix: Regex,
    /// `/^(?:"([^"]+)"|'([^']+)'|([^\s(]+))(?:\s|\()/u`, the command's executable.
    executable: Regex,
}

/// Compiles one pattern of the shell reader.
fn compile(name: &str, pattern: &str) -> gate_sdk::Result<Regex> {
    Regex::new(pattern).map_err(|error| {
        gate_sdk::Error::new(format!(
            "the shell {name} pattern does not compile ({error}); this is a defect of the plugin, report it"
        ))
    })
}

impl Patterns {
    /// Compiles the patterns.
    ///
    /// # Errors
    /// When a pattern does not compile, which is a defect of the plugin.
    pub fn new() -> gate_sdk::Result<Self> {
        let space = format!("[{SPACE}]");
        let other = format!("[^{SPACE}]");
        let dot = r"[^\n\r\x{2028}\x{2029}]";

        Ok(Self {
            assignment: compile(
                "assignment",
                &format!(
                    r#"(?:^|[;\n])[{SPACE}]*(?:export[{SPACE}]+)?([A-Za-z_][A-Za-z0-9_]*)[{SPACE}]*=[{SPACE}]*(?:"(?s:(.*?))"|'(?s:(.*?))')"#
                ),
            )?,
            reference: compile(
                "reference",
                r"\$(?:\{([A-Za-z_][A-Za-z0-9_]*)\}|([A-Za-z_][A-Za-z0-9_]*))",
            )?,
            prefix: compile(
                "prefix",
                &format!(
                    r#"^(?:(?:then|do|else){space}+|command(?:{space}+(?:-p|--))?{space}+|exec(?:{space}+(?:-[cl]|-a{space}+{other}+|--))*{space}+|env(?:{space}+(?:-i|--ignore-environment|(?:-u|--unset){space}+{other}+|--unset={other}+|--))*{space}+|[A-Za-z_][A-Za-z0-9_]*=(?:"(?:\\{dot}|[^"])*"|'[^']*'|{other}+){space}+)"#
                ),
            )?,
            executable: compile(
                "executable",
                &format!(r#"^(?:"([^"]+)"|'([^']+)'|([^{SPACE}(]+))(?:[{SPACE}]|\()"#),
            )?,
        })
    }
}

fn is_space(character: char) -> bool {
    matches!(
        character,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

fn checked(text: &str) -> Outcome<()> {
    validate(units(text)).map(|_| ())
}

/// The static assignments of a shell source, the last of a name winning.
fn assignments(patterns: &Patterns, source: &str) -> Outcome<HashMap<String, String>> {
    let mut assignments = HashMap::default();

    for captures in patterns.assignment.captures_iter(source) {
        let value = captures
            .get_group(2)
            .or_else(|| captures.get_group(3))
            .map_or("", |value| &source[value]);
        let name = captures.get_group(1).map_or("", |name| &source[name]);
        checked(value)?;
        assignments.insert(name.to_owned(), value.to_owned());
    }

    Ok(assignments)
}

/// `text` with every static reference replaced by its assignment, a reference to an unassigned
/// name kept as written.
fn replace_references(
    patterns: &Patterns,
    text: &str,
    assignments: &HashMap<String, String>,
) -> String {
    let mut replaced = String::with_capacity(text.len());
    let mut last = 0;

    for captures in patterns.reference.captures_iter(text) {
        let Some(whole) = captures.get_match() else {
            continue;
        };
        let name = captures
            .get_group(1)
            .or_else(|| captures.get_group(2))
            .map_or("", |name| &text[name]);

        replaced.push_str(&text[last..whole.start()]);
        replaced.push_str(
            assignments
                .get(name)
                .map_or(&text[whole.range()], String::as_str),
        );
        last = whole.end();
    }

    replaced.push_str(&text[last..]);
    replaced
}

/// A segment with its static variables expanded until it no longer changes.
fn expand(
    patterns: &Patterns,
    segment: &str,
    assignments: &HashMap<String, String>,
) -> Outcome<String> {
    let mut expanded = segment.to_owned();

    for _ in 0..=DEPTH_LIMIT {
        let next = replace_references(patterns, &expanded, assignments);

        checked(&next)?;

        if next == expanded {
            return Ok(next);
        }

        expanded = next;
    }

    Err(Fail::new(
        "Static shell-variable expansion budget was exceeded.",
    ))
}

/// The command segments of a shell source, nested `$(…)` and backtick commands included.
fn segments(source: &[char], depth: usize) -> Outcome<Vec<String>> {
    if depth > DEPTH_LIMIT {
        return Err(Fail::new(
            "Static shell-command nesting budget was exceeded.",
        ));
    }

    let text = |from: usize, to: usize| -> String {
        let to = to.min(source.len());
        source[from.min(to)..to].iter().collect()
    };
    let mut segments = Vec::new();
    let mut quote: Option<char> = None;
    let mut start = 0;
    let mut index = 0;

    while index < source.len() {
        let character = source[index];

        if character == '\\' && quote != Some('\'') {
            index += 2;
            continue;
        }

        if quote != Some('\'') && character == '$' && source.get(index + 1) == Some(&'(') {
            let mut nested_depth = 1;
            let mut nested_quote: Option<char> = None;
            let mut end = index + 2;

            while end < source.len() && nested_depth > 0 {
                let nested = source[end];

                if nested == '\\' && nested_quote != Some('\'') {
                    end += 1;
                } else if let Some(open) = nested_quote {
                    if nested == open {
                        nested_quote = None;
                    }
                } else if nested == '\'' || nested == '"' {
                    nested_quote = Some(nested);
                } else if nested == '(' {
                    nested_depth += 1;
                } else if nested == ')' {
                    nested_depth -= 1;
                }

                end += 1;
            }

            let inner: Vec<char> = text(index + 2, end - 1).chars().collect();
            segments.extend(self::segments(&inner, depth + 1)?);
            index = end;
            continue;
        }

        if quote != Some('\'') && character == '`' {
            let mut end = index + 1;

            while end < source.len() {
                if source[end] == '\\' {
                    end += 1;
                } else if source[end] == '`' {
                    break;
                }

                end += 1;
            }

            let inner: Vec<char> = text(index + 1, end).chars().collect();
            segments.extend(self::segments(&inner, depth + 1)?);
            index = end + 1;
            continue;
        }

        if let Some(open) = quote {
            if character == open {
                quote = None;
            }
        } else if character == '\'' || character == '"' {
            quote = Some(character);
        } else if character == '#' && text(start, index).trim_matches(is_space).is_empty() {
            index = source[index..]
                .iter()
                .position(|&found| found == '\n')
                .map_or(source.len(), |offset| index + offset);
            start = index + 1;
        } else if matches!(character, ';' | '\n' | '|' | '&') {
            segments.push(text(start, index));
            start = index + 1;
        }

        index += 1;
    }

    segments.push(text(start, source.len()));

    Ok(segments)
}

/// Whether one command segment runs advisory SQL through one of `executables`.
fn runs_advisory_sql(patterns: &Patterns, expanded: &str, executables: &[String]) -> Outcome<bool> {
    let mut command = expanded.trim_start_matches(is_space);

    for depth in 0..=DEPTH_LIMIT {
        let Some(prefix) = patterns.prefix.find(command) else {
            break;
        };

        if depth == DEPTH_LIMIT {
            return Err(Fail::new(
                "Static shell-command prefix budget was exceeded.",
            ));
        }

        command = command[prefix.end()..].trim_start_matches(is_space);
    }

    let mut captures = patterns.executable.create_captures();
    patterns.executable.captures(command, &mut captures);
    let executable = captures
        .get_group(1)
        .or_else(|| captures.get_group(2))
        .or_else(|| captures.get_group(3))
        .map(|name| command[name].rsplit('/').next().unwrap_or_default());

    if !executable.is_some_and(|executable| executables.iter().any(|known| known == executable)) {
        return Ok(false);
    }

    let normalized: String = command
        .to_lowercase()
        .chars()
        .filter(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || *character == '_'
        })
        .collect();

    Ok(normalized.contains("pg_")
        && [
            "advisory_lock",
            "advisory_unlock",
            "advisory_xact_lock",
            "advisory_xact_unlock",
        ]
        .iter()
        .any(|call| normalized.contains(call)))
}

/// Whether a shell source holds a command that runs advisory SQL.
fn contains_advisory_command(
    patterns: &Patterns,
    source: &str,
    executables: &[String],
) -> Outcome<bool> {
    let assignments = assignments(patterns, source)?;
    let characters: Vec<char> = source.chars().collect();

    for segment in segments(&characters, 0)? {
        if runs_advisory_sql(
            patterns,
            &expand(patterns, &segment, &assignments)?,
            executables,
        )? {
            return Ok(true);
        }
    }

    Ok(false)
}

/// The shell sources of a file: the `scripts` of a `package.json`, the whole text otherwise.
fn sources(path: &str, text: &str) -> Outcome<Vec<String>> {
    if !path.ends_with("package.json") {
        return Ok(vec![text.to_owned()]);
    }

    let Ok(document) = serde_json::from_str::<Value>(text) else {
        return Err(Fail::new(&format!("{path} is not valid JSON.")));
    };
    let scripts = match document.get("scripts") {
        Some(Value::Object(scripts)) => scripts.values().cloned().collect(),
        Some(Value::Array(scripts)) => scripts.clone(),
        _ => Vec::new(),
    };

    Ok(scripts
        .into_iter()
        .filter_map(|script| script.as_str().map(str::to_owned))
        .collect())
}

/// Checks the shell commands of a shell script, JSON file or `package.json`: none of them runs
/// advisory SQL through one of `executables`.
pub(super) fn check(
    patterns: &Patterns,
    path: &str,
    text: &str,
    executables: &[String],
) -> Outcome<()> {
    for source in sources(path, text)? {
        if contains_advisory_command(patterns, &source, executables)? {
            return Err(Fail::new(&format!(
                "{path} contains executable advisory SQL in a shell command."
            )));
        }
    }

    Ok(())
}

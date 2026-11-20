//! A small YAML writer for the generated workflow: block maps and lists, strings plain when that is
//! unambiguous and double-quoted otherwise. Deterministic: keys keep their order. The `uses` of a
//! job or a step written `<pin> # <release>` becomes the pin and a comment, as a workflow writes it
//! by hand; a `uses` anywhere else (an action's input) stays a string.

use serde_json::Value;

/// The YAML text of `value`, a JSON object, with a final newline.
#[must_use]
pub fn to_yaml(value: &Value) -> String {
    let mut lines = Vec::new();
    match value {
        Value::Object(map) => write_map(map, Place::Root, 0, &mut lines),
        other => lines.push(scalar(other)),
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// Where a map stands in the workflow: only a job's and a step's `uses` names an action or a
/// reusable workflow.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    /// The workflow itself.
    Root,
    /// The workflow's `jobs`.
    Jobs,
    /// One job.
    Job,
    /// One step of a job's `steps`.
    Step,
    /// Anything else.
    Other,
}

fn write_map(
    map: &serde_json::Map<String, Value>,
    place: Place,
    indent: usize,
    lines: &mut Vec<String>,
) {
    let pad = " ".repeat(indent);
    for (key, value) in map {
        let child_place = match place {
            Place::Root if key == "jobs" => Place::Jobs,
            Place::Jobs => Place::Job,
            _ => Place::Other,
        };
        let item_place = if place == Place::Job && key == "steps" {
            Place::Step
        } else {
            Place::Other
        };
        let names_action = key == "uses" && matches!(place, Place::Job | Place::Step);
        let key = if key == "on" {
            key.clone()
        } else {
            string(key)
        };
        match value {
            Value::Object(child) if !child.is_empty() => {
                lines.push(format!("{pad}{key}:"));
                write_map(child, child_place, indent + 2, lines);
            }
            Value::Array(items) if !items.is_empty() => {
                lines.push(format!("{pad}{key}:"));
                write_list(items, item_place, indent + 2, lines);
            }
            Value::String(text) if names_action => {
                lines.push(format!("{pad}{key}: {}", uses(text)));
            }
            other => lines.push(format!("{pad}{key}: {}", scalar(other))),
        }
    }
}

/// A `uses` value: `<pin> # <release>` as the pin followed by the release as a comment, when the
/// pin is a plain scalar and the release one line; anything else as a string.
fn uses(text: &str) -> String {
    match text.split_once(" # ") {
        Some((pin, release))
            if is_plain(pin) && !release.trim().is_empty() && !release.contains(['\n', '\r']) =>
        {
            format!("{pin} # {}", release.trim())
        }
        _ => string(text),
    }
}

/// Writes `items`; each map among them stands at `place`.
fn write_list(items: &[Value], place: Place, indent: usize, lines: &mut Vec<String>) {
    let pad = " ".repeat(indent);
    for item in items {
        match item {
            Value::Object(map) if !map.is_empty() => {
                let start = lines.len();
                write_map(map, place, indent + 2, lines);
                let first = &lines[start];
                lines[start] = format!("{pad}- {}", &first[indent + 2..]);
            }
            Value::Array(inner) if !inner.is_empty() => {
                let start = lines.len();
                write_list(inner, Place::Other, indent + 2, lines);
                let first = &lines[start];
                lines[start] = format!("{pad}- {}", &first[indent + 2..]);
            }
            other => lines.push(format!("{pad}- {}", scalar(other))),
        }
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => string(text),
        Value::Array(_) => "[]".to_owned(),
        Value::Object(_) => "{}".to_owned(),
    }
}

/// A string as a plain scalar when YAML reads it back as the same string, else double-quoted.
fn string(text: &str) -> String {
    if is_plain(text) {
        text.to_owned()
    } else {
        serde_json::to_string(text).unwrap_or_else(|_| format!("{text:?}"))
    }
}

fn is_plain(text: &str) -> bool {
    const RESERVED: &[&str] = &[
        "true", "false", "yes", "no", "on", "off", "y", "n", "null", "~",
    ];
    let Some(first) = text.chars().next() else {
        return false;
    };
    let looks_numeric = first.is_ascii_digit()
        || (matches!(first, '+' | '-' | '.')
            && text
                .chars()
                .nth(1)
                .is_some_and(|next| next.is_ascii_digit() || next == '.'))
        || text.starts_with(".inf")
        || text.starts_with(".nan");

    !looks_numeric
        && !"-?:,[]{}#&*!|>'\"%@`".contains(first)
        && !first.is_whitespace()
        && !text.ends_with(char::is_whitespace)
        && !text.ends_with(':')
        && !text.contains(": ")
        && !text.contains(" #")
        && !text.chars().any(char::is_control)
        && !RESERVED.contains(&text.to_ascii_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::to_yaml;

    #[test]
    fn a_job_or_step_uses_release_is_a_comment_after_the_pin_and_elsewhere_a_hash_is_quoted() {
        let pin = "actions/checkout@0123456789abcdef0123456789abcdef01234567";
        let called = "o/r/.github/workflows/build.yml@0123456789abcdef0123456789abcdef01234567";
        let text = to_yaml(&json!({
            "jobs": {
                "build": {
                    "steps": [
                        { "name": "Check out", "uses": format!("{pin} # v4.2.2") },
                        { "uses": pin },
                        { "run": "echo a # b" },
                        { "uses": pin, "with": { "uses": "a # b" } },
                    ],
                },
                "reuse": { "uses": format!("{called} # v1.0.0") },
            },
            "env": { "uses": "c # d" },
        }));
        assert_eq!(
            text,
            format!(
                "jobs:\n  build:\n    steps:\n      - name: Check out\n        uses: {pin} # v4.2.2\n      \
                 - uses: {pin}\n      - run: \"echo a # b\"\n      - uses: {pin}\n        with:\n          \
                 uses: \"a # b\"\n  reuse:\n    uses: {called} # v1.0.0\nenv:\n  uses: \"c # d\"\n"
            )
        );
    }
}

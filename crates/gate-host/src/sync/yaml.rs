//! A small YAML writer for the generated workflow: block maps and lists, strings plain when that is
//! unambiguous and double-quoted otherwise. Deterministic: keys keep their order. A `uses` value
//! written `<pin> # <release>` becomes the pin and a comment, as a workflow writes it by hand.

use serde_json::Value;

/// The YAML text of `value`, a JSON object, with a final newline.
#[must_use]
pub fn to_yaml(value: &Value) -> String {
    let mut lines = Vec::new();
    match value {
        Value::Object(map) => write_map(map, 0, &mut lines),
        other => lines.push(scalar(other)),
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn write_map(map: &serde_json::Map<String, Value>, indent: usize, lines: &mut Vec<String>) {
    let pad = " ".repeat(indent);
    for (key, value) in map {
        let key = if key == "on" {
            key.clone()
        } else {
            string(key)
        };
        match value {
            Value::Object(child) if !child.is_empty() => {
                lines.push(format!("{pad}{key}:"));
                write_map(child, indent + 2, lines);
            }
            Value::Array(items) if !items.is_empty() => {
                lines.push(format!("{pad}{key}:"));
                write_list(items, indent + 2, lines);
            }
            Value::String(text) if key == "uses" => {
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

fn write_list(items: &[Value], indent: usize, lines: &mut Vec<String>) {
    let pad = " ".repeat(indent);
    for item in items {
        match item {
            Value::Object(map) if !map.is_empty() => {
                let start = lines.len();
                write_map(map, indent + 2, lines);
                let first = &lines[start];
                lines[start] = format!("{pad}- {}", &first[indent + 2..]);
            }
            Value::Array(inner) if !inner.is_empty() => {
                let start = lines.len();
                write_list(inner, indent + 2, lines);
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
    fn a_uses_release_is_a_comment_after_the_pin_and_elsewhere_a_hash_is_quoted() {
        let pin = "actions/checkout@0123456789abcdef0123456789abcdef01234567";
        let text = to_yaml(&json!({
            "steps": [
                { "name": "Check out", "uses": format!("{pin} # v4.2.2") },
                { "uses": pin },
                { "run": "echo a # b" },
            ],
        }));
        assert_eq!(
            text,
            format!(
                "steps:\n  - name: Check out\n    uses: {pin} # v4.2.2\n  - uses: {pin}\n  - run: \"echo a # b\"\n"
            )
        );
    }
}

//! Option rows read with JavaScript's value semantics.
//!
//! A rule that keeps lenient option rows compares their fields with JavaScript's strict equality
//! (`===`), so a missing field (`undefined`), a `null` and a string stay distinct, and a message
//! prints a field as a template literal does.

use gate_sdk::serde_json::{self, Value};

/// A JavaScript value of an option field as far as the checks tell them apart.
#[derive(Clone, Copy, Debug)]
pub enum Js<'v> {
    Undefined,
    Null,
    Str(&'v str),
    /// Any other JSON value: a number or boolean equals the same number or boolean, and an object
    /// or array equals only itself, the same field read twice, as JavaScript objects compare by
    /// identity.
    Other(&'v Value),
}

/// JavaScript strict equality (`===`).
impl PartialEq for Js<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Undefined, Self::Undefined) | (Self::Null, Self::Null) => true,
            (Self::Str(left), Self::Str(right)) => left == right,
            (Self::Other(Value::Number(left)), Self::Other(Value::Number(right))) => {
                left.as_f64() == right.as_f64()
            }
            (Self::Other(Value::Bool(left)), Self::Other(Value::Bool(right))) => left == right,
            (Self::Other(left @ (Value::Object(_) | Value::Array(_))), Self::Other(right)) => {
                std::ptr::eq(*left, *right)
            }
            _ => false,
        }
    }
}

impl<'v> Js<'v> {
    #[must_use]
    pub fn of(value: Option<&'v Value>) -> Self {
        match value {
            None => Self::Undefined,
            Some(Value::Null) => Self::Null,
            Some(Value::String(text)) => Self::Str(text),
            Some(other) => Self::Other(other),
        }
    }

    #[must_use]
    pub fn string(self) -> Option<&'v str> {
        match self {
            Self::Str(text) => Some(text),
            _ => None,
        }
    }

    /// `this ?? fallback`.
    #[must_use]
    pub fn or(self, fallback: Self) -> Self {
        match self {
            Self::Undefined | Self::Null => fallback,
            other => other,
        }
    }

    /// The text a template literal prints for the value, as JavaScript's `String()` does.
    #[must_use]
    pub fn text(self) -> String {
        match self {
            Self::Undefined => "undefined".to_owned(),
            Self::Null => "null".to_owned(),
            Self::Str(text) => text.to_owned(),
            Self::Other(value) => string(value),
        }
    }

    /// `JSON.stringify(value)` as a template literal prints it.
    #[must_use]
    pub fn json(self) -> String {
        match self {
            Self::Undefined => "undefined".to_owned(),
            Self::Null => "null".to_owned(),
            Self::Str(text) => Value::String(text.to_owned()).to_string(),
            Self::Other(value) => json(value),
        }
    }
}

/// `JSON.stringify(value)`: compact, keys in their order, and every number as JavaScript prints it.
fn json(value: &Value) -> String {
    match value {
        Value::Number(number) => number_text(number),
        Value::Array(items) => {
            format!("[{}]", items.iter().map(json).collect::<Vec<_>>().join(","))
        }
        Value::Object(object) => format!(
            "{{{}}}",
            object
                .iter()
                .map(|(key, item)| format!("{}:{}", Value::String(key.clone()), json(item)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        other => other.to_string(),
    }
}

/// A number as JavaScript prints it, `1` for `1.0`.
fn number_text(number: &serde_json::Number) -> String {
    number
        .as_f64()
        .map_or_else(|| number.to_string(), gate_typescript::value::number_text)
}

/// `String(value)` of a JSON value that is no string: an array joins its elements' texts with
/// commas, `null` among them as nothing, and an object prints `[object Object]`.
fn string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number_text(number),
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

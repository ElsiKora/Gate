//! The toolchain files the rules of decision 10 read: `tsconfig.json` as JSON with comments,
//! `package.json`, and the Vite configuration as TypeScript.

use gate_sdk::serde_json::{Map, Value};
use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    CallExpression, Expression, ObjectExpression, ObjectProperty, ObjectPropertyKind,
};
use gate_typescript::oxc_span::Span;
use gate_typescript::value::string_like;

use crate::code::{Project, joined};

/// The TypeScript configuration of the project.
pub const TSCONFIG: &str = "tsconfig.json";

/// The npm manifest of the project.
pub const PACKAGE_JSON: &str = "package.json";

/// The names a Vite configuration takes, in the order Vite looks for them.
pub const VITE_CONFIGS: [&str; 4] = [
    "vite.config.js",
    "vite.config.mjs",
    "vite.config.ts",
    "vite.config.mts",
];

/// How many `extends` a tsconfig is followed through.
const EXTENDS_DEPTH: usize = 8;

/// The text of `text` with its `//` and `/* */` comments and its trailing commas removed, so that
/// a tsconfig reads as JSON.
#[must_use]
pub fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    let mut string = false;

    while let Some(character) = characters.next() {
        if string {
            out.push(character);
            match character {
                '\\' => out.extend(characters.next()),
                '"' => string = false,
                _ => {}
            }
            continue;
        }
        let following = characters.peek().copied();
        match (character, following) {
            ('"', _) => {
                string = true;
                out.push(character);
            }
            ('/', Some('/')) => {
                for inner in characters.by_ref() {
                    if inner == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                characters.next();
                let mut last = ' ';
                for inner in characters.by_ref() {
                    if last == '*' && inner == '/' {
                        break;
                    }
                    last = inner;
                }
            }
            (',', _) => {
                let rest: String = characters.clone().collect();
                let after = rest.trim_start_matches(char::is_whitespace);
                if !(after.starts_with('}') || after.starts_with(']')) {
                    out.push(character);
                }
            }
            _ => out.push(character),
        }
    }

    out
}

/// The JSON of the tracked file `path`, read as JSON with comments; `None` when it is not tracked.
///
/// # Errors
/// When the file cannot be read or is not JSON once its comments are removed.
pub fn json(project: &Project<'_, '_>, path: &str) -> gate_sdk::Result<Option<Value>> {
    let Some(text) = project.text(path)? else {
        return Ok(None);
    };
    gate_sdk::serde_json::from_str(&without_comments(&text))
        .map(Some)
        .map_err(|error| {
            gate_sdk::Error::new(format!(
                "{path} is not JSON ({error}); fix the file, which the frontend plugin reads"
            ))
        })
}

/// The `compilerOptions.paths` of the project's tsconfig, following its relative `extends` through
/// tracked files; `None` when no tsconfig of the chain sets them.
///
/// # Errors
/// When a tsconfig of the chain cannot be read or is not JSON.
pub fn tsconfig_paths(project: &Project<'_, '_>) -> gate_sdk::Result<Option<Map<String, Value>>> {
    let mut path = TSCONFIG.to_owned();

    for _ in 0..EXTENDS_DEPTH {
        let Some(config) = json(project, &path)? else {
            return Ok(None);
        };
        if let Some(paths) = config
            .pointer("/compilerOptions/paths")
            .and_then(Value::as_object)
        {
            return Ok(Some(paths.clone()));
        }
        let Some(next) = config
            .get("extends")
            .and_then(Value::as_str)
            .and_then(|extends| joined(&path, extends))
        else {
            return Ok(None);
        };
        path = next;
    }

    Ok(None)
}

/// The project's Vite configuration, by the first of [`VITE_CONFIGS`] that is tracked.
#[must_use]
pub fn vite_config<'o>(project: &Project<'_, 'o>) -> Option<&'o str> {
    VITE_CONFIGS
        .iter()
        .find_map(|name| project.paths.iter().copied().find(|path| *path == *name))
}

/// The string literals of `file` inside `span`.
#[must_use]
pub fn strings_within(file: &SourceFile<'_>, span: Span) -> Vec<String> {
    file.semantic()
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            AstKind::StringLiteral(literal)
                if span.start <= literal.span.start && literal.span.end <= span.end =>
            {
                Some(literal.value.as_str().to_owned())
            }
            _ => None,
        })
        .collect()
}

/// The property `name` of `object`, by its static key.
#[must_use]
pub fn property<'a, 'b>(
    object: &'b ObjectExpression<'a>,
    name: &str,
) -> Option<&'b ObjectProperty<'a>> {
    object
        .properties
        .iter()
        .find_map(|property| match property {
            ObjectPropertyKind::ObjectProperty(property)
                if property.key.static_name().as_deref() == Some(name) =>
            {
                Some(&**property)
            }
            _ => None,
        })
}

/// The literal text of the property `name` of `object`.
#[must_use]
pub fn property_text(object: &ObjectExpression<'_>, name: &str) -> Option<String> {
    property(object, name).and_then(|property| string_like(&property.value))
}

/// The object `call` takes as its first argument.
#[must_use]
pub fn first_object<'a, 'b>(call: &'b CallExpression<'a>) -> Option<&'b ObjectExpression<'a>> {
    match call.arguments.first()?.as_expression()? {
        Expression::ObjectExpression(object) => Some(object),
        _ => None,
    }
}

/// Whether `text`, a path a configuration names, is the folder `src/` (`./src`, `/src`, or a path
/// that ends in it).
#[must_use]
pub fn names_src(text: &str) -> bool {
    text.trim_end_matches('/').rsplit('/').next() == Some("src")
}

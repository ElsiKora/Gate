//! The closed literal grammars of the decorators: persisted columns, relation options and join
//! items, read as the TypeScript AST spells them. A string option is a string literal (no template,
//! no parentheses) and a boolean option a `true` or `false` keyword.

use std::collections::BTreeSet;

use gate_typescript::oxc_ast::ast::{
    Argument, ArrayExpressionElement, Expression, ObjectExpression, ObjectPropertyKind,
    PropertyKey, PropertyKind,
};
use gate_typescript::value::number_text;

use super::Vocabulary;
use super::decorators::Resolved;

/// The relation option keys a relation may set.
const RELATION_OPTION_KEYS: [&str; 6] = [
    "createForeignKeyConstraints",
    "deferrable",
    "eager",
    "nullable",
    "onDelete",
    "onUpdate",
];
const BOOLEAN_RELATION_OPTION_KEYS: [&str; 2] = ["eager", "nullable"];
const STRING_RELATION_OPTION_KEYS: [&str; 3] = ["deferrable", "onDelete", "onUpdate"];
const REQUIRED_RELATION_OPTION_KEYS: [&str; 3] = ["nullable", "onDelete", "onUpdate"];
const JOIN_ITEM_KEYS: [&str; 3] = ["foreignKeyConstraintName", "name", "referencedColumnName"];
const PRIMARY_COLUMN_DECORATORS: [&str; 2] = ["PrimaryColumn", "PrimaryGeneratedColumn"];

/// The text of a string literal, which `ts.isStringLiteral` accepts.
pub(super) fn string_literal<'a>(expression: &'a Expression<'a>) -> Option<&'a str> {
    match expression {
        Expression::StringLiteral(literal) => Some(literal.value.as_str()),
        _ => None,
    }
}

/// The value of a `true` or `false` keyword.
fn boolean_literal(expression: &Expression<'_>) -> Option<bool> {
    match expression {
        Expression::BooleanLiteral(literal) => Some(literal.value),
        _ => None,
    }
}

/// The name of a property assignment: an identifier, string
/// or number key that is not computed; `None` for any other member.
fn assignment_name(property: &ObjectPropertyKind<'_>) -> Option<String> {
    let ObjectPropertyKind::ObjectProperty(property) = property else {
        return None;
    };

    if property.kind != PropertyKind::Init
        || property.method
        || property.shorthand
        || property.computed
    {
        return None;
    }

    match &property.key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.to_string()),
        PropertyKey::StringLiteral(literal) => Some(literal.value.to_string()),
        PropertyKey::NumericLiteral(literal) => Some(number_text(literal.value)),
        _ => None,
    }
}

/// A closed object literal: its errors and the first value of each
/// key, in source order.
pub(super) struct Closed<'a> {
    pub errors: Vec<String>,
    pub values: Vec<(String, &'a Expression<'a>)>,
}

impl<'a> Closed<'a> {
    pub(super) fn get(&self, key: &str) -> Option<&'a Expression<'a>> {
        self.values
            .iter()
            .find_map(|(name, value)| (name == key).then_some(*value))
    }

    fn has(&self, key: &str) -> bool {
        self.values.iter().any(|(name, _)| name == key)
    }
}

/// Only unique, noncomputed property assignments, and with `allowed` only
/// those keys; `__proto__` is never allowed. An empty key is as invalid as a computed one, as
/// `if (!key)` read it.
pub(super) fn closed<'a>(
    object: &'a ObjectExpression<'a>,
    context: &str,
    allowed: Option<&[&str]>,
) -> Closed<'a> {
    let mut errors = Vec::new();
    let mut values: Vec<(String, &'a Expression<'a>)> = Vec::new();
    let mut invalid = false;

    for property in &object.properties {
        let Some(key) = assignment_name(property).filter(|key| !key.is_empty()) else {
            invalid = true;
            continue;
        };

        if values.iter().any(|(name, _)| *name == key) {
            errors.push(format!("{context} repeats {key}"));
            continue;
        }

        let ObjectPropertyKind::ObjectProperty(assignment) = property else {
            continue;
        };
        values.push((key.clone(), &assignment.value));

        if key == "__proto__" || allowed.is_some_and(|allowed| !allowed.contains(&key.as_str())) {
            errors.push(format!(
                "{context} has unsupported property {}",
                super::quote(&key)
            ));
        }
    }

    if invalid {
        errors.push(format!(
            "{context} must contain only unique explicit noncomputed property assignments"
        ));
    }

    Closed { errors, values }
}

/// A persisted column of a class: its member, persisted name, nullability and primary state.
#[derive(Clone, Copy, Debug)]
pub(super) struct Persisted {
    pub primary: bool,
    pub nullable: bool,
}

/// The persisted name and state of one column decorator, or the errors that leave it without one.
pub(super) fn column(
    property: Option<&str>,
    decorator: &Resolved<'_, '_, '_>,
) -> (Vec<String>, Option<(String, Persisted)>) {
    let name = format!("@{}", decorator.canonical);
    let primary_decorator = PRIMARY_COLUMN_DECORATORS.contains(&decorator.canonical);
    // An empty name is no name, as `!propertyName` read it.
    let (Some(property), Some(call)) = (property.filter(|name| !name.is_empty()), decorator.call)
    else {
        return (
            vec![format!(
                "{name} must use a supported literal column overload"
            )],
            None,
        );
    };
    let arguments: Vec<&Argument<'_>> = call.arguments.iter().collect();
    let type_name = |argument: &Argument<'_>| matches!(argument, Argument::StringLiteral(literal) if !literal.value.is_empty());
    let (supported, options) = match arguments.as_slice() {
        [] => (true, None),
        [Argument::ObjectExpression(object)] => (true, Some(&**object)),
        [only] => (type_name(only), None),
        [first, Argument::ObjectExpression(object)] if type_name(first) => (true, Some(&**object)),
        _ => (false, None),
    };

    if !supported {
        return (
            vec![format!(
                "{name} must use (), (\"type\"), ({{...}}), or (\"type\", {{...}})"
            )],
            None,
        );
    }

    let Some(options) = options else {
        return (
            Vec::new(),
            Some((
                property.to_owned(),
                Persisted {
                    primary: primary_decorator,
                    nullable: false,
                },
            )),
        );
    };
    let parsed = closed(options, &format!("{name} options"), None);
    let mut errors = parsed.errors.clone();
    let persisted = parsed.get("name");
    let nullable = parsed.get("nullable");
    let primary = parsed.get("primary");

    if persisted.is_some_and(|value| string_literal(value).is_none_or(str::is_empty)) {
        errors.push(format!(
            "{name} option name must be a nonempty string literal"
        ));
    }

    if let Some(persisted) = persisted.and_then(string_literal)
        && persisted != property
    {
        errors.push(format!(
            "{name} option name {persisted} must match persisted property {property}"
        ));
    }

    if nullable.is_some_and(|value| boolean_literal(value).is_none()) {
        errors.push(format!("{name} option nullable must be a boolean literal"));
    }

    if primary.is_some_and(|value| boolean_literal(value).is_none()) {
        errors.push(format!("{name} option primary must be a boolean literal"));
    }

    if !errors.is_empty() {
        return (errors, None);
    }

    let persisted_name = persisted
        .and_then(string_literal)
        .unwrap_or(property)
        .to_owned();

    (
        errors,
        Some((
            persisted_name,
            Persisted {
                primary: primary_decorator || primary.and_then(boolean_literal) == Some(true),
                nullable: nullable.and_then(boolean_literal) == Some(true),
            },
        )),
    )
}

/// The relation options a relation declares.
#[derive(Debug, Default)]
pub(super) struct Options {
    pub errors: Vec<String>,
    pub deferrable: Option<String>,
    pub nullable: Option<bool>,
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
}

/// The relation options an options object literal declares, and the errors in it.
pub(super) fn options(
    object: &ObjectExpression<'_>,
    decorator: &str,
    vocabulary: &Vocabulary,
) -> Options {
    let parsed = closed(
        object,
        &format!("{decorator} options"),
        Some(&RELATION_OPTION_KEYS),
    );
    let mut errors = parsed.errors.clone();

    for required in REQUIRED_RELATION_OPTION_KEYS {
        if !parsed.has(required) {
            errors.push(format!(
                "{decorator} options are missing required {required}"
            ));
        }
    }

    for (key, value) in &parsed.values {
        let key = key.as_str();

        if BOOLEAN_RELATION_OPTION_KEYS.contains(&key) && boolean_literal(value).is_none() {
            errors.push(format!(
                "{decorator} option {key} must be a boolean literal"
            ));
        } else if STRING_RELATION_OPTION_KEYS.contains(&key)
            && string_literal(value).is_none_or(str::is_empty)
        {
            errors.push(format!(
                "{decorator} option {key} must be a nonempty string literal"
            ));
        } else if key == "createForeignKeyConstraints" && boolean_literal(value) != Some(true) {
            errors.push(format!(
                "{decorator} option createForeignKeyConstraints must be literal true"
            ));
        }

        let text = string_literal(value).filter(|text| !text.is_empty());

        if (key == "onDelete" || key == "onUpdate")
            && text.is_some_and(|text| !vocabulary.actions.iter().any(|action| action == text))
        {
            errors.push(format!(
                "{decorator} option {key} must be one of {}",
                vocabulary.actions.join(", ")
            ));
        }

        if key == "deferrable"
            && text.is_some_and(|text| !vocabulary.deferrability.iter().any(|value| value == text))
        {
            errors.push(format!(
                "{decorator} option deferrable must be one of {}",
                vocabulary.deferrability.join(", ")
            ));
        }
    }

    let text = |key: &str| parsed.get(key).and_then(string_literal).map(str::to_owned);

    Options {
        errors,
        deferrable: text("deferrable"),
        nullable: parsed.get("nullable").and_then(boolean_literal),
        on_delete: text("onDelete"),
        on_update: text("onUpdate"),
    }
}

/// One join item: its local and referenced column names, and the foreign key name the first item
/// carries.
#[derive(Debug)]
pub(super) struct Item {
    pub foreign_key: Option<String>,
    pub name: String,
    pub referenced: String,
}

/// One join column object at `index`: its errors and the item it declares.
fn item(object: &ObjectExpression<'_>, index: usize) -> (Vec<String>, Option<Item>) {
    let prefix = format!("JoinColumn item {}", index + 1);
    let parsed = closed(object, &prefix, Some(&JOIN_ITEM_KEYS));
    let mut errors = parsed.errors.clone();
    let mut values = [None, None, None];

    for (slot, key) in JOIN_ITEM_KEYS.iter().enumerate() {
        let trailing_foreign_key = index > 0 && *key == "foreignKeyConstraintName";

        match parsed.get(key) {
            None if !trailing_foreign_key => errors.push(format!("{prefix} is missing {key}")),
            None => {}
            Some(value) => match string_literal(value).filter(|text| !text.is_empty()) {
                Some(text) => values[slot] = Some(text.to_owned()),
                None => errors.push(format!("{prefix} {key} must be a nonempty string literal")),
            },
        }
    }

    if index > 0 && parsed.has("foreignKeyConstraintName") {
        errors.push(format!("{prefix} must omit foreignKeyConstraintName"));
    }

    if !errors.is_empty() {
        return (errors, None);
    }

    let [foreign_key, name, referenced] = values;

    (
        errors,
        Some(Item {
            foreign_key,
            name: name.unwrap_or_default(),
            referenced: referenced.unwrap_or_default(),
        }),
    )
}

/// One inline object or a nonempty inline array of them.
pub(super) fn join(decorator: &Resolved<'_, '_, '_>) -> (Vec<String>, Vec<Item>) {
    const SHAPE: &str =
        "JoinColumn must receive exactly one inline object or nonempty inline object array";
    let items: Vec<Option<&ObjectExpression<'_>>> = match decorator.call.map(|call| &call.arguments)
    {
        Some(arguments) if arguments.len() == 1 => match &arguments[0] {
            Argument::ObjectExpression(object) => vec![Some(&**object)],
            Argument::ArrayExpression(array) if !array.elements.is_empty() => array
                .elements
                .iter()
                .map(|element| match element {
                    ArrayExpressionElement::ObjectExpression(object) => Some(&**object),
                    _ => None,
                })
                .collect(),
            _ => return (vec![SHAPE.to_owned()], Vec::new()),
        },
        _ => return (vec![SHAPE.to_owned()], Vec::new()),
    };
    let mut errors = Vec::new();
    let mut parsed = Vec::new();

    for (index, object) in items.into_iter().enumerate() {
        let Some(object) = object else {
            errors.push(format!(
                "JoinColumn item {} must be an inline object literal",
                index + 1
            ));
            continue;
        };
        let (item_errors, value) = item(object, index);
        errors.extend(item_errors);
        parsed.extend(value);
    }

    (errors, parsed)
}

/// The distinct values of a list, to tell a repeated component.
pub(super) fn repeats(values: &[&str]) -> bool {
    values.iter().collect::<BTreeSet<_>>().len() != values.len()
}

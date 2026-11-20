//! Following a value through the file that holds it, without running the code: a `const` binding to
//! its initializer, an object literal to a property through its spreads, and a construction to the
//! class it makes. What cannot be followed this way is unknown, and a rule that needs it reports
//! nothing.

use gate_typescript::binding::ImportBindings;
use gate_typescript::owners::{export_name, find_class};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    BindingPattern, Expression, IdentifierReference, ObjectExpression, ObjectPropertyKind,
    Statement, VariableDeclarationKind,
};
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::syntax::key_name;
use gate_typescript::value::unwrap;
use gate_typescript::{SourceFile, TypeScript};

use crate::sources::parsed;

/// How many bindings, spreads or classes are followed before a value counts as unknown.
const DEPTH: usize = 8;

/// The one declaration of the binding `identifier` refers to, or `None` for a global or a name
/// declared more than once.
#[must_use]
pub fn declaration(file: &SourceFile<'_>, identifier: &IdentifierReference<'_>) -> Option<NodeId> {
    let scoping = file.semantic().scoping();
    let symbol = scoping
        .get_reference(identifier.reference_id.get()?)
        .symbol_id()?;
    let declarations: Vec<NodeId> = scoping.symbol_declarations(symbol).collect();
    match declarations.as_slice() {
        [one] => Some(*one),
        _ => None,
    }
}

/// The initializer of the `const` binding `identifier` names, when it binds a single name.
fn constant_initializer<'a>(
    file: &SourceFile<'a>,
    identifier: &IdentifierReference<'_>,
) -> Option<&'a Expression<'a>> {
    let node = declaration(file, identifier)?;
    let nodes = file.semantic().nodes();
    let AstKind::VariableDeclarator(declarator) = nodes.kind(node) else {
        return None;
    };
    let AstKind::VariableDeclaration(list) = nodes.parent_kind(node) else {
        return None;
    };
    if list.kind != VariableDeclarationKind::Const
        || !matches!(declarator.id, BindingPattern::BindingIdentifier(_))
    {
        return None;
    }
    declarator.init.as_ref()
}

/// `expression` with parentheses and type assertions removed and each `const` binding of the same
/// file replaced by its initializer.
#[must_use]
pub fn constant<'e, 'a>(
    file: &SourceFile<'a>,
    expression: &'e Expression<'a>,
) -> &'e Expression<'a> {
    let mut current = unwrap(expression);
    for _ in 0..DEPTH {
        let Expression::Identifier(identifier) = current else {
            break;
        };
        let Some(initializer) = constant_initializer(file, identifier) else {
            break;
        };
        current = unwrap(initializer);
    }
    current
}

/// The object literal `expression` is, written in place or bound to a `const` of the same file.
#[must_use]
pub fn object<'e, 'a>(
    file: &SourceFile<'a>,
    expression: &'e Expression<'a>,
) -> Option<&'e ObjectExpression<'a>> {
    match constant(file, expression) {
        Expression::ObjectExpression(object) => Some(object),
        _ => None,
    }
}

/// What an object literal holds under one key.
#[derive(Debug)]
pub enum Property<'e, 'a> {
    /// The value of the last property of that name, its own or a spread one's.
    Set(&'e Expression<'a>),
    /// No property of that name, and every spread and key read.
    Absent,
    /// A spread or a computed key that cannot be read could hold it.
    Unknown,
}

/// The value `object` holds under `name`, through the spreads of same-file object literals.
#[must_use]
pub fn property<'e, 'a>(
    file: &SourceFile<'a>,
    object: &'e ObjectExpression<'a>,
    name: &str,
) -> Property<'e, 'a> {
    property_at(file, object, name, 0)
}

fn property_at<'e, 'a>(
    file: &SourceFile<'a>,
    object: &'e ObjectExpression<'a>,
    name: &str,
    depth: usize,
) -> Property<'e, 'a> {
    let mut held = Property::Absent;
    for item in &object.properties {
        match item {
            ObjectPropertyKind::ObjectProperty(entry) => {
                match key_name(&entry.key, entry.computed) {
                    Some(key) if key == name => held = Property::Set(&entry.value),
                    Some(_) => {}
                    None => held = Property::Unknown,
                }
            }
            ObjectPropertyKind::SpreadProperty(spread) => {
                let inner = if depth < DEPTH {
                    object(file, &spread.argument)
                } else {
                    None
                };
                match inner.map(|inner| property_at(file, inner, name, depth + 1)) {
                    Some(Property::Set(value)) => held = Property::Set(value),
                    Some(Property::Absent) => {}
                    Some(Property::Unknown) | None => held = Property::Unknown,
                }
            }
        }
    }
    held
}

/// Whether the module specifier names the CDK library or the constructs library.
fn construct_library(module: &str) -> bool {
    module == "aws-cdk-lib" || module.starts_with("aws-cdk-lib/") || module == "constructs"
}

/// Whether `new callee(…)` in `path` makes a construct: the class comes from `aws-cdk-lib` (or
/// `constructs`), or is a class of the app, in this file or an imported one, that extends one.
///
/// # Errors
/// When an imported file cannot be read or the tsconfig cannot be read.
pub fn is_construct<'a>(
    typescript: &TypeScript<'_>,
    path: &str,
    file: &SourceFile<'a>,
    callee: &Expression<'a>,
) -> gate_sdk::Result<bool> {
    constructs(typescript, path, file, callee, 0)
}

fn constructs<'a>(
    typescript: &TypeScript<'_>,
    path: &str,
    file: &SourceFile<'a>,
    callee: &Expression<'a>,
    depth: usize,
) -> gate_sdk::Result<bool> {
    if depth > DEPTH {
        return Ok(false);
    }
    let bindings = ImportBindings::any(file, false);
    if let Some(export) = bindings.resolve(callee) {
        let Some(module) = bindings.reached() else {
            return Ok(false);
        };
        if construct_library(&module) {
            return Ok(true);
        }
        let [name] = export.as_slice() else {
            return Ok(false);
        };
        return exported_class_constructs(typescript, path, &module, name, depth + 1);
    }
    let Expression::Identifier(identifier) = unwrap(callee) else {
        return Ok(false);
    };
    class_constructs(typescript, path, file, identifier.name.as_str(), depth + 1)
}

/// Whether the class `name` that `specifier`, imported by `importer`, exports extends a construct.
fn exported_class_constructs(
    typescript: &TypeScript<'_>,
    importer: &str,
    specifier: &str,
    name: &str,
    depth: usize,
) -> gate_sdk::Result<bool> {
    if depth > DEPTH {
        return Ok(false);
    }
    let Some(target) = typescript.resolve(importer, specifier)? else {
        return Ok(false);
    };
    let Some(file) = parsed(typescript, &target)? else {
        return Ok(false);
    };
    if find_class(&file, name).is_some() {
        return class_constructs(typescript, &target, &file, name, depth + 1);
    }
    for statement in &file.program().body {
        match statement {
            Statement::ExportAllDeclaration(all) if all.exported.is_none() => {
                if exported_class_constructs(
                    typescript,
                    &target,
                    all.source.value.as_str(),
                    name,
                    depth + 1,
                )? {
                    return Ok(true);
                }
            }
            Statement::ExportFromDeclaration(from) => {
                for specifier in &from.specifiers {
                    if export_name(&specifier.exported) == name {
                        let local = export_name(&specifier.local);
                        return exported_class_constructs(
                            typescript,
                            &target,
                            from.source.value.as_str(),
                            &local,
                            depth + 1,
                        );
                    }
                }
            }
            _ => {}
        }
    }
    Ok(false)
}

/// Whether the top-level class `name` of `file` extends a construct.
fn class_constructs(
    typescript: &TypeScript<'_>,
    path: &str,
    file: &SourceFile<'_>,
    name: &str,
    depth: usize,
) -> gate_sdk::Result<bool> {
    let Some(superclass) = find_class(file, name).and_then(|class| class.super_class.as_ref())
    else {
        return Ok(false);
    };
    constructs(typescript, path, file, superclass, depth)
}

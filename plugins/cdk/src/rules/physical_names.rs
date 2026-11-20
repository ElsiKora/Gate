//! `cdk/physical-names`: a name AWS gives a resource (`bucketName`, `exportName`, …) is never
//! written in place; it is the result of a `lib/naming/` function. A string built in place is a
//! literal, a template, a concatenation with one or a call of anything else, read in place or
//! through a `const` of the same file; the props are read in place or through a `const` object
//! literal of the same file, with its spreads.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Expression, ObjectExpression, ObjectProperty, ObjectPropertyKind,
};
use gate_typescript::oxc_syntax::operator::BinaryOperator;
use gate_typescript::syntax::key_name;
use gate_typescript::{SourceFile, TypeScript};

use crate::follow::{constant, is_construct, object};
use crate::options::PluginOptions;
use crate::rules::{Found, argument, is_naming_call, report};
use crate::sources::{Cdk, app_files, parsed};
use crate::standard::PHYSICAL_NAMES;

/// The rule.
#[derive(Debug, Default)]
pub struct PhysicalNames;

impl Rule<Cdk> for PhysicalNames {
    type Options = NoOptions;

    const NAME: &'static str = "physical-names";
    const TITLE: &'static str =
        "Resource names and export names come from a lib/naming/ function, never written in place";
    const ASPECTS: &'static [&'static str] = &["literal"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        PluginOptions::read(run)?;
        run.with_call(|run, shared| {
            let typescript = shared.typescript()?;
            for path in app_files(typescript) {
                let Some(file) = parsed(typescript, path)? else {
                    continue;
                };
                for found in judge(typescript, path, &file)? {
                    report(run, found);
                }
            }
            Ok(())
        })
    }
}

fn judge<'p>(
    typescript: &TypeScript<'_>,
    path: &'p str,
    file: &SourceFile<'_>,
) -> gate_sdk::Result<Vec<Found<'p>>> {
    let mut found = Vec::new();

    for node in file.semantic().nodes().iter() {
        let AstKind::NewExpression(construction) = node.kind() else {
            continue;
        };
        let Some(properties) =
            argument(&construction.arguments, 2).and_then(|props| object(file, props))
        else {
            continue;
        };
        if !is_construct(typescript, path, file, &construction.callee)? {
            continue;
        }
        for property in own_properties(file, properties, 0) {
            let Some(name) = key_name(&property.key, property.computed) else {
                continue;
            };
            if !PHYSICAL_NAMES.contains(&name.as_str())
                || !written(typescript, path, file, &property.value, 0)?
            {
                continue;
            }
            found.push(Found {
                aspect: "literal",
                path,
                line: Some(file.line(property.span.start)),
                key: name.clone(),
                message: format!("{path} sets {name} in place, not through a lib/naming/ function"),
                help: "build the name with the naming function that gives every name its project and environment",
            });
        }
    }

    Ok(found)
}

/// The properties of `literal` and of the same-file object literals it spreads.
fn own_properties<'e, 'a>(
    file: &SourceFile<'a>,
    literal: &'e ObjectExpression<'a>,
    depth: usize,
) -> Vec<&'e ObjectProperty<'a>> {
    let mut properties: Vec<&'e ObjectProperty<'a>> = Vec::new();
    for item in &literal.properties {
        match item {
            ObjectPropertyKind::ObjectProperty(property) => properties.push(property),
            ObjectPropertyKind::SpreadProperty(spread) if depth < 4 => {
                if let Some(inner) = object(file, &spread.argument) {
                    properties.extend(own_properties(file, inner, depth + 1));
                }
            }
            ObjectPropertyKind::SpreadProperty(_) => {}
        }
    }
    properties
}

/// Whether `value` is a string built in place: a literal, a template, a concatenation with one,
/// a call of anything but a `lib/naming/` function, or a same-file `const` bound to one of those.
fn written<'a>(
    typescript: &TypeScript<'_>,
    path: &str,
    file: &SourceFile<'a>,
    value: &Expression<'a>,
    depth: usize,
) -> gate_sdk::Result<bool> {
    if depth > 4 {
        return Ok(false);
    }
    Ok(match constant(file, value) {
        Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => true,
        Expression::BinaryExpression(binary) if binary.operator == BinaryOperator::Addition => {
            written(typescript, path, file, &binary.left, depth + 1)?
                || written(typescript, path, file, &binary.right, depth + 1)?
        }
        call @ Expression::CallExpression(_) => !is_naming_call(typescript, path, file, call)?,
        _ => false,
    })
}

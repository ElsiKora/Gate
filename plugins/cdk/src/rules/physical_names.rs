//! `cdk/physical-names`: a name AWS gives a resource (`bucketName`, `exportName`, …) is never
//! written in place; it is the result of a `lib/naming/` function.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{Expression, ObjectPropertyKind};
use gate_typescript::syntax::key_name;
use gate_typescript::value::unwrap;
use gate_typescript::{SourceFile, TypeScript};

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
        let Some(Expression::ObjectExpression(properties)) =
            argument(&construction.arguments, 2).map(unwrap)
        else {
            continue;
        };
        for property in &properties.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                continue;
            };
            let Some(name) = key_name(&property.key, property.computed) else {
                continue;
            };
            if !PHYSICAL_NAMES.contains(&name.as_str()) {
                continue;
            }
            let written = match unwrap(&property.value) {
                Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => true,
                Expression::CallExpression(_) => {
                    !is_naming_call(typescript, path, file, &property.value)?
                }
                _ => false,
            };
            if written {
                found.push(Found {
                    aspect: "literal",
                    path,
                    line: Some(file.line(property.span.start)),
                    key: name.clone(),
                    message: format!(
                        "{path} sets {name} in place, not through a lib/naming/ function"
                    ),
                    help: "build the name with the naming function that gives every name its project and environment",
                });
            }
        }
    }

    Ok(found)
}

//! `cdk/stack-names`: a stack's directory, file and class name one topic; its construct id is a
//! call of a `lib/naming/` function with that topic, and its `stackName` is the same value.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{Argument, BindingPattern, Expression, ObjectPropertyKind};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::syntax::key_name;
use gate_typescript::value::unwrap;
use gate_typescript::{SourceFile, TypeScript};

use crate::options::PluginOptions;
use crate::rules::{Found, argument, imported_from, is_naming_call, report, text};
use crate::sources::{Cdk, app_files, parsed};
use crate::standard::kebab;

/// The rule.
#[derive(Debug, Default)]
pub struct StackNames;

impl Rule<Cdk> for StackNames {
    type Options = NoOptions;

    const NAME: &'static str = "stack-names";
    const TITLE: &'static str =
        "A stack's directory, file, class and id name one topic, through a lib/naming/ function";
    const ASPECTS: &'static [&'static str] = &["topic", "id", "stack-name"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        PluginOptions::read(run)?;

        for path in run.tree().paths() {
            let Some(rest) = path.strip_prefix("lib/stack/") else {
                continue;
            };
            let Some((directory, name)) = rest.split_once('/') else {
                continue;
            };
            let Some(stem) = name.strip_suffix(".stack.ts") else {
                continue;
            };
            if stem != directory {
                let message = format!(
                    "{path}: the stack's directory is {directory} but its file is {stem}; one topic names both"
                );
                report(
                    run,
                    Found {
                        aspect: "topic",
                        path: &path,
                        line: None,
                        key: stem.to_owned(),
                        message,
                        help: "name the directory, the file and the class with the same topic",
                    },
                );
            }
        }

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
        let Expression::Identifier(callee) = unwrap(&construction.callee) else {
            continue;
        };
        let class = callee.name.as_str();
        let Some(topic) = class
            .strip_suffix("Stack")
            .filter(|topic| !topic.is_empty())
        else {
            continue;
        };
        if !imported_from(typescript, path, file, class, "lib/stack/")? {
            continue;
        }
        let topic = kebab(topic);
        let line = Some(file.line(construction.span.start));
        let Some(id) = argument(&construction.arguments, 1) else {
            continue;
        };

        if !names_topic(typescript, path, file, id, &topic)? {
            found.push(Found {
                aspect: "id",
                path,
                line,
                key: class.to_owned(),
                message: format!(
                    "{path} gives {class} an id that is not a call of a lib/naming/ function with the topic \"{topic}\""
                ),
                help: "build the stack's id with the naming function of stack names, passing the topic",
            });
        }

        let Some(Expression::ObjectExpression(properties)) =
            argument(&construction.arguments, 2).map(unwrap)
        else {
            continue;
        };
        for property in &properties.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                continue;
            };
            if key_name(&property.key, property.computed).as_deref() != Some("stackName") {
                continue;
            }
            if source_text(file, id) != source_text(file, &property.value) {
                found.push(Found {
                    aspect: "stack-name",
                    path,
                    line: Some(file.line(property.span.start)),
                    key: class.to_owned(),
                    message: format!("{path} gives {class} a stackName other than its id"),
                    help: "pass the same value as the id, or leave stackName out",
                });
            }
        }
    }

    Ok(found)
}

/// Whether `id` is a `lib/naming/` call with a string argument `topic`, directly or through a
/// variable it initializes.
fn names_topic<'a>(
    typescript: &TypeScript<'_>,
    path: &str,
    file: &SourceFile<'a>,
    id: &'a Expression<'a>,
    topic: &str,
) -> gate_sdk::Result<bool> {
    let call = match unwrap(id) {
        Expression::Identifier(variable) => {
            let Some(init) = initializer(file, variable.name.as_str()) else {
                return Ok(false);
            };
            init
        }
        _ => id,
    };
    let Expression::CallExpression(naming) = unwrap(call) else {
        return Ok(false);
    };
    if !naming
        .arguments
        .iter()
        .filter_map(Argument::as_expression)
        .any(|argument| text(argument).as_deref() == Some(topic))
    {
        return Ok(false);
    }
    is_naming_call(typescript, path, file, call)
}

/// The initializer of the variable `name` declared in `file`.
fn initializer<'a>(file: &SourceFile<'a>, name: &str) -> Option<&'a Expression<'a>> {
    file.semantic()
        .nodes()
        .iter()
        .find_map(|node| match node.kind() {
            AstKind::VariableDeclarator(declarator) => match &declarator.id {
                BindingPattern::BindingIdentifier(binding) if binding.name.as_str() == name => {
                    declarator.init.as_ref()
                }
                _ => None,
            },
            _ => None,
        })
}

fn source_text<'s>(file: &'s SourceFile<'_>, expression: &Expression<'_>) -> Option<&'s str> {
    let span = unwrap(expression).span();
    file.source().get(span.start as usize..span.end as usize)
}

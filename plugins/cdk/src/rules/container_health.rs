//! `cdk/container-health`: every ECS container definition, `taskDefinition.addContainer(id, props)`
//! and `new ContainerDefinition(scope, id, props)`, gives the container a `healthCheck`.
//!
//! The props are read where they are written: an object literal in place or in a `const` of the
//! same file (with its spreads), the single returned value of a same-file function that builds
//! them, or, when they are a parameter of a same-file function that passes them through, the
//! argument of each call of that function. Props read any other way are unknown and give no
//! finding. The `taskImageOptions` of the `aws-ecs-patterns` constructs are not read: their types
//! take no container health check (aws-cdk-lib 2.277.0).

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::binding::ImportBindings;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    ArrowFunctionBody, BindingPattern, Expression, FunctionBody, IdentifierReference, Statement,
};
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::value::unwrap;

use crate::follow::{Property, constant, declaration, property};
use crate::options::PluginOptions;
use crate::rules::{Found, argument, report, text};
use crate::sources::{Cdk, app_files, parsed};

/// The rule.
#[derive(Debug, Default)]
pub struct ContainerHealth;

/// How many helpers are followed from one container definition.
const DEPTH: usize = 4;

impl Rule<Cdk> for ContainerHealth {
    type Options = NoOptions;

    const NAME: &'static str = "container-health";
    const TITLE: &'static str = "Every ECS container definition has a health check";
    const ASPECTS: &'static [&'static str] = &["missing"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        PluginOptions::read(run)?;
        run.with_call(|run, shared| {
            let typescript = shared.typescript()?;
            for path in app_files(typescript) {
                let Some(file) = parsed(typescript, path)? else {
                    continue;
                };
                for found in judge(path, &file) {
                    report(run, found);
                }
            }
            Ok(())
        })
    }
}

fn judge<'p>(path: &'p str, file: &SourceFile<'_>) -> Vec<Found<'p>> {
    let module = ImportBindings::new(file, "aws-cdk-lib/aws-ecs", false);
    let library = ImportBindings::new(file, "aws-cdk-lib", false);
    if module.is_none() && library.is_none() {
        return Vec::new();
    }
    let mut found = Vec::new();

    for node in file.semantic().nodes().iter() {
        let (id, props, start) = match node.kind() {
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(member) = unwrap(&call.callee) else {
                    continue;
                };
                if member.property.name != "addContainer" {
                    continue;
                }
                (
                    argument(&call.arguments, 0),
                    argument(&call.arguments, 1),
                    call.span.start,
                )
            }
            AstKind::NewExpression(construction) => {
                let definition = module
                    .as_ref()
                    .and_then(|bindings| bindings.resolve(&construction.callee))
                    .is_some_and(|export| export == ["ContainerDefinition"])
                    || library
                        .as_ref()
                        .and_then(|bindings| bindings.resolve(&construction.callee))
                        .is_some_and(|export| export == ["aws_ecs", "ContainerDefinition"]);
                if !definition {
                    continue;
                }
                (
                    argument(&construction.arguments, 1),
                    argument(&construction.arguments, 2),
                    construction.span.start,
                )
            }
            _ => continue,
        };
        let Some(props) = props else {
            continue;
        };
        let name = id.and_then(text).unwrap_or_else(|| "container".to_owned());
        for line in missing(file, props, file.line(start), 0) {
            found.push(Found {
                aspect: "missing",
                path,
                line: Some(line),
                key: name.clone(),
                message: format!("{path} defines the container {name} without a healthCheck"),
                help: "give the container a healthCheck (command, interval, timeout, retries) so ECS replaces a container that stops answering",
            });
        }
    }

    found
}

/// The lines of the container definitions `props` (at `line`) leaves without a health check: none
/// when it has one or cannot be read.
fn missing<'a>(file: &SourceFile<'a>, props: &Expression<'a>, line: u32, depth: usize) -> Vec<u32> {
    if depth > DEPTH {
        return Vec::new();
    }
    match constant(file, props) {
        Expression::ObjectExpression(object) => match property(file, object, "healthCheck") {
            Property::Absent => vec![line],
            Property::Set(_) | Property::Unknown => Vec::new(),
        },
        Expression::CallExpression(call) => returned(file, &call.callee)
            .map_or_else(Vec::new, |value| missing(file, value, line, depth + 1)),
        Expression::Identifier(identifier) => passed(file, identifier)
            .into_iter()
            .flat_map(|(call_line, value)| missing(file, value, call_line, depth + 1))
            .collect(),
        _ => Vec::new(),
    }
}

/// The one value the same-file function `callee` returns.
fn returned<'a>(file: &SourceFile<'a>, callee: &Expression<'a>) -> Option<&'a Expression<'a>> {
    let Expression::Identifier(identifier) = unwrap(callee) else {
        return None;
    };
    let nodes = file.semantic().nodes();
    match nodes.kind(declaration(file, identifier)?) {
        AstKind::Function(function) => single_return(function.body.as_ref()?),
        AstKind::VariableDeclarator(declarator) => match unwrap(declarator.init.as_ref()?) {
            Expression::ArrowFunctionExpression(arrow) => match &arrow.body {
                ArrowFunctionBody::FunctionBody(body) => single_return(body),
                body => body.as_expression(),
            },
            Expression::FunctionExpression(function) => single_return(function.body.as_ref()?),
            _ => None,
        },
        _ => None,
    }
}

/// The value of a body that is one `return` statement.
fn single_return<'a>(body: &'a FunctionBody<'a>) -> Option<&'a Expression<'a>> {
    match body.statements.as_slice() {
        [Statement::ReturnStatement(statement)] => statement.argument.as_ref(),
        _ => None,
    }
}

/// When `identifier` is a parameter of a same-file function, the line and the argument of each
/// call of that function in the file that passes it.
fn passed<'a>(
    file: &SourceFile<'a>,
    identifier: &IdentifierReference<'a>,
) -> Vec<(u32, &'a Expression<'a>)> {
    let Some((function, index)) = parameter(file, identifier) else {
        return Vec::new();
    };
    let mut calls = Vec::new();
    for node in file.semantic().nodes().iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        let Expression::Identifier(callee) = unwrap(&call.callee) else {
            continue;
        };
        if declaration(file, callee) != Some(function) {
            continue;
        }
        if let Some(value) = argument(&call.arguments, index) {
            calls.push((file.line(call.span.start), value));
        }
    }
    calls
}

/// The declaration of the same-file function whose parameter `identifier` is, and the index of
/// that parameter.
fn parameter(
    file: &SourceFile<'_>,
    identifier: &IdentifierReference<'_>,
) -> Option<(NodeId, usize)> {
    let nodes = file.semantic().nodes();
    let declared = declaration(file, identifier)?;
    let AstKind::FormalParameter(declared_parameter) = nodes.kind(declared) else {
        return None;
    };
    let list = nodes.parent_id(declared);
    let AstKind::FormalParameters(parameters) = nodes.kind(list) else {
        return None;
    };
    let index = parameters
        .items
        .iter()
        .position(|item| item.span == declared_parameter.span)?;
    let owner = nodes.parent_id(list);
    let binding = match nodes.kind(owner) {
        AstKind::Function(function) if function.id.is_some() => owner,
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
            let holder = nodes.parent_id(owner);
            match nodes.kind(holder) {
                AstKind::VariableDeclarator(declarator)
                    if matches!(declarator.id, BindingPattern::BindingIdentifier(_)) =>
                {
                    holder
                }
                _ => return None,
            }
        }
        _ => return None,
    };
    Some((binding, index))
}

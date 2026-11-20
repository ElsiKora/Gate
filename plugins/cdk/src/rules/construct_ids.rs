//! `cdk/construct-ids`: a construct id written in place is Pascal case, and no two constructs of
//! one scope in one function share a literal id.

use std::collections::HashSet;

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::Expression;
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::value::unwrap;

use crate::options::PluginOptions;
use crate::rules::{Found, argument, report};
use crate::sources::{Cdk, app_files, parsed};
use crate::standard::is_pascal;

/// The rule.
#[derive(Debug, Default)]
pub struct ConstructIds;

impl Rule<Cdk> for ConstructIds {
    type Options = NoOptions;

    const NAME: &'static str = "construct-ids";
    const TITLE: &'static str =
        "Construct ids are Pascal case, and one scope gives no two constructs the same id";
    const ASPECTS: &'static [&'static str] = &["case", "duplicate"];
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
    let nodes = file.semantic().nodes();
    let mut found = Vec::new();
    let mut seen: HashSet<(Option<NodeId>, String, String)> = HashSet::new();

    for node in nodes.iter() {
        let AstKind::NewExpression(construction) = node.kind() else {
            continue;
        };
        let Some(scope) = argument(&construction.arguments, 0).map(unwrap) else {
            continue;
        };
        if !matches!(
            scope,
            Expression::ThisExpression(_) | Expression::Identifier(_)
        ) {
            continue;
        }
        let Some(id) = argument(&construction.arguments, 1).map(unwrap) else {
            continue;
        };
        let line = Some(file.line(id.span().start));

        let (shown, pascal, literal) = match id {
            Expression::StringLiteral(literal) => {
                let value = literal.value.as_str();
                (value.to_owned(), is_pascal(value), true)
            }
            Expression::TemplateLiteral(template) => {
                let parts: Vec<&str> = template
                    .quasis
                    .iter()
                    .map(|quasi| quasi.value.raw.as_str())
                    .collect();
                let pascal = parts
                    .first()
                    .is_some_and(|first| first.starts_with(|c: char| c.is_ascii_uppercase()))
                    && parts
                        .iter()
                        .all(|part| part.chars().all(|c| c.is_ascii_alphanumeric()));
                (parts.join("${…}"), pascal, false)
            }
            _ => continue,
        };

        if !pascal {
            found.push(Found {
                aspect: "case",
                path,
                line,
                key: shown.clone(),
                message: format!("{path} gives a construct the id \"{shown}\", which is not Pascal case"),
                help: "write the id in Pascal case, such as VpcEndpoints; a changed id replaces the resource, so rename it with care",
            });
        }

        if literal {
            let function = nodes.ancestor_ids(node.id()).find(|ancestor| {
                matches!(
                    nodes.kind(*ancestor),
                    AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
                )
            });
            let scope_text = file
                .source()
                .get(scope.span().start as usize..scope.span().end as usize)
                .unwrap_or("")
                .to_owned();
            if !seen.insert((function, scope_text, shown.clone())) {
                found.push(Found {
                    aspect: "duplicate",
                    path,
                    line,
                    key: shown.clone(),
                    message: format!("{path} gives two constructs of one scope the id \"{shown}\""),
                    help: "give each construct of a scope its own id",
                });
            }
        }
    }

    found
}

//! `cdk/construct-ids`: a construct id written in place is Pascal case, and no two constructs of
//! one scope in one function share a literal id unless they lie in branches that exclude each
//! other. A construction is `new X(this | scope, id, …)` where `X` comes from `aws-cdk-lib` (or
//! `constructs`) or is a class of the app that extends one; of a template id only the literal
//! parts are judged.

use std::collections::HashMap;

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::Expression;
use gate_typescript::oxc_span::{GetSpan, Span};
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::value::unwrap;
use gate_typescript::{SourceFile, TypeScript};

use crate::follow::is_construct;
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
    let nodes = file.semantic().nodes();
    let mut found = Vec::new();
    let mut seen: HashMap<(Option<NodeId>, String, String), Vec<NodeId>> = HashMap::new();

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
                (parts.join("${…}"), template_pascal(&parts), false)
            }
            _ => continue,
        };
        if !is_construct(typescript, path, file, &construction.callee)? {
            continue;
        }
        let line = Some(file.line(id.span().start));

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
            let earlier = seen
                .entry((function, scope_text, shown.clone()))
                .or_default();
            if earlier
                .iter()
                .any(|other| !exclusive(file, node.id(), nodes.kind(*other).span()))
            {
                found.push(Found {
                    aspect: "duplicate",
                    path,
                    line,
                    key: shown.clone(),
                    message: format!("{path} gives two constructs of one scope the id \"{shown}\""),
                    help: "give each construct of a scope its own id",
                });
            }
            earlier.push(node.id());
        }
    }

    Ok(found)
}

/// Whether the literal parts of a template id are Pascal case: letters and digits only, and an
/// uppercase first letter when the id starts with a literal part.
fn template_pascal(parts: &[&str]) -> bool {
    parts
        .first()
        .is_none_or(|first| first.is_empty() || first.starts_with(|c: char| c.is_ascii_uppercase()))
        && parts
            .iter()
            .all(|part| part.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Whether the construction `node` and the one at `other` lie in branches that exclude each
/// other: the two branches of an `if` or of a conditional, or two cases of a `switch`.
fn exclusive(file: &SourceFile<'_>, node: NodeId, other: Span) -> bool {
    let nodes = file.semantic().nodes();
    let span = nodes.kind(node).span();
    let apart = |first: Span, second: Span| {
        (contains(first, span) && contains(second, other))
            || (contains(second, span) && contains(first, other))
    };
    nodes
        .ancestor_ids(node)
        .any(|ancestor| match nodes.kind(ancestor) {
            AstKind::IfStatement(statement) => statement
                .alternate
                .as_ref()
                .is_some_and(|alternate| apart(statement.consequent.span(), alternate.span())),
            AstKind::ConditionalExpression(conditional) => {
                apart(conditional.consequent.span(), conditional.alternate.span())
            }
            AstKind::SwitchCase(case) => {
                let AstKind::SwitchStatement(switch) = nodes.parent_kind(ancestor) else {
                    return false;
                };
                switch
                    .cases
                    .iter()
                    .any(|sibling| sibling.span != case.span && contains(sibling.span, other))
            }
            _ => false,
        })
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

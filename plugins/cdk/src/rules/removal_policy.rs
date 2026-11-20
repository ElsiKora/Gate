//! `cdk/removal-policy`: `RemovalPolicy.DESTROY` appears only as one branch of a choice whose other
//! branch keeps the data (a conditional, an `if` with an `else`, a `switch`), and
//! `autoDeleteObjects: true` only beside a `removalPolicy` whose `DESTROY` is such a choice.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::binding::ImportBindings;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{Expression, StaticMemberExpression};
use gate_typescript::oxc_span::{GetSpan, Span};
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::syntax::key_name;
use gate_typescript::value::unwrap;

use crate::follow::{Property, constant, property};
use crate::options::PluginOptions;
use crate::rules::{Found, report};
use crate::sources::{Cdk, app_files, parsed};

/// The rule.
#[derive(Debug, Default)]
pub struct RemovalPolicy;

impl Rule<Cdk> for RemovalPolicy {
    type Options = NoOptions;

    const NAME: &'static str = "removal-policy";
    const TITLE: &'static str =
        "Data is destroyed only on a branch of a choice whose other branch keeps it";
    const ASPECTS: &'static [&'static str] = &["destroy", "auto-delete"];
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
    let library = ImportBindings::new(file, "aws-cdk-lib", false);
    let bindings = library.as_ref();
    let nodes = file.semantic().nodes();
    let mut found = Vec::new();

    for node in nodes.iter() {
        match node.kind() {
            AstKind::StaticMemberExpression(member)
                if policy(bindings, member) == Some(Policy::Destroy)
                    && !chosen(file, bindings, node.id(), member.span) =>
            {
                found.push(Found {
                    aspect: "destroy",
                    path,
                    line: Some(file.line(member.span.start)),
                    key: "DESTROY".to_owned(),
                    message: format!(
                        "{path} sets RemovalPolicy.DESTROY without a choice that keeps the data"
                    ),
                    help: "make DESTROY one branch of a choice, from the configuration, whose other branch keeps the data (RETAIN, RETAIN_ON_UPDATE_OR_DELETE or SNAPSHOT)",
                });
            }
            AstKind::ObjectProperty(property)
                if key_name(&property.key, property.computed).as_deref()
                    == Some("autoDeleteObjects")
                    && matches!(unwrap(&property.value), Expression::BooleanLiteral(literal) if literal.value)
                    && !sibling_destroy_chosen(file, bindings, node.id()) =>
            {
                found.push(Found {
                    aspect: "auto-delete",
                    path,
                    line: Some(file.line(property.span.start)),
                    key: "autoDeleteObjects".to_owned(),
                    message: format!(
                        "{path} sets autoDeleteObjects: true without a removal policy that destroys only by choice"
                    ),
                    help: "set autoDeleteObjects beside a removalPolicy whose DESTROY is one branch of a choice that keeps the data",
                });
            }
            _ => {}
        }
    }

    found
}

/// What a `RemovalPolicy` member does to the data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Policy {
    Destroy,
    Keep,
}

/// The policy `member` names, when it is a member of `RemovalPolicy` of `aws-cdk-lib`.
fn policy<'a>(
    bindings: Option<&ImportBindings<'_, 'a>>,
    member: &StaticMemberExpression<'a>,
) -> Option<Policy> {
    let path = bindings?.resolve_static_member(member)?;
    match path.as_slice() {
        [owner, name] if owner == "RemovalPolicy" => match name.as_str() {
            "DESTROY" => Some(Policy::Destroy),
            "RETAIN" | "RETAIN_ON_UPDATE_OR_DELETE" | "SNAPSHOT" => Some(Policy::Keep),
            _ => None,
        },
        _ => None,
    }
}

/// Whether some `RemovalPolicy` member within `span` keeps the data.
fn keeps<'a>(file: &SourceFile<'a>, bindings: Option<&ImportBindings<'_, 'a>>, span: Span) -> bool {
    file.semantic()
        .nodes()
        .iter()
        .any(|node| match node.kind() {
            AstKind::StaticMemberExpression(member) => {
                contains(span, member.span) && policy(bindings, member) == Some(Policy::Keep)
            }
            _ => false,
        })
}

/// Whether the `DESTROY` at `node` (`span`) is one branch of a choice whose other branch keeps
/// the data: a conditional expression, an `if` with an `else`, or a `switch` with another case.
/// A logical expression (`policy ?? RemovalPolicy.DESTROY`) is a default, not a choice.
fn chosen<'a>(
    file: &SourceFile<'a>,
    bindings: Option<&ImportBindings<'_, 'a>>,
    node: NodeId,
    span: Span,
) -> bool {
    let nodes = file.semantic().nodes();
    for ancestor in nodes.ancestor_ids(node) {
        match nodes.kind(ancestor) {
            AstKind::ConditionalExpression(conditional) => {
                if contains(conditional.test.span(), span) {
                    continue;
                }
                let other = if contains(conditional.consequent.span(), span) {
                    conditional.alternate.span()
                } else {
                    conditional.consequent.span()
                };
                return keeps(file, bindings, other);
            }
            AstKind::IfStatement(statement) => {
                if contains(statement.test.span(), span) {
                    continue;
                }
                let other = if contains(statement.consequent.span(), span) {
                    statement.alternate.as_ref().map(GetSpan::span)
                } else {
                    Some(statement.consequent.span())
                };
                return other.is_some_and(|other| keeps(file, bindings, other));
            }
            AstKind::SwitchCase(case) => {
                let AstKind::SwitchStatement(switch) = nodes.parent_kind(ancestor) else {
                    return false;
                };
                return switch
                    .cases
                    .iter()
                    .any(|other| other.span != case.span && keeps(file, bindings, other.span));
            }
            AstKind::LogicalExpression(_)
            | AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

/// Whether the object literal that holds the property `node` sets a `removalPolicy` whose
/// `DESTROY` is chosen, read in place or through a `const` of the same file.
fn sibling_destroy_chosen<'a>(
    file: &SourceFile<'a>,
    bindings: Option<&ImportBindings<'_, 'a>>,
    node: NodeId,
) -> bool {
    let nodes = file.semantic().nodes();
    let AstKind::ObjectExpression(object) = nodes.parent_kind(node) else {
        return false;
    };
    let Property::Set(value) = property(file, object, "removalPolicy") else {
        return false;
    };
    let span = constant(file, value).span();
    let mut destroys = nodes.iter().filter(|candidate| match candidate.kind() {
        AstKind::StaticMemberExpression(member) => {
            contains(span, member.span) && policy(bindings, member) == Some(Policy::Destroy)
        }
        _ => false,
    });
    let first = destroys.next();
    first.is_some_and(|first| chosen(file, bindings, first.id(), first.kind().span()))
        && destroys.all(|other| chosen(file, bindings, other.id(), other.kind().span()))
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

//! `cdk/removal-policy`: `RemovalPolicy.DESTROY` and `autoDeleteObjects: true` appear only as one
//! branch of a choice whose other branch keeps the data.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::binding::ImportBindings;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::Expression;
use gate_typescript::oxc_span::{GetSpan, Span};
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::syntax::key_name;
use gate_typescript::value::unwrap;

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
    let bindings = ImportBindings::new(file, "aws-cdk-lib", false);
    let mut found = Vec::new();

    for node in file.semantic().nodes().iter() {
        match node.kind() {
            AstKind::StaticMemberExpression(member) => {
                let destroy = bindings
                    .as_ref()
                    .and_then(|bindings| bindings.resolve_static_member(member))
                    .is_some_and(|path| path == ["RemovalPolicy", "DESTROY"]);
                if destroy && !chosen(file, node.id(), member.span) {
                    found.push(Found {
                        aspect: "destroy",
                        path,
                        line: Some(file.line(member.span.start)),
                        key: "DESTROY".to_owned(),
                        message: format!("{path} sets RemovalPolicy.DESTROY unconditionally"),
                        help: "make DESTROY one branch of a choice, from the configuration, whose other branch keeps the data (RETAIN, RETAIN_ON_UPDATE_OR_DELETE or SNAPSHOT)",
                    });
                }
            }
            AstKind::ObjectProperty(property)
                if key_name(&property.key, property.computed).as_deref()
                    == Some("autoDeleteObjects")
                    && matches!(unwrap(&property.value), Expression::BooleanLiteral(literal) if literal.value) =>
            {
                found.push(Found {
                    aspect: "auto-delete",
                    path,
                    line: Some(file.line(property.span.start)),
                    key: "autoDeleteObjects".to_owned(),
                    message: format!("{path} sets autoDeleteObjects: true unconditionally"),
                    help: "set autoDeleteObjects from the same choice as the removal policy",
                });
            }
            _ => {}
        }
    }

    found
}

/// Whether the expression `node` at `span` is one branch of a conditional, an `if`, or a logical
/// expression within its function, and for a conditional, whether its other branch does not
/// destroy too.
fn chosen(file: &SourceFile<'_>, node: NodeId, span: Span) -> bool {
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
                let text = file
                    .source()
                    .get(other.start as usize..other.end as usize)
                    .unwrap_or("");
                return !text.contains("DESTROY");
            }
            AstKind::IfStatement(_) | AstKind::LogicalExpression(_) | AstKind::SwitchCase(_) => {
                return true;
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

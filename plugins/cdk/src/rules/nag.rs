//! `cdk/nag`: `bin/application.ts` registers each declared cdk-nag pack as a validation plugin of
//! the app, `Validations.of(app).addPlugins(new AwsSolutionsChecks(app))`, and the cdk-nag 2 form
//! (a pack added as an aspect, `NagSuppressions`) is not used.

use std::collections::BTreeSet;

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::binding::ImportBindings;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::Expression;
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::value::unwrap;

use crate::options::PluginOptions;
use crate::rules::{Found, report};
use crate::sources::{Cdk, app_files, parsed};
use crate::standard::{ENTRY, NAG_PACKS};

/// The rule.
#[derive(Debug, Default)]
pub struct Nag;

impl Rule<Cdk> for Nag {
    type Options = NoOptions;

    const NAME: &'static str = "nag";
    const TITLE: &'static str = "The app registers each declared cdk-nag pack as a validation plugin, in the cdk-nag 3 form";
    const ASPECTS: &'static [&'static str] = &["missing", "pack", "form"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        let options = PluginOptions::read(run)?;
        run.with_call(|run, shared| {
            let typescript = shared.typescript()?;
            let mut registered = BTreeSet::new();
            let mut entry_read = false;

            for path in app_files(typescript) {
                let Some(file) = parsed(typescript, path)? else {
                    continue;
                };
                let nag_uses = uses(&file);
                if path == ENTRY {
                    entry_read = true;
                    registered.extend(nag_uses.registered);
                }
                for (line, what) in nag_uses.legacy {
                    report(
                        run,
                        Found {
                            aspect: "form",
                            path,
                            line: Some(line),
                            message: format!("{path} uses {what}, the cdk-nag 2 form"),
                            key: what,
                            help: "register the pack with Validations.of(app).addPlugins(…) and acknowledge findings with Validations.of(construct).acknowledge(…)",
                        },
                    );
                }
            }

            if !entry_read {
                return Ok(());
            }
            if registered.is_empty() {
                report(
                    run,
                    Found {
                        aspect: "missing",
                        path: ENTRY,
                        line: None,
                        key: "nag".to_owned(),
                        message: format!(
                            "{ENTRY} registers no cdk-nag pack as a validation plugin of the app"
                        ),
                        help: "add Validations.of(app).addPlugins(new AwsSolutionsChecks(app, { verbose: true }))",
                    },
                );
                return Ok(());
            }
            for pack in &options.nag_packs {
                if !registered.contains(pack) {
                    report(
                        run,
                        Found {
                            aspect: "pack",
                            path: ENTRY,
                            line: None,
                            key: pack.clone(),
                            message: format!(
                                "{ENTRY} does not register the declared cdk-nag pack {pack}"
                            ),
                            help: "register the pack with Validations.of(app).addPlugins(…), or remove it from nag-packs",
                        },
                    );
                }
            }
            Ok(())
        })
    }
}

/// What one file does with cdk-nag.
struct Uses {
    /// The packs it registers as validation plugins.
    registered: Vec<String>,
    /// The cdk-nag 2 forms it uses, with their line.
    legacy: Vec<(u32, String)>,
}

fn uses(file: &SourceFile<'_>) -> Uses {
    let mut uses = Uses {
        registered: Vec::new(),
        legacy: Vec::new(),
    };
    let Some(nag) = ImportBindings::new(file, "cdk-nag", false) else {
        return uses;
    };
    let library = ImportBindings::new(file, "aws-cdk-lib", false);
    let nodes = file.semantic().nodes();

    for node in nodes.iter() {
        match node.kind() {
            AstKind::NewExpression(construction) => {
                let Some(pack) = nag
                    .resolve(&construction.callee)
                    .and_then(|path| path.first().cloned())
                    .filter(|pack| NAG_PACKS.contains(&pack.as_str()))
                else {
                    continue;
                };
                match receiver(file, node.id(), library.as_ref()) {
                    Some(Receiver::Validations) => uses.registered.push(pack),
                    Some(Receiver::Aspects) => uses.legacy.push((
                        file.line(construction.span.start),
                        format!("Aspects.of(…).add(new {pack}(…))"),
                    )),
                    None => {}
                }
            }
            AstKind::CallExpression(call) => {
                if nag
                    .resolve(&call.callee)
                    .is_some_and(|path| path.first().is_some_and(|name| name == "NagSuppressions"))
                {
                    uses.legacy
                        .push((file.line(call.span.start), "NagSuppressions".to_owned()));
                }
            }
            _ => {}
        }
    }

    uses
}

/// Where a pack construction is handed.
enum Receiver {
    /// `Validations.of(x).addPlugins(…)`.
    Validations,
    /// `Aspects.of(x).add(…)`.
    Aspects,
}

/// The call the construction `node` is an argument of, when it is `Validations.of(x).addPlugins`
/// or `Aspects.of(x).add`.
fn receiver<'a>(
    file: &SourceFile<'a>,
    node: NodeId,
    library: Option<&ImportBindings<'_, 'a>>,
) -> Option<Receiver> {
    let nodes = file.semantic().nodes();
    let call = nodes
        .ancestor_ids(node)
        .find_map(|ancestor| match nodes.kind(ancestor) {
            AstKind::CallExpression(call) => Some(call),
            _ => None,
        })?;
    let Expression::StaticMemberExpression(member) = unwrap(&call.callee) else {
        return None;
    };
    let Expression::CallExpression(of) = unwrap(&member.object) else {
        return None;
    };
    let owner = library?.resolve(&of.callee)?;
    let span = nodes.kind(node).span();
    if !call
        .arguments
        .iter()
        .any(|argument| argument.span().start <= span.start && span.end <= argument.span().end)
    {
        return None;
    }
    match (owner.as_slice(), member.property.name.as_str()) {
        ([first, second], "addPlugins") if first == "Validations" && second == "of" => {
            Some(Receiver::Validations)
        }
        ([first, second], "add") if first == "Aspects" && second == "of" => Some(Receiver::Aspects),
        _ => None,
    }
}

//! `cdk/nag`: the app depends on cdk-nag 3, `bin/application.ts` registers each declared pack as a
//! validation plugin of the App, `Validations.of(app).addPlugins(new AwsSolutionsChecks(app))`,
//! each acknowledgment gives its reason, and the cdk-nag 2 form (a pack added as an aspect,
//! `NagSuppressions`) is not used.
//!
//! A pack is read in place or through a `const` of the same file; the App is the `const` bound to
//! `new App(…)`. The pack class names are those cdk-nag 3.0.2 exports.

use std::collections::BTreeSet;

use gate_sdk::serde_json::Value;
use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::binding::ImportBindings;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{Argument, Expression};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::value::unwrap;

use crate::follow::{Property, constant, object, property};
use crate::options::PluginOptions;
use crate::rules::{Found, argument, report, text};
use crate::sources::{Cdk, app_files, parsed};
use crate::standard::{ENTRY, NAG_PACKS};

/// The rule.
#[derive(Debug, Default)]
pub struct Nag;

/// The form cdk-nag 3 registers a pack with.
const FORM: &str = "Validations.of(app).addPlugins(new AwsSolutionsChecks(app))";

impl Rule<Cdk> for Nag {
    type Options = NoOptions;

    const NAME: &'static str = "nag";
    const TITLE: &'static str =
        "The app registers each declared cdk-nag 3 pack as a validation plugin of the App";
    const ASPECTS: &'static [&'static str] = &[
        "missing",
        "pack",
        "form",
        "version",
        "target",
        "acknowledgment",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        let options = PluginOptions::read(run)?;
        if let Some(version) = run
            .tree()
            .text("package.json")
            .and_then(|package| before_three(&package))
        {
            report(
                run,
                Found {
                    aspect: "version",
                    path: "package.json",
                    line: None,
                    key: "cdk-nag".to_owned(),
                    message: format!(
                        "package.json depends on cdk-nag `{version}`, a version before 3; cdk-nag 3 registers its packs as validation plugins: {FORM}"
                    ),
                    help: "upgrade cdk-nag to 3 and register each pack with Validations.of(app).addPlugins(…)",
                },
            );
        }
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
                for found in nag_uses.found(path) {
                    report(run, found);
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
                            "{ENTRY} registers no cdk-nag pack as a validation plugin of the App"
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

/// The cdk-nag version `package.json` names when its major version is below 3; `None` when it is
/// 3 or later, missing (`cdk/app#pin` reports that) or not a version (a catalog entry).
fn before_three(package: &str) -> Option<String> {
    let document: Value = gate_sdk::serde_json::from_str(package).ok()?;
    let version = ["dependencies", "devDependencies"]
        .iter()
        .find_map(|section| document.get(section)?.get("cdk-nag")?.as_str())?;
    let digits: String = version
        .trim_start_matches(|character: char| !character.is_ascii_digit())
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let major: u32 = digits.parse().ok()?;
    (major < 3 && !version.contains(':')).then(|| version.to_owned())
}

/// What one file does with cdk-nag.
#[derive(Default)]
struct Uses {
    /// The packs it registers on the App.
    registered: Vec<String>,
    /// The cdk-nag 2 forms it uses, with their line.
    legacy: Vec<(u32, String)>,
    /// The packs it registers on something other than the App: line, pack, target.
    misplaced: Vec<(u32, String, String)>,
    /// The acknowledgments without a reason: line and rule id.
    unexplained: Vec<(u32, String)>,
}

impl Uses {
    fn found(self, path: &str) -> Vec<Found<'_>> {
        let mut found = Vec::new();
        for (line, what) in self.legacy {
            found.push(Found {
                aspect: "form",
                path,
                line: Some(line),
                message: format!("{path} uses {what}, the cdk-nag 2 form"),
                key: what,
                help: "register the pack with Validations.of(app).addPlugins(…) and acknowledge findings with Validations.of(construct).acknowledge(…)",
            });
        }
        for (line, pack, target) in self.misplaced {
            found.push(Found {
                aspect: "target",
                path,
                line: Some(line),
                message: format!(
                    "{path} registers {pack} on {target}; a pack is a validation plugin of the App"
                ),
                key: pack,
                help: "register the pack on the App: Validations.of(app).addPlugins(…)",
            });
        }
        for (line, id) in self.unexplained {
            found.push(Found {
                aspect: "acknowledgment",
                path,
                line: Some(line),
                message: format!("{path} acknowledges {id} without a reason"),
                key: id,
                help: "say in `reason` why the finding is accepted",
            });
        }
        found
    }
}

fn uses(file: &SourceFile<'_>) -> Uses {
    let mut uses = Uses::default();
    let nag = ImportBindings::new(file, "cdk-nag", false);
    let library = ImportBindings::new(file, "aws-cdk-lib", false);

    for node in file.semantic().nodes().iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        let line = file.line(call.span.start);
        if nag
            .as_ref()
            .and_then(|nag| nag.resolve(&call.callee))
            .is_some_and(|path| path.first().is_some_and(|name| name == "NagSuppressions"))
        {
            uses.legacy.push((line, "NagSuppressions".to_owned()));
            continue;
        }
        let Expression::StaticMemberExpression(member) = unwrap(&call.callee) else {
            continue;
        };
        let Expression::CallExpression(of) = unwrap(&member.object) else {
            continue;
        };
        let Some(owner) = library
            .as_ref()
            .and_then(|library| library.resolve(&of.callee))
        else {
            continue;
        };
        let target = argument(&of.arguments, 0);
        match (owner.as_slice(), member.property.name.as_str()) {
            ([first, second], "addPlugins") if first == "Validations" && second == "of" => {
                let on_app = target.is_some_and(|target| is_app(file, library.as_ref(), target));
                for pack in packs(file, nag.as_ref(), &call.arguments) {
                    if on_app {
                        uses.registered.push(pack);
                    } else {
                        let shown = target.map_or_else(
                            || "nothing".to_owned(),
                            |target| source_text(file, target),
                        );
                        uses.misplaced.push((line, pack, shown));
                    }
                }
            }
            ([first, second], "add") if first == "Aspects" && second == "of" => {
                for pack in packs(file, nag.as_ref(), &call.arguments) {
                    uses.legacy
                        .push((line, format!("Aspects.of(…).add(new {pack}(…))")));
                }
            }
            ([first, second], "acknowledge") if first == "Validations" && second == "of" => {
                for acknowledgment in call.arguments.iter().filter_map(Argument::as_expression) {
                    if let Some(id) = unexplained(file, acknowledgment) {
                        uses.unexplained.push((line, id));
                    }
                }
            }
            _ => {}
        }
    }

    uses
}

/// The cdk-nag packs `arguments` construct, in place or through a `const` of the same file.
fn packs<'a>(
    file: &SourceFile<'a>,
    nag: Option<&ImportBindings<'_, 'a>>,
    arguments: &[Argument<'a>],
) -> Vec<String> {
    let Some(nag) = nag else {
        return Vec::new();
    };
    arguments
        .iter()
        .filter_map(Argument::as_expression)
        .filter_map(|value| match constant(file, value) {
            Expression::NewExpression(construction) => nag
                .resolve(&construction.callee)
                .and_then(|path| path.first().cloned())
                .filter(|pack| NAG_PACKS.contains(&pack.as_str())),
            _ => None,
        })
        .collect()
}

/// Whether `target` is the App: `new App(…)` of `aws-cdk-lib`, in place or through a `const`.
fn is_app<'a>(
    file: &SourceFile<'a>,
    library: Option<&ImportBindings<'_, 'a>>,
    target: &Expression<'a>,
) -> bool {
    let Expression::NewExpression(construction) = constant(file, target) else {
        return false;
    };
    library
        .and_then(|library| library.resolve(&construction.callee))
        .is_some_and(|path| path == ["App"])
}

/// The rule id of an acknowledgment that gives no reason, or an empty one; `None` when it gives
/// one or cannot be read.
fn unexplained<'a>(file: &SourceFile<'a>, acknowledgment: &Expression<'a>) -> Option<String> {
    let entry = object(file, acknowledgment)?;
    let missing = match property(file, entry, "reason") {
        Property::Absent => true,
        Property::Set(reason) => text(reason).is_some_and(|reason| reason.trim().is_empty()),
        Property::Unknown => false,
    };
    missing.then(|| match property(file, entry, "id") {
        Property::Set(id) => text(id).unwrap_or_else(|| "a rule".to_owned()),
        Property::Absent | Property::Unknown => "a rule".to_owned(),
    })
}

/// The source text of `expression`.
fn source_text(file: &SourceFile<'_>, expression: &Expression<'_>) -> String {
    let span = expression.span();
    file.source()
        .get(span.start as usize..span.end as usize)
        .unwrap_or("")
        .to_owned()
}

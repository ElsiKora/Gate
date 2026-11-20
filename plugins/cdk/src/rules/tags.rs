//! `cdk/tags`: tags are applied only in `lib/naming/`, with the standard's keys and the declared
//! ones, and every standard key is applied there.

use std::collections::BTreeSet;

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::binding::ImportBindings;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::Expression;
use gate_typescript::value::unwrap;

use crate::options::PluginOptions;
use crate::rules::{Found, argument, report, text};
use crate::sources::{Cdk, app_files, parsed};
use crate::standard::TAGS;

/// The rule.
#[derive(Debug, Default)]
pub struct Tags;

impl Rule<Cdk> for Tags {
    type Options = NoOptions;

    const NAME: &'static str = "tags";
    const TITLE: &'static str =
        "Tags are applied in lib/naming/ only, with the standard's keys, each of them applied";
    const ASPECTS: &'static [&'static str] = &["place", "key", "required"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        let options = PluginOptions::read(run)?;
        run.with_call(|run, shared| {
            let typescript = shared.typescript()?;
            let mut applied = BTreeSet::new();

            for path in app_files(typescript) {
                let Some(file) = parsed(typescript, path)? else {
                    continue;
                };
                for (line, key) in tag_calls(&file) {
                    let naming = path.starts_with("lib/naming/");
                    if !naming {
                        report(
                            run,
                            Found {
                                aspect: "place",
                                path,
                                line: Some(line),
                                key: key.clone().unwrap_or_else(|| "tag".to_owned()),
                                message: format!("{path} applies a tag; tags are applied only in lib/naming/"),
                                help: "apply the tag through the tag function of lib/naming/",
                            },
                        );
                    }
                    let Some(key) = key else {
                        continue;
                    };
                    if !options.allows_tag(&key) {
                        report(
                            run,
                            Found {
                                aspect: "key",
                                path,
                                line: Some(line),
                                message: format!(
                                    "{path} applies the tag key {key}, which is neither a standard key ({}) nor declared in extra-tags",
                                    TAGS.join(", ")
                                ),
                                key: key.clone(),
                                help: "use the standard key, or declare the key in [plugins.cdk] options extra-tags with its job",
                            },
                        );
                    }
                    if naming {
                        applied.insert(key);
                    }
                }
            }

            for tag in TAGS {
                if !applied.contains(tag) {
                    report(
                        run,
                        Found {
                            aspect: "required",
                            path: "lib/naming",
                            line: None,
                            key: tag.to_owned(),
                            message: format!("lib/naming/ never applies the standard tag {tag}"),
                            help: "apply the tag in the tag function of lib/naming/",
                        },
                    );
                }
            }
            Ok(())
        })
    }
}

/// The tag applications of `file`, `Tags.of(x).add(key, …)` and `manager.setTag(key, …)`, with
/// their line and their key when it is written as a literal.
fn tag_calls(file: &SourceFile<'_>) -> Vec<(u32, Option<String>)> {
    let bindings = ImportBindings::new(file, "aws-cdk-lib", false);
    let mut calls = Vec::new();

    for node in file.semantic().nodes().iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        let Expression::StaticMemberExpression(member) = unwrap(&call.callee) else {
            continue;
        };
        let tagging = match member.property.name.as_str() {
            "setTag" => true,
            "add" => match unwrap(&member.object) {
                Expression::CallExpression(of) => bindings
                    .as_ref()
                    .and_then(|bindings| bindings.resolve(&of.callee))
                    .is_some_and(|path| path == ["Tags", "of"]),
                _ => false,
            },
            _ => false,
        };
        if tagging {
            calls.push((
                file.line(call.span.start),
                argument(&call.arguments, 0).and_then(text),
            ));
        }
    }

    calls
}

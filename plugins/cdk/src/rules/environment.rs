//! `cdk/environment`: no account id or unlisted region in the tree, the configuration file named
//! by the one context key, no process environment read outside `lib/config/`, and no real
//! configuration tracked.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::Expression;
use gate_typescript::value::unwrap;

use crate::options::PluginOptions;
use crate::rules::{Found, argument, report, text};
use crate::sources::{Cdk, code_files, parsed};
use crate::standard::{CONTEXT_KEY, account_ids, regions};

/// The rule.
#[derive(Debug, Default)]
pub struct Environment;

const ACCOUNT_HELP: &str = "read the account from the configuration file";

impl Rule<Cdk> for Environment {
    type Options = NoOptions;

    const NAME: &'static str = "environment";
    const TITLE: &'static str = "Accounts, regions and environment values come from the configuration file, never from the code";
    const ASPECTS: &'static [&'static str] = &[
        "account",
        "region",
        "context-key",
        "process-env",
        "tracked-configuration",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        let options = PluginOptions::read(run)?;
        let tree = run.tree();
        let mut found: Vec<Owned> = Vec::new();

        for path in tree.paths() {
            if path.starts_with("deploy/")
                && path.ends_with(".json")
                && !path.ends_with(".config.example.json")
            {
                found.push(Owned {
                    aspect: "tracked-configuration",
                    path: path.clone(),
                    line: None,
                    key: path.clone(),
                    message: format!(
                        "{path} is a configuration file; only <environment>.config.example.json is tracked"
                    ),
                    help: "untrack the file (ignore deploy/*.config.json) and keep an example beside it",
                });
            }
            if path.ends_with(".json")
                && !path.ends_with("package-lock.json")
                && !path.split('/').any(|segment| segment == "node_modules")
                && let Some(contents) = tree.text(&path)
            {
                for (index, line) in contents.lines().enumerate() {
                    for id in account_ids(line) {
                        found.push(Owned {
                            aspect: "account",
                            line: u32::try_from(index + 1).ok(),
                            message: format!("{path} holds the account id {id}"),
                            path: path.clone(),
                            key: id,
                            help: ACCOUNT_HELP,
                        });
                    }
                }
            }
        }

        run.with_call(|run, shared| {
            let typescript = shared.typescript()?;
            let mut files = code_files(typescript, "bin/");
            files.extend(code_files(typescript, "lib/"));
            files.extend(code_files(typescript, "scripts/"));

            for path in files {
                let Some(file) = parsed(typescript, path)? else {
                    continue;
                };
                literals(&file, path, &options, &mut found);
                if !path.starts_with("scripts/") {
                    reads(&file, path, &mut found);
                }
            }

            for entry in found {
                report(
                    run,
                    Found {
                        aspect: entry.aspect,
                        path: &entry.path,
                        line: entry.line,
                        key: entry.key,
                        message: entry.message,
                        help: entry.help,
                    },
                );
            }
            Ok(())
        })
    }
}

/// A finding before it is reported.
struct Owned {
    aspect: &'static str,
    path: String,
    line: Option<u32>,
    key: String,
    message: String,
    help: &'static str,
}

/// The account ids and the regions in the string and template literals of `file`.
fn literals(file: &SourceFile<'_>, path: &str, options: &PluginOptions, found: &mut Vec<Owned>) {
    for node in file.semantic().nodes().iter() {
        let (texts, start): (Vec<&str>, u32) = match node.kind() {
            AstKind::StringLiteral(literal) => (vec![literal.value.as_str()], literal.span.start),
            AstKind::TemplateLiteral(template) => (
                template
                    .quasis
                    .iter()
                    .map(|quasi| quasi.value.raw.as_str())
                    .collect(),
                template.span.start,
            ),
            _ => continue,
        };
        let line = Some(file.line(start));

        for value in texts {
            for id in account_ids(value) {
                found.push(Owned {
                    aspect: "account",
                    path: path.to_owned(),
                    line,
                    message: format!("{path} holds the account id {id} in a literal"),
                    key: id,
                    help: ACCOUNT_HELP,
                });
            }
            for region in regions(value) {
                if options.regions.iter().any(|allowed| allowed == region) {
                    continue;
                }
                found.push(Owned {
                    aspect: "region",
                    path: path.to_owned(),
                    line,
                    key: region.to_owned(),
                    message: format!(
                        "{path} names the region {region} in a literal; allowed: {}",
                        options.regions.join(", ")
                    ),
                    help: "read the region from the configuration file, or list it in [plugins.cdk] options regions",
                });
            }
        }
    }
}

/// The context reads and the process environment reads of `file`.
fn reads(file: &SourceFile<'_>, path: &str, found: &mut Vec<Owned>) {
    for node in file.semantic().nodes().iter() {
        match node.kind() {
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(member) = unwrap(&call.callee) else {
                    continue;
                };
                let method = member.property.name.as_str();
                if method != "tryGetContext" && method != "getContext" {
                    continue;
                }
                let key = argument(&call.arguments, 0).and_then(text);
                if key.as_deref() == Some(CONTEXT_KEY) {
                    continue;
                }
                let shown = key.unwrap_or_else(|| "<expression>".to_owned());
                found.push(Owned {
                    aspect: "context-key",
                    path: path.to_owned(),
                    line: Some(file.line(call.span.start)),
                    message: format!(
                        "{path} reads the context key {shown}; the app reads only `{CONTEXT_KEY}`, the configuration file"
                    ),
                    key: shown,
                    help: "move the value into the configuration file",
                });
            }
            AstKind::IdentifierReference(reference)
                if reference.name.as_str() == "process" && !path.starts_with("lib/config/") =>
            {
                found.push(Owned {
                    aspect: "process-env",
                    path: path.to_owned(),
                    line: Some(file.line(reference.span.start)),
                    key: "process".to_owned(),
                    message: format!(
                        "{path} reads the process environment; only lib/config/ reads it"
                    ),
                    help: "read the value from the configuration, which lib/config/ builds",
                });
            }
            _ => {}
        }
    }
}

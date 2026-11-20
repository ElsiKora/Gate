//! `frontend/api-boundary`: only the API segment of the shared layer talks to the network, and only
//! `api` segments and the app layer import it.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::call::TypeScriptCall;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::Expression;

use crate::code::{Project, imports, is_global};
use crate::options::PACK;
use crate::rules::{Found, report};
use crate::standard::{Unit, place, within};

/// The rule.
#[derive(Debug, Default)]
pub struct ApiBoundary;

/// The options of `frontend/api-boundary`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The HTTP packages; an import of one, or of a subpath of it, is a network call.
    pub packages: Vec<String>,
    /// The globals that reach the network, also as members of `window`, `globalThis` and `self`.
    pub globals: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            packages: [
                "axios",
                "ky",
                "ofetch",
                "cross-fetch",
                "node-fetch",
                "isomorphic-fetch",
                "undici",
                "graphql-request",
            ]
            .map(str::to_owned)
            .to_vec(),
            globals: ["fetch", "XMLHttpRequest", "WebSocket", "EventSource"]
                .map(str::to_owned)
                .to_vec(),
        }
    }
}

/// The globals whose members are the globals of a browser.
const GLOBAL_OBJECTS: [&str; 3] = ["window", "globalThis", "self"];

impl Rule<TypeScriptCall> for ApiBoundary {
    type Options = Options;

    const NAME: &'static str = "api-boundary";
    const TITLE: &'static str = "Only the API segment of the shared layer talks to the network";
    const ASPECTS: &'static [&'static str] = &["raw-call", "client-import"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, Options, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let found = check(&project, run.options())?;
            report(run, found);
            Ok(())
        })
    }
}

fn check(project: &Project<'_>, options: &Options) -> gate_sdk::Result<Vec<Found>> {
    let api = project.pack.api_folder();
    let mut found = Vec::new();

    for path in project.code() {
        if within(path, api) {
            continue;
        }
        let Some(file) = project.parse(path)? else {
            continue;
        };

        for import in imports(&file) {
            let specifier = import.specifier.as_str();
            let package = options.packages.iter().find(|package| {
                specifier == package.as_str()
                    || specifier
                        .strip_prefix(package.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            });
            if let Some(package) = package {
                if !import.type_only {
                    found.push(
                        Found::at(
                            "raw-call",
                            path,
                            format!("{path} imports the HTTP package {package} outside {api}/"),
                            format!("call the endpoint through the api segment of its slice, which uses the client in {api}/"),
                        )
                        .line(import.line)
                        .named(package),
                    );
                }
                continue;
            }

            if client_importer(project, path) {
                continue;
            }
            let Some(target) = project.resolve(path, specifier)? else {
                continue;
            };
            if within(&target, api) {
                found.push(
                    Found::at(
                        "client-import",
                        path,
                        format!("{path} imports the API client {specifier} outside an api segment"),
                        "call an endpoint of the api segment of a slice; only api segments and src/app/ use the client",
                    )
                    .line(import.line)
                    .named(specifier),
                );
            }
        }

        for node in file.semantic().nodes().iter() {
            let global = match node.kind() {
                AstKind::IdentifierReference(identifier)
                    if options
                        .globals
                        .iter()
                        .any(|name| identifier.name.as_str() == name)
                        && is_global(&file, identifier) =>
                {
                    Some((identifier.name.as_str(), identifier.span.start))
                }
                AstKind::StaticMemberExpression(member)
                    if options
                        .globals
                        .iter()
                        .any(|name| member.property.name.as_str() == name) =>
                {
                    match &member.object {
                        Expression::Identifier(object)
                            if GLOBAL_OBJECTS.contains(&object.name.as_str())
                                && is_global(&file, object) =>
                        {
                            Some((member.property.name.as_str(), member.span.start))
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some((name, start)) = global {
                found.push(
                    Found::at(
                        "raw-call",
                        path,
                        format!("{path} calls the network through {name} outside {api}/"),
                        format!("call the endpoint through the api segment of its slice, which uses the client in {api}/"),
                    )
                    .line(file.line(start))
                    .named(name),
                );
            }
        }
    }

    Ok(found)
}

/// Whether the file at `path` may import the API client: an `api` segment, the app layer, or a
/// library's entry, which re-exports it.
fn client_importer(project: &Project<'_>, path: &str) -> bool {
    place(project.pack, path).is_some_and(|place| {
        place.unit == Unit::App
            || place.segment == Some("api")
            || (project.pack.library() && place.unit == Unit::Root)
    })
}

//! `frontend/state-placement`: a store is created in a `model` segment or in the app layer.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::binding::ImportBindings;
use gate_typescript::call::TypeScriptCall;
use gate_typescript::oxc_ast::AstKind;

use crate::code::Project;
use crate::options::PACK;
use crate::rules::{Found, report};
use crate::standard::{Unit, place};

/// The rule.
#[derive(Debug, Default)]
pub struct StatePlacement;

/// A function of a package that creates a store.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Store {
    /// The package, matched exactly.
    pub package: String,
    /// The exported function.
    pub export: String,
}

/// The options of `frontend/state-placement`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The functions that create a store.
    pub stores: Vec<Store>,
}

impl Default for Options {
    fn default() -> Self {
        let store = |package: &str, export: &str| Store {
            package: package.to_owned(),
            export: export.to_owned(),
        };
        Self {
            stores: vec![
                store("zustand", "create"),
                store("@reduxjs/toolkit", "createSlice"),
                store("@reduxjs/toolkit", "configureStore"),
                store("recoil", "atom"),
                store("jotai", "atom"),
                store("mobx", "makeAutoObservable"),
            ],
        }
    }
}

impl Rule<TypeScriptCall> for StatePlacement {
    type Options = Options;

    const NAME: &'static str = "state-placement";
    const TITLE: &'static str = "Stores are created in a model segment or the app layer";
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, Options, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let found = check(&project, &run.options().stores)?;
            report(run, found);
            Ok(())
        })
    }
}

fn check(project: &Project<'_, '_>, stores: &[Store]) -> gate_sdk::Result<Vec<Found>> {
    let mut found = Vec::new();

    for path in project.code() {
        let placed = place(project.pack, path)
            .is_some_and(|place| place.unit == Unit::App || place.segment == Some("model"));
        if placed {
            continue;
        }
        let Some(file) = project.parse(path)? else {
            continue;
        };

        for store in stores {
            let Some(bindings) = ImportBindings::new(&file, &store.package, false) else {
                continue;
            };
            for node in file.semantic().nodes().iter() {
                let AstKind::CallExpression(call) = node.kind() else {
                    continue;
                };
                let denotes = bindings
                    .resolve(&call.callee)
                    .is_some_and(|names| names == [store.export.as_str()]);
                if denotes {
                    found.push(
                        Found::at(
                            "",
                            path,
                            format!(
                                "{path} creates a store with {} of {} outside a model segment and the app layer",
                                store.export, store.package
                            ),
                            "create the store in the model segment of its slice, or the root store in src/app/",
                        )
                        .line(file.line(call.span.start))
                        .named(&store.export),
                    );
                }
            }
        }
    }

    Ok(found)
}

//! `frontend/import-alias`: the one alias `@/` is declared where the code is checked and bundled:
//! `"@/*": ["./src/*"]` in the tsconfig `paths` and no other alias, and, when the project has a Vite
//! configuration, an `@` alias to `src/` in it or the `vite-tsconfig-paths` plugin.

use gate_sdk::serde_json::Value;
use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::binding::ImportBindings;
use gate_typescript::call::TypeScriptCall;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::Expression;
use gate_typescript::oxc_span::GetSpan;

use crate::code::Project;
use crate::config::{
    TSCONFIG, names_src, property, property_text, strings_within, tsconfig_paths, vite_config,
};
use crate::options::PACK;
use crate::rules::{Found, report};

/// The rule.
#[derive(Debug, Default)]
pub struct ImportAlias;

/// The alias of `src/` as the tsconfig `paths` declare it.
const PATHS_KEY: &str = "@/*";

/// The plugin that makes Vite read the tsconfig `paths`.
const TSCONFIG_PATHS_PLUGIN: &str = "vite-tsconfig-paths";

impl Rule<TypeScriptCall> for ImportAlias {
    type Options = NoOptions;

    const NAME: &'static str = "import-alias";
    const TITLE: &'static str = "The alias @/ for src/ is declared in the tsconfig and the Vite configuration, and no other";
    const ASPECTS: &'static [&'static str] = &["tsconfig", "vite"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, NoOptions, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let mut found = tsconfig(&project)?;
            found.extend(vite(&project)?);
            report(run, found);
            Ok(())
        })
    }
}

/// The tsconfig `paths`: `@/*` to `./src/*`, and nothing else.
fn tsconfig(project: &Project<'_, '_>) -> gate_sdk::Result<Vec<Found>> {
    let mut found = Vec::new();
    let help = format!(
        "declare \"{PATHS_KEY}\": [\"./src/*\"] in compilerOptions.paths of {TSCONFIG}, and no other alias"
    );

    let Some(paths) = tsconfig_paths(project)? else {
        found.push(
            Found::at(
                "tsconfig",
                TSCONFIG,
                format!("{TSCONFIG} declares no compilerOptions.paths, so @/ resolves nowhere"),
                help,
            )
            .named(PATHS_KEY),
        );
        return Ok(found);
    };

    for (key, value) in &paths {
        if key != PATHS_KEY {
            found.push(
                Found::at(
                    "tsconfig",
                    TSCONFIG,
                    format!("{TSCONFIG} declares the alias {key}; the one alias is {PATHS_KEY}"),
                    format!("import through @/ and remove {key} from compilerOptions.paths"),
                )
                .named(key),
            );
        } else if !to_src(value) {
            found.push(
                Found::at(
                    "tsconfig",
                    TSCONFIG,
                    format!("{TSCONFIG} maps {PATHS_KEY} to {value}, not to ./src/*"),
                    help.clone(),
                )
                .named(PATHS_KEY),
            );
        }
    }
    if !paths.contains_key(PATHS_KEY) {
        found.push(
            Found::at(
                "tsconfig",
                TSCONFIG,
                format!("{TSCONFIG} declares no {PATHS_KEY} in compilerOptions.paths"),
                help,
            )
            .named(PATHS_KEY),
        );
    }

    Ok(found)
}

/// Whether a `paths` value is the one target `./src/*`.
fn to_src(value: &Value) -> bool {
    value.as_array().is_some_and(|targets| {
        targets.len() == 1
            && targets[0]
                .as_str()
                .is_some_and(|target| matches!(target, "./src/*" | "src/*"))
    })
}

/// The Vite configuration, when there is one: an `@` alias to `src/`, or the plugin that reads the
/// tsconfig `paths`.
fn vite(project: &Project<'_, '_>) -> gate_sdk::Result<Vec<Found>> {
    let Some(path) = vite_config(project) else {
        return Ok(Vec::new());
    };
    let Some(file) = project.parse(path)? else {
        return Ok(Vec::new());
    };
    if aliased(&file) || reads_tsconfig(&file) {
        return Ok(Vec::new());
    }

    Ok(vec![
        Found::at(
            "vite",
            path,
            format!("{path} has no @ alias to src/, so the bundler does not resolve @/"),
            format!(
                "add resolve.alias {{ \"@\": <the path of src> }}, or the {TSCONFIG_PATHS_PLUGIN} plugin"
            ),
        )
        .named("@"),
    ])
}

/// Whether `file` declares an `alias` that maps `@` to `src/`, in the object form or the
/// `{ find, replacement }` form.
fn aliased(file: &SourceFile<'_>) -> bool {
    file.semantic().nodes().iter().any(|node| {
        let AstKind::ObjectProperty(alias) = node.kind() else {
            return false;
        };
        if alias.key.static_name().as_deref() != Some("alias") {
            return false;
        }
        match &alias.value {
            Expression::ObjectExpression(object) => property(object, "@").is_some_and(|entry| {
                strings_within(file, entry.value.span())
                    .iter()
                    .any(|text| names_src(text))
            }),
            Expression::ArrayExpression(array) => array.elements.iter().any(|element| {
                let Some(Expression::ObjectExpression(object)) = element.as_expression() else {
                    return false;
                };
                property_text(object, "find").as_deref() == Some("@")
                    && property(object, "replacement").is_some_and(|replacement| {
                        strings_within(file, replacement.value.span())
                            .iter()
                            .any(|text| names_src(text))
                    })
            }),
            _ => false,
        }
    })
}

/// Whether `file` calls the plugin that makes Vite read the tsconfig `paths`.
fn reads_tsconfig(file: &SourceFile<'_>) -> bool {
    let Some(bindings) = ImportBindings::new(file, TSCONFIG_PATHS_PLUGIN, false) else {
        return false;
    };
    file.semantic().nodes().iter().any(|node| {
        matches!(node.kind(), AstKind::CallExpression(call) if bindings.resolve(&call.callee).is_some())
    })
}

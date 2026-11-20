//! `frontend/route-config`: in a Vite app, the Vite configuration runs `TanStack` Router's plugin
//! with the route folder and the generated route tree of the standard.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::binding::ImportBindings;
use gate_typescript::call::TypeScriptCall;
use gate_typescript::oxc_ast::AstKind;

use crate::code::Project;
use crate::config::{VITE_CONFIGS, first_object, property_text, vite_config};
use crate::options::{PACK, Pack};
use crate::rules::{Found, report};
use crate::standard::VITE_ROUTE_TREE;

/// The rule.
#[derive(Debug, Default)]
pub struct RouteConfig;

/// The package of `TanStack` Router's Vite plugin.
const PLUGIN_PACKAGE: &str = "@tanstack/router-plugin/vite";

/// The names the package exports the plugin by.
const PLUGIN_EXPORTS: [&str; 2] = ["tanstackRouter", "TanStackRouterVite"];

/// The route folder of a Vite app.
const ROUTES_DIRECTORY: &str = "src/app/routes";

impl Rule<TypeScriptCall> for RouteConfig {
    type Options = NoOptions;

    const NAME: &'static str = "route-config";
    const TITLE: &'static str =
        "A Vite app runs TanStack Router's plugin on src/app/routes and src/app/route-tree.gen.ts";
    const ASPECTS: &'static [&'static str] = &["plugin", "routes-directory", "route-tree"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, NoOptions, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let found = if project.pack == Pack::ReactVite {
                check(&project)?
            } else {
                Vec::new()
            };
            report(run, found);
            Ok(())
        })
    }
}

fn check(project: &Project<'_, '_>) -> gate_sdk::Result<Vec<Found>> {
    let plugin_help = format!(
        "add tanstackRouter({{ target: \"react\", routesDirectory: \"./{ROUTES_DIRECTORY}\", generatedRouteTree: \"./{VITE_ROUTE_TREE}\" }}) of {PLUGIN_PACKAGE} to the plugins"
    );
    let Some(path) = vite_config(project) else {
        return Ok(vec![
            Found::at(
                "plugin",
                VITE_CONFIGS[2],
                format!(
                    "the project has no Vite configuration ({})",
                    VITE_CONFIGS.join(", ")
                ),
                plugin_help,
            )
            .named("plugin"),
        ]);
    };
    let Some(file) = project.parse(path)? else {
        return Ok(Vec::new());
    };

    let plugin = ImportBindings::new(&file, PLUGIN_PACKAGE, false).and_then(|bindings| {
        file.semantic()
            .nodes()
            .iter()
            .find_map(|node| match node.kind() {
                AstKind::CallExpression(call)
                    if bindings.resolve(&call.callee).is_some_and(|names| {
                        PLUGIN_EXPORTS.iter().any(|name| names == [*name])
                    }) =>
                {
                    Some(call)
                }
                _ => None,
            })
    });
    let Some(call) = plugin else {
        return Ok(vec![
            Found::at(
                "plugin",
                path,
                format!("{path} does not run TanStack Router's plugin of {PLUGIN_PACKAGE}"),
                plugin_help,
            )
            .named("plugin"),
        ]);
    };

    let options = first_object(call);
    let mut found = Vec::new();
    for (aspect, option, expected) in [
        ("routes-directory", "routesDirectory", ROUTES_DIRECTORY),
        ("route-tree", "generatedRouteTree", VITE_ROUTE_TREE),
    ] {
        let set = options.and_then(|object| property_text(object, option));
        if set.as_deref().map(|text| text.trim_start_matches("./")) == Some(expected) {
            continue;
        }
        let message = match &set {
            Some(text) => format!("{path} sets {option} to {text}, not ./{expected}"),
            None => {
                format!("{path} leaves {option} to the plugin's default, which is not ./{expected}")
            }
        };
        found.push(
            Found::at(
                aspect,
                path,
                message,
                format!("set {option}: \"./{expected}\" in the plugin's options"),
            )
            .named(option),
        );
    }

    Ok(found)
}

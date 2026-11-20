//! `frontend/route-files`: the route folder holds route files only, each with the export its router
//! requires: `TanStack` Router's `Route` in a Vite app's `src/app/routes/`, Next.js's default export
//! (or the handlers of a `route.ts`) in a Next.js app's root `app/`.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::{Project, exports};
use crate::options::{PACK, Pack};
use crate::rules::{Found, report};
use crate::standard::{FileName, NEXT_APP, name_of, within};

/// The rule.
#[derive(Debug, Default)]
pub struct RouteFiles;

/// The Next.js files of a route segment that export their content as the default export.
pub const NEXT_DEFAULT_FILES: [&str; 17] = [
    "apple-icon",
    "default",
    "error",
    "forbidden",
    "global-error",
    "icon",
    "layout",
    "loading",
    "manifest",
    "not-found",
    "opengraph-image",
    "page",
    "robots",
    "sitemap",
    "template",
    "twitter-image",
    "unauthorized",
];

/// The Next.js route handler file, which exports HTTP methods.
const NEXT_HANDLER_FILE: &str = "route";

/// The HTTP methods a Next.js route handler exports.
const HTTP_METHODS: [&str; 7] = ["GET", "HEAD", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"];

impl Rule<TypeScriptCall> for RouteFiles {
    type Options = NoOptions;

    const NAME: &'static str = "route-files";
    const TITLE: &'static str =
        "The route folder holds route files only, each with the export its router requires";
    const ASPECTS: &'static [&'static str] = &["foreign", "export"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, NoOptions, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let found = check(&project)?;
            report(run, found);
            Ok(())
        })
    }
}

/// Whether `path` is a Next.js route file by its name: one of the special files of a segment of the
/// root `app/`.
#[must_use]
pub fn next_route_file(path: &str) -> bool {
    let name = FileName::new(name_of(path));
    within(path, NEXT_APP)
        && matches!(name.extension, "ts" | "tsx")
        && name.dotted.is_empty()
        && (NEXT_DEFAULT_FILES.contains(&name.stem) || name.stem == NEXT_HANDLER_FILE)
}

fn check(project: &Project<'_, '_>) -> gate_sdk::Result<Vec<Found>> {
    match project.pack {
        Pack::ReactVite => vite(project),
        Pack::Nextjs => next(project),
        Pack::ReactLibrary => Ok(Vec::new()),
    }
}

/// A Vite app: every file of `src/app/routes/` is a `.ts` or `.tsx` route file (a `.lazy` one
/// too) that exports `Route`.
fn vite(project: &Project<'_, '_>) -> gate_sdk::Result<Vec<Found>> {
    let routes = "src/app/routes";
    let mut found = Vec::new();

    for path in project.sources().filter(|path| within(path, routes)) {
        let name = FileName::new(name_of(path));
        let code = matches!(name.extension, "ts" | "tsx");
        if !code || name.dotted.iter().any(|part| *part != "lazy") {
            found.push(Found::at(
                "foreign",
                path,
                format!("{path} is not a route file; {routes}/ holds route files only"),
                "move it into the layer and slice of its job",
            ));
            continue;
        }
        let Some(file) = project.parse(path)? else {
            continue;
        };
        let exported = exports(&file);
        if exported.get("Route").is_none() && !exported.star {
            found.push(
                Found::at(
                    "export",
                    path,
                    format!("{path} exports no Route, so it is not a route of TanStack Router"),
                    "export the route as Route (createFileRoute, or createRootRoute in __root.tsx), or move the file into a layer",
                )
                .named("Route"),
            );
        }
    }

    Ok(found)
}

/// A Next.js app: every code file of the root `app/` is a special file of a route segment with its
/// required export.
fn next(project: &Project<'_, '_>) -> gate_sdk::Result<Vec<Found>> {
    let mut found = Vec::new();

    for path in project.routes() {
        if !next_route_file(path) {
            found.push(Found::at(
                "foreign",
                path,
                format!("{path} is not a Next.js route file; {NEXT_APP}/ holds route files only"),
                "move components, hooks and helpers into the page, layer and slice of their job in src/",
            ));
            continue;
        }
        let Some(file) = project.parse(path)? else {
            continue;
        };
        let exported = exports(&file);
        if exported.star {
            continue;
        }
        let stem = FileName::new(name_of(path)).stem;
        if stem == NEXT_HANDLER_FILE {
            if !HTTP_METHODS
                .iter()
                .any(|method| exported.get(method).is_some())
            {
                found.push(
                    Found::at(
                        "export",
                        path,
                        format!("{path} exports no HTTP method handler"),
                        format!(
                            "export a handler named after its method: {}",
                            HTTP_METHODS.join(", ")
                        ),
                    )
                    .named("method"),
                );
            }
        } else if exported.get("default").is_none() {
            found.push(
                Found::at(
                    "export",
                    path,
                    format!("{path} has no default export, which Next.js requires of {stem}"),
                    "export the route's content as the default export",
                )
                .named("default"),
            );
        }
    }

    Ok(found)
}

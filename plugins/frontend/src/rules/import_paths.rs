//! `frontend/import-paths`: one alias, `@/` for `src/`; an import out of a slice names its target
//! from `@/` and through the slice's public API, and an import inside one is relative. The app and
//! shared layers are one slice each.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::{Project, imports};
use crate::options::{PACK, Pack};
use crate::rules::{Found, report};
use crate::standard::{
    CROSS_IMPORT, Layer, NEXT_APP, OPEN_SEGMENTS, Place, SOURCE_ROOT, Unit, VITE_ROUTE_TREE, within,
};

/// The rule.
#[derive(Debug, Default)]
pub struct ImportPaths;

/// The one alias, for `src/`.
const ALIAS: &str = "@/";

impl Rule<TypeScriptCall> for ImportPaths {
    type Options = NoOptions;

    const NAME: &'static str = "import-paths";
    const TITLE: &'static str =
        "Imports out of a slice go through @/ and its public API, imports inside it are relative";
    const ASPECTS: &'static [&'static str] =
        &["alias", "relative-exit", "absolute-inside", "public-api"];
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

fn check(project: &Project<'_, '_>) -> gate_sdk::Result<Vec<Found>> {
    let mut found = Vec::new();
    for path in project.all_code() {
        let origin = project.place(path).map_or(Unit::None, |place| place.unit);
        let Some(file) = project.parse(path)? else {
            continue;
        };

        for import in imports(&file) {
            let specifier = import.specifier.as_str();
            let Some(target) = project.resolve(path, specifier)? else {
                continue;
            };

            let finding = if specifier.starts_with('.') {
                relative(project, path, origin, specifier, &target)
            } else if specifier.starts_with(ALIAS) {
                aliased(project, path, origin, specifier, &target)
            } else {
                project.place(&target).map(|_| {
                    Found::at(
                        "alias",
                        path,
                        format!("{path} imports {specifier}, an alias other than {ALIAS}"),
                        format!(
                            "name the target as {ALIAS}<path below src/>, and remove the alias from tsconfig.json"
                        ),
                    )
                })
            };

            if let Some(finding) = finding {
                found.push(finding.line(import.line).named(specifier));
            }
        }
    }

    Ok(found)
}

/// The finding about the relative import `specifier` of `path`, of the unit `origin`, that resolves
/// to `target`: one that leaves the unit, or for a file outside the source root its folder.
fn relative(
    project: &Project<'_, '_>,
    path: &str,
    origin: Unit<'_>,
    specifier: &str,
    target: &str,
) -> Option<Found> {
    if let Some(folder) = home(path) {
        if home(target) == Some(folder) {
            return None;
        }
        let left = if folder.is_empty() {
            "the project root"
        } else {
            "app/"
        };
        return Some(Found::at(
            "relative-exit",
            path,
            format!("{path} imports {specifier}, which leaves {left} by a relative path"),
            format!("name a target in src/ as {ALIAS}<path below src/>"),
        ));
    }
    let imported = project.place(target)?;
    if origin == Unit::None
        || imported.unit == origin
        || (origin == Unit::Root && target == VITE_ROUTE_TREE)
    {
        return None;
    }
    Some(Found::at(
        "relative-exit",
        path,
        format!("{path} imports {specifier}, which leaves its slice by a relative path"),
        format!("name the target from the source root, as {ALIAS}<path below src/>"),
    ))
}

/// The finding about the `@/` import `specifier` of `path`, of the unit `origin`, that resolves to
/// `target`: one inside its own unit, or one past the target's public API.
fn aliased(
    project: &Project<'_, '_>,
    path: &str,
    origin: Unit<'_>,
    specifier: &str,
    target: &str,
) -> Option<Found> {
    let pack = project.pack;
    let rest = specifier.strip_prefix(ALIAS)?;
    let imported = project.place(target)?;
    if origin != Unit::None && imported.unit == origin {
        return Some(Found::at(
            "absolute-inside",
            path,
            format!("{path} imports {specifier}, a module of its own slice, through {ALIAS}"),
            "import inside a slice by a relative path",
        ));
    }
    if public(pack, origin, &imported, rest) {
        return None;
    }
    let help = match without_index(rest).filter(|bare| public(pack, origin, &imported, bare)) {
        Some(bare) => format!("write {ALIAS}{bare}; a public API is imported by its folder"),
        None => format!(
            "import {ALIAS}{} and export what is needed from its index.ts",
            api_of(&imported)
        ),
    };
    Some(Found::at(
        "public-api",
        path,
        format!(
            "{path} imports {specifier}, past the public API of {}",
            api_of(&imported)
        ),
        help,
    ))
}

/// The folder a file outside the source root keeps its relative imports in: the root `app/` of
/// Next.js for its files, the project root (`""`) for Next.js's root files; `None` for a file under
/// the source root.
fn home(path: &str) -> Option<&'static str> {
    if within(path, SOURCE_ROOT) {
        None
    } else if within(path, NEXT_APP) {
        Some(NEXT_APP)
    } else {
        Some("")
    }
}

/// `rest` without a trailing `/index`, `/index.ts` or `/index.tsx`: the folder it names.
fn without_index(rest: &str) -> Option<&str> {
    let rest = rest.trim_end_matches('/');
    ["/index.tsx", "/index.ts", "/index"]
        .iter()
        .find_map(|suffix| rest.strip_suffix(suffix))
}

/// The public API an import from outside should name: `<layer>/<slice>`, or `shared/<segment>`.
fn api_of(imported: &Place<'_>) -> String {
    match (imported.unit, imported.segment) {
        (Unit::Slice(layer, slice), _) => format!("{}/{slice}", layer.name()),
        (Unit::Shared, Some(segment)) => format!("shared/{segment}"),
        _ => "its slice".to_owned(),
    }
}

/// Whether `rest`, a specifier below `@/` that leaves the importing slice for `imported`, names a
/// public API: a slice (its `index.server` too in Next.js), an entity's `@x` file from another
/// entity, a shared segment (its `index.server` too in Next.js), or for `ui` and `lib` one level
/// below it. The app layer and a library have none to check. `/index` is not a public API's name:
/// the folder is.
fn public(pack: Pack, origin: Unit<'_>, imported: &Place<'_>, rest: &str) -> bool {
    let rest = rest.trim_end_matches('/');
    match imported.unit {
        Unit::Slice(layer, slice) => {
            let Some(inner) = rest
                .strip_prefix(layer.name())
                .and_then(|after| after.strip_prefix('/'))
                .and_then(|after| after.strip_prefix(slice))
            else {
                return false;
            };
            match inner {
                "" => true,
                "/index.server" => pack == Pack::Nextjs,
                _ => {
                    layer == Layer::Entities
                        && matches!(origin, Unit::Slice(Layer::Entities, _))
                        && inner
                            .strip_prefix('/')
                            .and_then(|after| after.strip_prefix(CROSS_IMPORT))
                            .and_then(|after| after.strip_prefix('/'))
                            .is_some_and(|file| !file.contains('/'))
                }
            }
        }
        Unit::Shared if !pack.library() => {
            let mut parts = rest.split('/');
            let (Some("shared"), Some(segment)) = (parts.next(), parts.next()) else {
                return false;
            };
            match (parts.next(), parts.next()) {
                (None, _) => true,
                (Some(part), None) => {
                    OPEN_SEGMENTS.contains(&segment)
                        || (pack == Pack::Nextjs && part == "index.server")
                }
                _ => false,
            }
        }
        _ => true,
    }
}

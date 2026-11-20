//! `frontend/import-paths`: one alias, `@/` for `src/`; an import out of a slice names its target
//! from `@/` and through the slice's public API, and an import inside one is relative. The app and
//! shared layers are one slice each.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::{Project, imports};
use crate::options::{PACK, Pack};
use crate::rules::{Found, report};
use crate::standard::{CROSS_IMPORT, Layer, OPEN_SEGMENTS, Place, Unit};

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
    let pack = project.pack;
    let mut found = Vec::new();
    for path in project.all_code() {
        let importer = project.place(path);
        let importer_unit = importer.map_or(Unit::None, |place| place.unit);
        let Some(file) = project.parse(path)? else {
            continue;
        };

        for import in imports(&file) {
            let specifier = import.specifier.as_str();
            let Some(target) = project.resolve(path, specifier)? else {
                continue;
            };
            let Some(imported) = project.place(&target) else {
                continue;
            };

            let finding = if specifier.starts_with('.') {
                if importer_unit == Unit::None || imported.unit == importer_unit {
                    continue;
                }
                Found::at(
                    "relative-exit",
                    path,
                    format!(
                        "{path} imports {specifier}, which leaves its slice by a relative path"
                    ),
                    format!("name the target from the source root, as {ALIAS}<path below src/>"),
                )
            } else if let Some(rest) = specifier.strip_prefix(ALIAS) {
                if importer_unit != Unit::None && imported.unit == importer_unit {
                    Found::at(
                        "absolute-inside",
                        path,
                        format!(
                            "{path} imports {specifier}, a module of its own slice, through {ALIAS}"
                        ),
                        "import inside a slice by a relative path",
                    )
                } else if public(pack, importer_unit, &imported, rest) {
                    continue;
                } else {
                    Found::at(
                        "public-api",
                        path,
                        format!(
                            "{path} imports {specifier}, past the public API of {}",
                            api_of(&imported)
                        ),
                        format!(
                            "import {ALIAS}{} and export what is needed from its index.ts",
                            api_of(&imported)
                        ),
                    )
                }
            } else {
                Found::at(
                    "alias",
                    path,
                    format!("{path} imports {specifier}, an alias other than {ALIAS}"),
                    format!(
                        "name the target as {ALIAS}<path below src/>, and remove the alias from tsconfig.json"
                    ),
                )
            };

            found.push(finding.line(import.line).named(specifier));
        }
    }

    Ok(found)
}

/// The public API an import from outside should name: `<layer>/<slice>`, or `shared/<segment>`.
fn api_of(imported: &Place<'_>) -> String {
    match (imported.unit, imported.segment) {
        (Unit::Slice(layer, slice), _) => format!("{}/{slice}", layer.name()),
        (Unit::Shared, Some(segment)) => format!("shared/{segment}"),
        _ => "its slice".to_owned(),
    }
}

/// Whether `rest`, a specifier below `@/` that leaves the importer's slice for `imported`, names a
/// public API: a slice (its `index.server` too in Next.js), an entity's `@x` file from another
/// entity, a shared segment, or for `ui` and `lib` one level below it. The app layer and a
/// library have none to check.
fn public(pack: Pack, importer: Unit<'_>, imported: &Place<'_>, rest: &str) -> bool {
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix("/index").unwrap_or(rest);
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
                        && matches!(importer, Unit::Slice(Layer::Entities, _))
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
                (Some(_), None) => OPEN_SEGMENTS.contains(&segment),
                _ => false,
            }
        }
        _ => true,
    }
}

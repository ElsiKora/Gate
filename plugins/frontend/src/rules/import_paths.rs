//! `frontend/import-paths`: one alias, `@/` for `src/`; an import out of a slice or segment names
//! its target from `@/`, and an import inside one is relative.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::{Project, imports};
use crate::options::PACK;
use crate::rules::{Found, report};
use crate::standard::{Unit, place};

/// The rule.
#[derive(Debug, Default)]
pub struct ImportPaths;

/// The one alias, for `src/`.
const ALIAS: &str = "@/";

impl Rule<TypeScriptCall> for ImportPaths {
    type Options = NoOptions;

    const NAME: &'static str = "import-paths";
    const TITLE: &'static str =
        "Imports out of a slice or segment go through @/, imports inside it are relative";
    const ASPECTS: &'static [&'static str] = &["alias", "relative-exit", "absolute-inside"];
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

fn check(project: &Project<'_>) -> gate_sdk::Result<Vec<Found>> {
    let pack = project.pack;
    let mut found = Vec::new();

    for path in project.code() {
        let Some(importer) = place(pack, path) else {
            continue;
        };
        let Some(file) = project.parse(path)? else {
            continue;
        };

        for import in imports(&file) {
            let specifier = import.specifier.as_str();
            let Some(target) = project.resolve(path, specifier)? else {
                continue;
            };
            let target_unit = place(pack, &target).map(|place| place.unit);

            let finding = if specifier.starts_with('.') {
                if importer.unit == Unit::None || target_unit == Some(importer.unit) {
                    continue;
                }
                Found::at(
                    "relative-exit",
                    path,
                    format!(
                        "{path} imports {specifier}, which leaves its slice or segment by a relative path"
                    ),
                    format!("name the target from the source root, as {ALIAS}<path below src/>"),
                )
            } else if specifier.starts_with(ALIAS) {
                if importer.unit == Unit::None || target_unit != Some(importer.unit) {
                    continue;
                }
                Found::at(
                    "absolute-inside",
                    path,
                    format!(
                        "{path} imports {specifier}, a module of its own slice or segment, through {ALIAS}"
                    ),
                    "import inside a slice or segment by a relative path",
                )
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

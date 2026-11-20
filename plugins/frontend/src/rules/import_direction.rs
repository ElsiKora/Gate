//! `frontend/import-direction`: layers import only layers below them, and a slice imports no other
//! slice of its own layer.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::{Project, imports};
use crate::options::PACK;
use crate::rules::{Found, report};
use crate::standard::{Unit, place};

/// The rule.
#[derive(Debug, Default)]
pub struct ImportDirection;

impl Rule<TypeScriptCall> for ImportDirection {
    type Options = NoOptions;

    const NAME: &'static str = "import-direction";
    const TITLE: &'static str =
        "Layers import only layers below them, and slices no slice of their own layer";
    const ASPECTS: &'static [&'static str] = &["layer", "cross-slice"];
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

    if pack.library() {
        return Ok(found);
    }

    for path in project.code() {
        let Some(importer) = place(pack, path) else {
            continue;
        };
        let Some(layer) = importer.layer else {
            continue;
        };
        let Some(file) = project.parse(path)? else {
            continue;
        };

        for import in imports(&file) {
            let Some(target) = project.resolve(path, &import.specifier)? else {
                continue;
            };
            let Some(imported) = place(pack, &target) else {
                continue;
            };
            let Some(imported_layer) = imported.layer else {
                continue;
            };

            let finding = if imported_layer > layer {
                Found::at(
                    "layer",
                    path,
                    format!(
                        "{path} imports {}, a module of the higher layer {}; {} imports only layers below it",
                        import.specifier,
                        imported_layer.name(),
                        layer.name()
                    ),
                    "move the shared part down to the lower layer, or compose the two in the higher one",
                )
            } else if imported_layer == layer
                && layer.sliced()
                && imported.unit != importer.unit
                && matches!(imported.unit, Unit::Slice(..))
            {
                Found::at(
                    "cross-slice",
                    path,
                    format!(
                        "{path} imports {}, another slice of {}",
                        import.specifier,
                        layer.name()
                    ),
                    "compose the two slices in a higher layer, or move what both need to a lower one",
                )
            } else {
                continue;
            };

            found.push(finding.line(import.line).named(&import.specifier));
        }
    }

    Ok(found)
}

//! `frontend/style-files`: the global stylesheet lives in the app layer's `styles` segment (a
//! library's `ui` segment) and is imported only from the app layer or Next.js's root `app/`, one of
//! them from the code; every other stylesheet is a CSS module beside the component of its name.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::{Project, imports, joined};
use crate::options::{PACK, Pack};
use crate::rules::{Found, report};
use crate::standard::{
    FileName, NEXT_APP, SOURCE_ROOT, STYLE_EXTENSIONS, Unit, folder_of, name_of, within,
};

/// The rule.
#[derive(Debug, Default)]
pub struct StyleFiles;

impl Rule<TypeScriptCall> for StyleFiles {
    type Options = NoOptions;

    const NAME: &'static str = "style-files";
    const TITLE: &'static str =
        "Global styles live in one place; every other stylesheet is a module beside its component";
    const ASPECTS: &'static [&'static str] = &["global", "orphan", "import"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, NoOptions, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let mut found = files(&project);
            found.extend(imported(&project)?);
            report(run, found);
            Ok(())
        })
    }
}

/// Whether `path` is a stylesheet; `Some(true)` for a CSS module.
fn stylesheet(path: &str) -> Option<bool> {
    let name = FileName::new(name_of(path));
    STYLE_EXTENSIONS
        .contains(&name.extension)
        .then(|| name.module())
}

/// The stylesheets by where they sit.
fn files(project: &Project<'_, '_>) -> Vec<Found> {
    let global_folder = project.pack.global_styles_folder();
    let mut found = Vec::new();

    let paths = project.paths.iter().copied().filter(|path| {
        within(path, SOURCE_ROOT) || (project.pack == Pack::Nextjs && within(path, NEXT_APP))
    });

    for path in paths {
        match stylesheet(path) {
            Some(true) => {
                let stem = FileName::new(name_of(path)).stem;
                let component = format!("{}/{stem}.tsx", folder_of(path));
                if !project.contains(&component) {
                    found.push(Found::at(
                        "orphan",
                        path,
                        format!("{path} has no {stem}.tsx beside it"),
                        "keep a CSS module beside the component of its name; delete it when the component is gone",
                    ));
                }
            }
            Some(false) if !within(path, global_folder) => {
                found.push(Found::at(
                    "global",
                    path,
                    format!("{path} is a global stylesheet outside {global_folder}/"),
                    format!("make it a CSS module beside its component (name.module.css), or move global styles to {global_folder}/"),
                ));
            }
            _ => {}
        }
    }

    found
}

/// The global stylesheets the code imports: only from the app layer, the source root or the root
/// `app/` of Next.js, and one.
fn imported(project: &Project<'_, '_>) -> gate_sdk::Result<Vec<Found>> {
    let mut found = Vec::new();
    let mut first: Option<String> = None;

    for path in project.all_code() {
        let Some(file) = project.parse(path)? else {
            continue;
        };
        let importer_allowed = project
            .place(path)
            .is_none_or(|place| matches!(place.unit, Unit::App | Unit::Root));

        for import in imports(&file) {
            let specifier = import.specifier.as_str();
            if stylesheet(specifier) != Some(false) {
                continue;
            }
            if !importer_allowed {
                found.push(
                    Found::at(
                        "import",
                        path,
                        format!("{path} imports the global stylesheet {specifier} outside the app layer"),
                        "import global styles once, from the app layer; a component imports its CSS module",
                    )
                    .line(import.line)
                    .named(specifier),
                );
                continue;
            }
            let Some(target) = joined(path, specifier).filter(|target| project.contains(target))
            else {
                continue;
            };
            match &first {
                None => first = Some(target),
                Some(global) if *global != target => found.push(
                    Found::at(
                        "global",
                        path,
                        format!(
                            "{path} imports {specifier}, a second global stylesheet beside {global}"
                        ),
                        format!(
                            "keep one global stylesheet, {global}, and import the others from it"
                        ),
                    )
                    .line(import.line)
                    .named(specifier),
                ),
                Some(_) => {}
            }
        }
    }

    Ok(found)
}

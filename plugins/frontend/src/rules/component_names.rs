//! `frontend/component-names`: a `.tsx` file of a `ui` segment exports, by name, the component its
//! file name gives, and every other component it exports starts with that name.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::{Export, Project, exports};
use crate::options::PACK;
use crate::rules::{Found, report};
use crate::standard::{FileName, name_of, pascal_of};

/// The rule.
#[derive(Debug, Default)]
pub struct ComponentNames;

impl Rule<TypeScriptCall> for ComponentNames {
    type Options = NoOptions;

    const NAME: &'static str = "component-names";
    const TITLE: &'static str = "A component file exports by name the component its name gives";
    const ASPECTS: &'static [&'static str] = &["name", "extra", "default-export"];
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

    for path in project.code() {
        let name = FileName::new(name_of(path));
        let in_ui = project
            .place(path)
            .is_some_and(|place| place.segment == Some("ui"));
        if name.extension != "tsx" || !in_ui || name.stem == "index" || !name.dotted.is_empty() {
            continue;
        }
        let Some(file) = project.parse(path)? else {
            continue;
        };
        let exported = exports(&file);
        let component = pascal_of(name.stem);

        if exported
            .get(&component)
            .is_none_or(|export| export.type_only)
            && !exported.star
        {
            found.push(
                Found::at(
                    "name",
                    path,
                    format!("{path} does not export the component {component}, which its name gives"),
                    format!("export the component as {component}, or rename the file after the component it holds"),
                )
                .named(&component),
            );
        }

        for export in &exported.names {
            if export.name == "default" {
                found.push(
                    Found::at(
                        "default-export",
                        path,
                        format!("{path} has a default export; a component is exported by its name"),
                        format!("export it as {component}"),
                    )
                    .line(export.line)
                    .named("default"),
                );
            } else if component_like(export) && !export.name.starts_with(&component) {
                found.push(
                    Found::at(
                        "extra",
                        path,
                        format!(
                            "{path} exports the component {}, whose name does not start with {component}",
                            export.name
                        ),
                        "move it to a file of its own name, or name it after the file's component",
                    )
                    .line(export.line)
                    .named(&export.name),
                );
            }
        }
    }

    Ok(found)
}

/// Whether `export` is a component: a function, a class or a `const` bound to a function, whose
/// name starts with a capital letter and is not all capitals (`MAX_SIZE` is a constant). Types,
/// enums and other values are not components.
fn component_like(export: &Export) -> bool {
    let name = export.name.as_str();
    !export.type_only
        && export.function
        && name.starts_with(|character: char| character.is_ascii_uppercase())
        && !name.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

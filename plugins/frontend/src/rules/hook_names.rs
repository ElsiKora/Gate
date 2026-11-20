//! `frontend/hook-names`: a hook lives in a file of its own name, `useBalance` in `use-balance.ts`,
//! in a `model` or `lib` segment or the app layer.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::{Project, exports};
use crate::options::PACK;
use crate::rules::{Found, report};
use crate::standard::{FileName, Unit, kebab_of, name_of, place};

/// The rule.
#[derive(Debug, Default)]
pub struct HookNames;

impl Rule<TypeScriptCall> for HookNames {
    type Options = NoOptions;

    const NAME: &'static str = "hook-names";
    const TITLE: &'static str =
        "A hook lives in a file of its own name, in a model or lib segment or the app layer";
    const ASPECTS: &'static [&'static str] = &["file", "export", "placement"];
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

/// Whether `name` is a hook's: `use` and a capital letter.
pub fn is_hook(name: &str) -> bool {
    name.strip_prefix("use")
        .is_some_and(|rest| rest.starts_with(|character: char| character.is_ascii_uppercase()))
}

fn check(project: &Project<'_>) -> gate_sdk::Result<Vec<Found>> {
    let mut found = Vec::new();

    for path in project.code() {
        let name = FileName::new(name_of(path));
        if !name.dotted.is_empty() || name.stem == "index" {
            continue;
        }
        let Some(file) = project.parse(path)? else {
            continue;
        };
        let exported = exports(&file);
        let hooks: Vec<_> = exported
            .names
            .iter()
            .filter(|export| !export.type_only && !export.reexport && is_hook(&export.name))
            .collect();
        let hook_file = name.stem.starts_with("use-");

        for hook in &hooks {
            let expected = kebab_of(&hook.name);
            if name.stem != expected {
                found.push(
                    Found::at(
                        "file",
                        path,
                        format!(
                            "{path} exports the hook {}, which lives in {expected}.{}",
                            hook.name, name.extension
                        ),
                        "move the hook into a file of its own name",
                    )
                    .line(hook.line)
                    .named(&hook.name),
                );
            }
        }

        if hook_file
            && !exported.star
            && !hooks.iter().any(|hook| kebab_of(&hook.name) == name.stem)
        {
            found.push(Found::at(
                "export",
                path,
                format!("{path} is named after a hook it does not export"),
                "export the hook its name gives, or rename the file after what it holds",
            ));
        }

        if (hook_file || !hooks.is_empty()) && !placed(project, path) {
            found.push(Found::at(
                "placement",
                path,
                format!("{path} holds a hook outside a model or lib segment and the app layer"),
                "move the hook into the model segment of its slice, or into a lib segment when it knows no business",
            ));
        }
    }

    Ok(found)
}

/// Whether a hook may live at `path`: a `model` or `lib` segment, or the app layer.
fn placed(project: &Project<'_>, path: &str) -> bool {
    place(project.pack, path).is_some_and(|place| {
        place.unit == Unit::App || matches!(place.segment, Some("model" | "lib"))
    })
}

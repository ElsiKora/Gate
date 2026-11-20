//! `frontend/typescript-sources`: no JavaScript source under `src/`.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::Project;
use crate::options::PACK;
use crate::rules::{Found, report};
use crate::standard::{JAVASCRIPT_EXTENSIONS, extension_of};

/// The rule.
#[derive(Debug, Default)]
pub struct TypeScriptSources;

impl Rule<TypeScriptCall> for TypeScriptSources {
    type Options = NoOptions;

    const NAME: &'static str = "typescript-sources";
    const TITLE: &'static str = "Sources under src/ are TypeScript";
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, NoOptions, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let found: Vec<Found> = project
                .sources()
                .filter(|path| JAVASCRIPT_EXTENSIONS.contains(&extension_of(path)))
                .map(|path| {
                    Found::at(
                        "",
                        path,
                        format!("{path} is JavaScript; sources under src/ are TypeScript"),
                        "rename it to .ts or .tsx and type it",
                    )
                })
                .collect();
            report(run, found);
            Ok(())
        })
    }
}

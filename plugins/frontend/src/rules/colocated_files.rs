//! `frontend/colocated-files`: a test or a story sits beside the file it tests, which has its name.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::Project;
use crate::options::PACK;
use crate::rules::{Found, report};
use crate::standard::{FileName, folder_of, name_of};

/// The rule.
#[derive(Debug, Default)]
pub struct ColocatedFiles;

impl Rule<TypeScriptCall> for ColocatedFiles {
    type Options = NoOptions;

    const NAME: &'static str = "colocated-files";
    const TITLE: &'static str = "Tests and stories sit beside the file they are about";
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, NoOptions, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let sources: Vec<&str> = project.sources().collect();
            let found: Vec<Found> = sources
                .iter()
                .filter(|path| {
                    let file = FileName::new(name_of(path));
                    file.test_or_story() && !has_subject(&sources, path, file.stem)
                })
                .map(|path| {
                    let stem = FileName::new(name_of(path)).stem;
                    Found::at(
                        "",
                        path,
                        format!("{path} has no {stem} file beside it"),
                        "keep a test or a story beside the file it is about, with its name; delete it when that file is gone",
                    )
                })
                .collect();
            report(run, found);
            Ok(())
        })
    }
}

/// Whether the folder of `path` holds a file named `stem` that is neither a test nor a story.
fn has_subject(sources: &[&str], path: &str, stem: &str) -> bool {
    let folder = folder_of(path);
    sources.iter().any(|other| {
        let file = FileName::new(name_of(other));
        folder_of(other) == folder && file.stem == stem && !file.test_or_story()
    })
}

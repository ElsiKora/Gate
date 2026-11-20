//! `cdk/tests`: each stack, and each function file of `lib/naming/` and `lib/config/`, has a unit
//! test at its mirror path below `test/`.

use std::collections::BTreeSet;

use gate_sdk::{NoOptions, Rule, Run};

use crate::options::PluginOptions;
use crate::rules::{Found, file_name, name_and_role, report};
use crate::sources::Cdk;
use crate::standard::{TESTED_FOLDERS, TESTED_ROLES};

/// The rule.
#[derive(Debug, Default)]
pub struct Tests;

impl Rule<Cdk> for Tests {
    type Options = NoOptions;

    const NAME: &'static str = "tests";
    const TITLE: &'static str =
        "Each stack and each naming and configuration function has a unit test at its mirror path";
    const ASPECTS: &'static [&'static str] = &["stack", "module"];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        PluginOptions::read(run)?;
        let paths: BTreeSet<String> = run.tree().paths().into_iter().collect();
        let mut found = Vec::new();

        for path in &paths {
            let Some((_, role)) = name_and_role(file_name(path)) else {
                continue;
            };
            let aspect = if path.starts_with("lib/stack/") && role == "stack" {
                "stack"
            } else if TESTED_FOLDERS.iter().any(|folder| path.starts_with(folder))
                && TESTED_ROLES.contains(&role)
            {
                "module"
            } else {
                continue;
            };
            let Some(test) = mirror(path) else {
                continue;
            };
            if !paths.contains(&test) {
                found.push((aspect, path.clone(), test));
            }
        }

        for (aspect, path, test) in found {
            report(
                run,
                Found {
                    aspect,
                    path: &path,
                    line: None,
                    key: test.clone(),
                    message: format!("{path} has no unit test at {test}"),
                    help: "add the unit test at the mirror path of the file",
                },
            );
        }

        Ok(())
    }
}

/// `lib/<rest>.ts` as `test/<rest>.test.ts`.
fn mirror(path: &str) -> Option<String> {
    let rest = path.strip_prefix("lib/")?.strip_suffix(".ts")?;
    Some(format!("test/{rest}.test.ts"))
}

//! `cdk/barrels`: each folder below `lib/` that holds a `.ts` file has an `index.ts` that holds
//! only export declarations and re-exports each file and child folder of its folder, and nothing
//! outside it.

use std::collections::{BTreeMap, BTreeSet};

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::ast::{Statement, StringLiteral};
use gate_typescript::syntax::statement_start;

use crate::options::PluginOptions;
use crate::rules::{Found, file_name, folder, report};
use crate::sources::{Cdk, code_files, parsed};

/// The rule.
#[derive(Debug, Default)]
pub struct Barrels;

impl Rule<Cdk> for Barrels {
    type Options = NoOptions;

    const NAME: &'static str = "barrels";
    const TITLE: &'static str = "Each folder below lib/ has an index.ts that only re-exports each of its files and child folders";
    const ASPECTS: &'static [&'static str] =
        &["missing", "reexport-only", "explicit-export", "outside"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        PluginOptions::read(run)?;
        run.with_call(|run, shared| {
            let typescript = shared.typescript()?;
            let files = code_files(typescript, "lib/");
            let mut modules: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
            let mut barrels: BTreeSet<&str> = BTreeSet::new();

            for path in &files {
                let name = file_name(path);
                if name == "index.ts" {
                    barrels.insert(folder(path));
                } else if let Some(stem) = name.strip_suffix(".ts") {
                    modules
                        .entry(folder(path))
                        .or_default()
                        .insert(format!("./{stem}"));
                }
            }

            let mut found: Vec<(String, Judged)> = Vec::new();

            for folder_path in modules.keys() {
                if !barrels.contains(folder_path) {
                    let barrel = format!("{folder_path}/index.ts");
                    found.push((
                        barrel,
                        (
                            "missing",
                            None,
                            "index.ts".to_owned(),
                            format!("{folder_path} holds TypeScript files but no index.ts"),
                            "add an index.ts that re-exports each file and child folder",
                        ),
                    ));
                }
            }

            for barrel_folder in &barrels {
                let barrel = format!("{barrel_folder}/index.ts");
                let Some(file) = parsed(typescript, &barrel)? else {
                    continue;
                };
                let mut wanted = modules.get(barrel_folder).cloned().unwrap_or_default();
                for child in barrels
                    .iter()
                    .filter(|child| folder(child) == *barrel_folder)
                {
                    wanted.insert(format!("./{}", file_name(child)));
                }
                for judged in judge(&file, &barrel, &wanted) {
                    found.push((barrel.clone(), judged));
                }
            }

            for (path, (aspect, line, key, message, help)) in found {
                report(
                    run,
                    Found {
                        aspect,
                        path: &path,
                        line,
                        key,
                        message,
                        help,
                    },
                );
            }
            Ok(())
        })
    }
}

/// aspect, line, key, message, help
type Judged = (&'static str, Option<u32>, String, String, &'static str);

fn judge(file: &SourceFile<'_>, barrel: &str, wanted: &BTreeSet<String>) -> Vec<Judged> {
    let mut found = Vec::new();
    let program = file.program();

    let owned = program
        .directives
        .first()
        .map(|directive| directive.span.start)
        .or_else(|| {
            program
                .body
                .iter()
                .find(|statement| {
                    !matches!(
                        statement,
                        Statement::ExportAllDeclaration(_)
                            | Statement::ExportFromDeclaration(_)
                            | Statement::ExportNamedDeclaration(_)
                    )
                })
                .map(statement_start)
        });
    if let Some(start) = owned {
        found.push((
            "reexport-only",
            Some(file.line(start)),
            "owned-code".to_owned(),
            format!("{barrel} holds code; a barrel holds only re-exports"),
            "move the code into a file of its own and re-export it",
        ));
    }

    let mut exported = BTreeSet::new();
    for statement in &program.body {
        let module: &StringLiteral<'_> = match statement {
            Statement::ExportAllDeclaration(export) => &export.source,
            Statement::ExportFromDeclaration(export) if !export.specifiers.is_empty() => {
                &export.source
            }
            _ => continue,
        };
        let specifier = module.value.as_str();
        if !specifier.starts_with("./") || specifier.split('/').any(|segment| segment == "..") {
            found.push((
                "outside",
                Some(file.line(module.span.start)),
                specifier.to_owned(),
                format!("{barrel} re-exports {specifier}, which is outside its folder"),
                "re-export only the files and child folders of the barrel's own folder",
            ));
            continue;
        }
        exported.insert(normal(specifier));
    }

    for specifier in wanted.difference(&exported) {
        found.push((
            "explicit-export",
            None,
            specifier.clone(),
            format!("{barrel} does not re-export {specifier}"),
            "re-export the file or child folder from the barrel",
        ));
    }

    found
}

/// `./name.ts`, `./name.js` and `./child/index` as `./name` and `./child`.
fn normal(specifier: &str) -> String {
    let specifier = specifier
        .strip_suffix(".ts")
        .or_else(|| specifier.strip_suffix(".js"))
        .unwrap_or(specifier);
    specifier
        .strip_suffix("/index")
        .unwrap_or(specifier)
        .to_owned()
}

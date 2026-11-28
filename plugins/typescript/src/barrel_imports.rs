//! `typescript/barrel-imports`: a module outside a folder with a barrel imports from that folder
//! only through its barrel, and only names the barrel exports.
//!
//! A folder has a barrel when it holds a tracked file named `barrel-name`. For each static import
//! and re-export of a TypeScript file, the rule resolves the specifier the way the plugin resolves
//! modules (relative paths and the tsconfig `paths`), and finds the outermost folder with a barrel
//! that holds the imported file but not the importer. An import of any other file of that folder
//! is a deep path (`#deep-path`); an import of the barrel itself may name only what the barrel
//! exports (`#not-exported`). Files inside an owner folder (`owners`) import the folders inside that
//! owner directly, and importers that `exempt` matches are not checked. Type-only imports follow the
//! same rule.

use std::collections::{BTreeSet, HashSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Location, Result, Rule, Run};
use gate_typescript::call::TypeScriptCall;
use gate_typescript::exports::ExportMap;
use gate_typescript::loads::{Form, Names, static_loads};
use gate_typescript::{SourceFile, TypeScript};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

/// The rule.
#[derive(Debug, Default)]
pub struct BarrelImports;

/// The options of `typescript/barrel-imports`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The file name of a folder's barrel: a name, not a path.
    #[schemars(length(min = 1), regex(pattern = r"^[^/\\]+$"))]
    pub barrel_name: String,
    /// Globs of the owner folders, relative to the project root: inside an owner, its files import
    /// its subfolders and each other directly.
    pub owners: Vec<String>,
    /// Globs of the importing files the rule does not check, relative to the project root: test
    /// roots, for example.
    pub exempt: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            barrel_name: "index.ts".to_owned(),
            owners: Vec::new(),
            exempt: Vec::new(),
        }
    }
}

impl Rule<TypeScriptCall> for BarrelImports {
    type Options = Options;

    const NAME: &'static str = "barrel-imports";
    const TITLE: &'static str = "A folder with a barrel is imported from outside only through its barrel, and only for names it exports";
    const ASPECTS: &'static [&'static str] = &["deep-path", "not-exported"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, TypeScriptCall>) -> Result<()> {
        run.with_call(|run, shared| check(run, shared.typescript()?))
    }
}

/// The check of [`BarrelImports`] over the call's TypeScript view.
fn check(run: &mut Run<'_, Options, TypeScriptCall>, typescript: &TypeScript<'_>) -> Result<()> {
    let options = run.options();
    let barrel_name = options.barrel_name.as_str();
    if barrel_name.is_empty()
        || barrel_name.contains(['/', '\\'])
        || barrel_name == "."
        || barrel_name == ".."
    {
        return Err(Error::config(format!(
            "barrel-name {barrel_name:?} is not a file name; write the name of a folder's \
             barrel file, for example \"index.ts\""
        )));
    }
    let owners = globs("owners", &options.owners)?;
    let exempt = globs("exempt", &options.exempt)?;

    let checker = Checker {
        typescript,
        files: typescript.files().iter().copied().collect(),
        barrel_name,
        owners: &owners,
        exports: ExportMap::new(typescript, |importer, specifier| {
            typescript.resolve(importer, specifier)
        }),
    };

    let mut found = Vec::new();
    for path in typescript.files() {
        if typescript.settings().skips(path) || exempt.is_match(path) {
            continue;
        }
        let Some(file) = checker.parse(path)? else {
            continue;
        };
        for load in loads(&file) {
            checker.check(path, &load, &mut found)?;
        }
    }

    // The checker holds the options; it ends before the run reports.
    drop(checker);

    for finding in found {
        let help = match finding.aspect {
            "deep-path" => format!(
                "import from the folder's barrel {} instead; a name the barrel does not \
                 export is internal to {}: export it from the barrel, or keep the import \
                 inside the folder",
                finding.barrel, finding.folder
            ),
            _ => format!(
                "export the name from the barrel {}, or keep the import inside {}",
                finding.barrel, finding.folder
            ),
        };
        run.report()
            .finding(
                finding.aspect,
                Location::line(&finding.path, finding.line),
                finding.message,
            )
            .target(finding.path.clone())
            .key(finding.key)
            .help(help);
    }
    Ok(())
}

/// Globs whose `*` does not cross `/`, from option `key`.
fn globs(key: &str, patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(
            GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map_err(|error| {
                    Error::config(format!(
                        "{key} glob {pattern:?} is not valid: {error}; write a glob relative to \
                         the project root, such as \"src/modules/*\""
                    ))
                })?,
        );
    }
    builder
        .build()
        .map_err(|error| Error::config(format!("{key} globs do not build: {error}")))
}

/// One module a file loads statically, and the names it takes from it.
struct Load {
    specifier: String,
    /// The names imported, `default` for a default import; `None` when the whole module is taken
    /// (`import * as`, `export *`) or nothing named is (`import "x"`).
    names: Option<Vec<String>>,
    /// The one-based line of the declaration.
    line: usize,
}

/// The static imports and re-exports of a file: the top-level declarations with a source and
/// `import x = require("x")`, which takes the whole module, as `import * as x` does.
fn loads(file: &SourceFile<'_>) -> Vec<Load> {
    static_loads(file)
        .into_iter()
        .map(|load| Load {
            specifier: load.specifier,
            names: match (load.form, load.names) {
                (Form::ExportFrom, Names::Listed(names)) => Some(names),
                (_, Names::Listed(names)) if !names.is_empty() => Some(names),
                _ => None,
            },
            line: load.line,
        })
        .collect()
}

/// One finding before it is reported.
struct Found {
    aspect: &'static str,
    path: String,
    line: usize,
    message: String,
    /// What the finding is about, stable when the message is reworded or a path moves.
    key: String,
    barrel: String,
    folder: String,
}

struct Checker<'c, 'a> {
    typescript: &'c TypeScript<'a>,
    files: HashSet<&'a str>,
    barrel_name: &'c str,
    owners: &'c GlobSet,
    /// What each module read exports, followed through `export *`.
    exports: ExportMap<'c, 'a>,
}

impl<'a> Checker<'_, 'a> {
    /// The syntax tree of a tracked file; `None` when it is not tracked or does not parse, which
    /// `typescript/parse` reports.
    fn parse(&self, path: &str) -> Result<Option<std::rc::Rc<SourceFile<'a>>>> {
        let file = self.typescript.parse(path)?;
        Ok(file.filter(|file| !file.fatal()))
    }

    fn check(&self, path: &str, load: &Load, found: &mut Vec<Found>) -> Result<()> {
        let Some(target) = self.typescript.resolve(path, &load.specifier)? else {
            return Ok(());
        };
        let Some(folder) = self.barrel_folder(path, &target) else {
            return Ok(());
        };
        let barrel = join(&folder, self.barrel_name);
        let specifier = &load.specifier;

        if target != barrel {
            found.push(Found {
                aspect: "deep-path",
                path: path.to_owned(),
                line: load.line,
                message: format!(
                    "{path} imports {specifier} ({target}) past the barrel {barrel} of {folder}"
                ),
                key: specifier.clone(),
                barrel,
                folder,
            });
            return Ok(());
        }

        let Some(names) = &load.names else {
            return Ok(());
        };
        let (exported, open) = self.exports.names(&barrel)?;
        if open {
            return Ok(());
        }
        let mut reported = BTreeSet::new();
        for name in names {
            if !exported.contains(name) && reported.insert(name) {
                found.push(Found {
                    aspect: "not-exported",
                    path: path.to_owned(),
                    line: load.line,
                    message: format!(
                        "{path} imports {name} from the barrel {barrel}, which does not export it"
                    ),
                    key: format!("{name} from {specifier}"),
                    barrel: barrel.clone(),
                    folder: folder.clone(),
                });
            }
        }
        Ok(())
    }

    /// The outermost folder that holds `target` and a barrel but not `importer`, skipping the
    /// folders inside an owner folder that holds the importer.
    fn barrel_folder(&self, importer: &str, target: &str) -> Option<String> {
        let importer_owners: Vec<&str> = ancestors(parent(importer))
            .filter(|folder| self.owners.is_match(folder))
            .collect();
        ancestors(parent(target))
            .find(|folder| {
                !inside(folder, importer)
                    && self.files.contains(join(folder, self.barrel_name).as_str())
                    && !importer_owners.iter().any(|owner| inside(owner, folder))
            })
            .map(str::to_owned)
    }
}

/// The folder of a path; empty at the project root.
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// The folders from the outermost to `folder` itself, without the project root: `a`, `a/b`,
/// `a/b/c` for `a/b/c`.
fn ancestors(folder: &str) -> impl Iterator<Item = &str> {
    folder
        .match_indices('/')
        .map(|(index, _)| &folder[..index])
        .chain((!folder.is_empty()).then_some(folder))
}

/// Whether `path` lies inside `folder`.
fn inside(folder: &str, path: &str) -> bool {
    path.strip_prefix(folder)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// `name` in `folder`.
fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

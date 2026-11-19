//! `nestjs/import-aliases`: a source file loads another source owner through that owner's alias,
//! and an entity loads enums only through the enum barrels.
//!
//! Every tracked `.ts` file below `layout.source-root`, declaration files included and
//! `[languages.typescript] skip-folders` left out, is read for every module it loads (imports, re-exports,
//! `import x = require(...)`, `import(...)` types, dynamic `import(...)` and single-string
//! `require(...)`):
//!
//! - `owner-alias`: the specifier is the source root or a path below it, a relative path with more
//!   than one `..` segment, or a relative path that leaves the file's own owner (the folder below
//!   the source root) when that owner is one of `layout.source-owners`;
//! - `entity-enum-barrel`: a file directly in an `entity-folder` loads an enum file
//!   (`*.<enum-kind>`, any script extension) through a relative, source-root or alias path, or
//!   loads its own resource's `enum-folder` through a non-relative path instead of `../<enum-folder>`.
//!
//! Aliases resolve through `layout.path-aliases`. A file without a syntax tree is skipped:
//! `typescript/parse` reports it.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};
use gate_typescript::{Allocator, TypeScript};

use crate::layout::{Layout, PluginOptions};
use crate::shared::modules::{loads, normalize, relative_target};
use crate::shared::project;

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/import-aliases`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The folder whose files are entities.
    pub entity_folder: String,
    /// The folder of a resource that holds its enums behind a barrel.
    pub enum_folder: String,
    /// The kind of an enum file: the extension before the script extension (`state.enum.ts`).
    pub enum_kind: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            entity_folder: "entity".to_owned(),
            enum_folder: "enum".to_owned(),
            enum_kind: "enum".to_owned(),
        }
    }
}

impl gate_sdk::Rule for Rule {
    type Options = Options;

    const NAME: &'static str = "import-aliases";
    const TITLE: &'static str = "Source files load other owners through their aliases, and entities load no enum file or own enum alias";
    const ASPECTS: &'static [&'static str] = &["entity-enum-barrel", "owner-alias"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        {
            use crate::shared::paths::Form::Name;
            let o = run.options();
            crate::shared::paths::check("entity-folder", &o.entity_folder, Name)?;
            crate::shared::paths::check("enum-folder", &o.enum_folder, Name)?;
        }

        let layout = PluginOptions::read(run)?.layout;
        let options = run.options();
        let paths = Paths {
            layout: &layout,
            options,
        };
        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);
        let mut findings = Vec::new();

        for (path, loads) in loads(&typescript, &layout.source_root, true)? {
            let owner = paths.owner(path);
            let entity = path.rsplit('/').nth(1) == Some(options.entity_folder.as_str());

            for specifier in &loads.all {
                if entity && paths.names_enum_file(path, specifier) {
                    findings.push((
                        "entity-enum-barrel",
                        path,
                        format!(
                            "{path} loads the enum file {specifier}; import the enum through its owner's enum barrel"
                        ),
                        "import the enum through its owner's enum barrel".to_owned(),
                    ));
                }

                if entity && paths.names_own_enum_barrel(path, specifier) {
                    let barrel = format!("../{}", options.enum_folder);
                    findings.push((
                        "entity-enum-barrel",
                        path,
                        format!(
                            "{path} loads its own resource's enum barrel through {specifier}; import it as {barrel}"
                        ),
                        format!("import the barrel as {barrel}"),
                    ));
                }

                if paths.leaves_owner(path, owner, specifier) {
                    findings.push((
                        "owner-alias",
                        path,
                        format!("{path} loads {specifier}; use a canonical source-owner alias"),
                        "load the other owner through its alias".to_owned(),
                    ));
                }
            }
        }

        for (aspect, path, message, help) in findings {
            run.report()
                .finding(aspect, Location::file(path), message)
                .help(help);
        }

        Ok(())
    }
}

/// The paths a specifier names, read with the layout and the options.
struct Paths<'r> {
    layout: &'r Layout,
    options: &'r Options,
}

impl Paths<'_> {
    /// The owner of a source file: its first folder below the source root.
    fn owner<'p>(&self, path: &'p str) -> &'p str {
        let below = path
            .strip_prefix(self.layout.source_prefix().as_str())
            .unwrap_or_default();

        below.split('/').next().unwrap_or_default()
    }

    /// Whether `specifier` is the source root or a path below it.
    fn below_source_root(&self, specifier: &str) -> bool {
        let root = self.layout.source_root.trim_end_matches('/');

        if root.is_empty() {
            return false;
        }

        specifier == root
            || specifier
                .strip_prefix(root)
                .is_some_and(|rest| rest.starts_with('/'))
    }

    /// Whether `path`, owned by `owner`, loads `specifier` past its owner's alias: a source-root
    /// path, more than one `..` segment, or a relative path out of a known source owner.
    fn leaves_owner(&self, path: &str, owner: &str, specifier: &str) -> bool {
        if self.below_source_root(specifier) {
            return true;
        }

        if !(specifier.starts_with("./") || specifier.starts_with("../")) {
            return false;
        }

        let target = relative_target(path, specifier);
        let traversals = specifier.split('/').filter(|part| *part == "..").count();
        let folder = format!("{}{owner}", self.layout.source_prefix());
        let crosses = self.layout.is_source_owner(owner)
            && target != folder
            && !target.starts_with(&format!("{folder}/"));

        traversals > 1 || crosses
    }

    /// The project path a source specifier of `path` names: a relative path, a source-root path or
    /// an alias; `None` for a package.
    fn source_target(&self, path: &str, specifier: &str) -> Option<String> {
        if specifier.starts_with('.') {
            Some(relative_target(path, specifier))
        } else if self.below_source_root(specifier) {
            Some(normalize(specifier))
        } else {
            self.layout
                .resolve_alias(specifier)
                .map(|target| normalize(&target))
        }
    }

    /// Whether a specifier of `path` names an enum file by its path: a source target whose file
    /// name, without a script extension, has the enum kind as its extension.
    fn names_enum_file(&self, path: &str, specifier: &str) -> bool {
        self.source_target(path, specifier).is_some_and(|target| {
            let name = stem(&target).rsplit('/').next().unwrap_or_default();
            std::path::Path::new(name)
                .extension()
                .is_some_and(|extension| extension == self.options.enum_kind.as_str())
        })
    }

    /// Whether a non-relative specifier of the entity file `path` names the enum barrel of the
    /// entity's own resource (the folder above the entity folder), which it loads as
    /// `../<enum-folder>`.
    fn names_own_enum_barrel(&self, path: &str, specifier: &str) -> bool {
        let Some(resource) = path
            .rsplit_once('/')
            .and_then(|(folder, _)| folder.rsplit_once('/'))
            .map(|(resource, _)| resource)
        else {
            return false;
        };

        !specifier.starts_with('.')
            && self.source_target(path, specifier).is_some_and(|target| {
                names_file(
                    &format!("{resource}/{}", self.options.enum_folder),
                    target.trim_end_matches('/'),
                )
            })
    }
}

/// A source path without its script extension: `js`, `ts`, `jsx` or `tsx`, with an optional `c` or
/// `m` before it.
fn stem(path: &str) -> &str {
    let Some((before, extension)) = path.rsplit_once('.') else {
        return path;
    };
    let bytes = extension.as_bytes();
    let core = bytes
        .strip_prefix(b"c")
        .or_else(|| bytes.strip_prefix(b"m"))
        .unwrap_or(bytes);
    let script = matches!(core, [b'j' | b't', b's'] | [b'j' | b't', b's', b'x']);

    if script { before } else { path }
}

/// Whether a source target names the source file `file`, directly or as its folder's index.
fn names_file(target: &str, file: &str) -> bool {
    let file = stem(file);
    target == file || format!("{target}/index") == file
}

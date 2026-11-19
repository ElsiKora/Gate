//! `nestjs/barrel-exports`: every source folder with exportable files exposes a barrel that
//! re-exports each of them, barrels hold only re-exports, a wildcard re-export names a child
//! folder, and the private runtime entrypoints stay out of every barrel.

use std::collections::BTreeSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::oxc_ast::ast::{Statement, StringLiteral};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::syntax::statement_start;
use gate_typescript::{Allocator, Source, SourceFile, TypeScript};

use crate::layout::{Layout, PluginOptions, folder_prefix};
use crate::shared::paths::{self, Form};
use crate::shared::report::Findings;
use crate::shared::{project, tree};

/// The rule.
#[derive(Debug, Default)]
pub struct BarrelExports;

/// The options of `nestjs/barrel-exports`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The source files that are private runtime entrypoints: no barrel exports them and they are
    /// no exportable entry of their folder. With none, every entrypoint is reported.
    pub private_entrypoints: Vec<String>,
    /// The barrel's file name.
    pub barrel_name: String,
    /// Child folders a source barrel may leave out.
    pub optional_child_folders: Vec<String>,
    /// The folder whose files are never exportable entries of an enclosing folder.
    pub hidden_folder: String,
    /// The ending of an entrypoint's path.
    pub entrypoint_suffix: String,
    /// The extensions a relative re-export may name its target with.
    pub script_extensions: Vec<String>,
}

impl Options {
    /// Checks that every path is a normalized path relative to the project root and every name a
    /// plain name.
    fn check_paths(&self) -> gate_sdk::Result<()> {
        paths::check_all("private-entrypoints", &self.private_entrypoints, Form::File)?;
        paths::check("barrel-name", &self.barrel_name, Form::Name)?;
        paths::check_all(
            "optional-child-folders",
            &self.optional_child_folders,
            Form::Name,
        )?;
        paths::check("hidden-folder", &self.hidden_folder, Form::Name)
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            private_entrypoints: Vec::new(),
            barrel_name: "index.ts".to_owned(),
            optional_child_folders: ["function", "internal", "type", "utility"]
                .map(str::to_owned)
                .to_vec(),
            hidden_folder: "internal".to_owned(),
            entrypoint_suffix: "/main.ts".to_owned(),
            script_extensions: [".cjs", ".cts", ".js", ".mjs", ".mts", ".ts"]
                .map(str::to_owned)
                .to_vec(),
        }
    }
}

impl Rule for BarrelExports {
    type Options = Options;

    const NAME: &'static str = "barrel-exports";
    const TITLE: &'static str =
        "Source folders expose re-export-only barrels without private entrypoints";
    const ASPECTS: &'static [&'static str] = &[
        "explicit-export",
        "missing",
        "private-entrypoint",
        "reexport-only",
        "runtime-entrypoint",
        "wildcard",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let layout = PluginOptions::read(run)?.layout;
        let options = run.options();
        options.check_paths()?;
        let private: BTreeSet<String> = options.private_entrypoints.iter().cloned().collect();
        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);
        let barrels = Barrels {
            options,
            layout: &layout,
            private: &private,
            source: &source,
            typescript: &typescript,
        };
        let mut findings = Findings::default();

        for path in tree::typescript_files(&source, &layout.source_root, typescript.settings()) {
            if path.ends_with(options.entrypoint_suffix.as_str()) && !private.contains(path) {
                findings.push_keyed(
                    "runtime-entrypoint",
                    path,
                    None,
                    "entrypoint",
                    format!("{path} is not an approved private runtime entrypoint"),
                );
            }
        }

        barrels.folder_barrels(&mut findings)?;
        barrels.kind_folder_barrels(&mut findings)?;
        barrels.contents(&mut findings)?;
        findings.report(run, help);
        Ok(())
    }
}

/// What to do about a finding of `aspect`.
fn help(aspect: &str) -> &'static str {
    match aspect {
        "runtime-entrypoint" => {
            "list the file in private-entrypoints if it starts a runtime, or rename it"
        }
        "missing" | "explicit-export" => "re-export each exportable entry from the folder's barrel",
        "reexport-only" => "move the code to its own file and re-export it from the barrel",
        "private-entrypoint" => "remove the re-export of the private entrypoint",
        _ => "re-export named files, or a child folder with a wildcard",
    }
}

/// The run's view of the tree and its settings.
struct Barrels<'r, 'a> {
    options: &'r Options,
    layout: &'r Layout,
    private: &'r BTreeSet<String>,
    source: &'r dyn Source,
    typescript: &'r TypeScript<'a>,
}

impl<'r> Barrels<'r, '_> {
    /// The path of the barrel of `folder`.
    fn barrel(&self, folder: &str) -> String {
        format!("{folder}/{}", self.options.barrel_name)
    }

    /// Whether a file name is a barrel.
    fn is_barrel(&self, path: &str) -> bool {
        path.ends_with(&format!("/{}", self.options.barrel_name))
    }

    /// Whether one of the tracked files below `folder` is an exportable entry of it: a `.ts` file
    /// other than a barrel, outside every hidden folder and not private.
    fn exportable(&self, folder: &str) -> bool {
        tree::files(self.source, folder, self.typescript.settings()).any(|path| {
            let relative = &path[folder.len() + 1..];
            let (folders, name) = relative.rsplit_once('/').unwrap_or(("", relative));

            !folders
                .split('/')
                .any(|segment| segment == self.options.hidden_folder)
                && name != self.options.barrel_name
                && tree::is_typescript(name)
                && !self.private.contains(path)
        })
    }

    /// Every source folder with an exportable entry has a barrel that re-exports each entry.
    fn folder_barrels(&self, findings: &mut Findings) -> gate_sdk::Result<()> {
        let barrel_name = &self.options.barrel_name;

        for folder in tree::folders(
            self.source,
            &self.layout.source_root,
            self.typescript.settings(),
        ) {
            if !self.exportable(&folder) {
                continue;
            }

            let barrel = self.barrel(&folder);

            if !self.source.contains(&barrel) {
                findings.push_keyed(
                    "missing",
                    &barrel,
                    None,
                    "folder",
                    format!("{folder} must expose an {barrel_name} barrel"),
                );
                continue;
            }

            let Some(file) = project::parsed(self.typescript, &barrel)? else {
                continue;
            };
            let exported = self.exported_specifiers(&file, false);

            for (entry, is_folder) in
                tree::entries(self.source, &folder, self.typescript.settings())
            {
                let path = format!("{folder}/{entry}");

                if entry == *barrel_name
                    || self.private.contains(&path)
                    || (is_folder
                        && (self.options.optional_child_folders.contains(&entry)
                            || !self.exportable(&path)))
                    || (!is_folder && !tree::is_typescript(&entry))
                {
                    continue;
                }

                let specifier = if is_folder {
                    format!("./{entry}")
                } else {
                    format!("./{}", &entry[..entry.len() - ".ts".len()])
                };

                if !exported.contains(&specifier) {
                    findings.push_keyed(
                        "explicit-export",
                        &barrel,
                        None,
                        specifier.as_str(),
                        format!("{barrel} must export {specifier}"),
                    );
                }
            }
        }

        Ok(())
    }

    /// Every kind folder has a barrel that re-exports each of its files with a double-quoted
    /// specifier.
    fn kind_folder_barrels(&self, findings: &mut Findings) -> gate_sdk::Result<()> {
        let barrel_name = &self.options.barrel_name;
        let kind_folders: BTreeSet<&str> = tree::typescript_files(
            self.source,
            &self.layout.source_root,
            self.typescript.settings(),
        )
        .filter(|path| !self.is_barrel(path))
        .filter_map(|path| path.rsplit_once('/').map(|(folder, _)| folder))
        .filter(|folder| {
            let name = folder.rsplit_once('/').map_or(*folder, |(_, name)| name);
            self.layout.kind_folders.iter().any(|kind| kind == name)
        })
        .collect();

        for folder in kind_folders {
            let barrel = self.barrel(folder);

            if !self.source.contains(&barrel) {
                findings.push_keyed(
                    "missing",
                    &barrel,
                    None,
                    "kind-folder",
                    format!("{folder} is a kind folder and must expose an {barrel_name} barrel"),
                );
                continue;
            }

            let Some(file) = project::parsed(self.typescript, &barrel)? else {
                continue;
            };
            let exported = self.exported_specifiers(&file, true);

            for (entry, is_folder) in tree::entries(self.source, folder, self.typescript.settings())
            {
                if is_folder || entry == *barrel_name || !tree::is_typescript(&entry) {
                    continue;
                }

                let specifier = format!("./{}", &entry[..entry.len() - ".ts".len()]);

                if !exported.contains(&specifier) {
                    findings.push_keyed(
                        "explicit-export",
                        &barrel,
                        None,
                        specifier.as_str(),
                        format!("{barrel} must export {specifier}"),
                    );
                }
            }
        }

        Ok(())
    }

    /// The source, test-support and infrastructure-as-code barrels, each once.
    fn barrels(&self) -> Vec<&'r str> {
        let layout = self.layout;
        let settings = self.typescript.settings();

        tree::distinct(
            tree::typescript_files(self.source, &layout.source_root, settings)
                .chain(
                    tree::typescript_files(self.source, &layout.test_root, settings)
                        .filter(|path| tree::is_test_support(layout, path)),
                )
                .chain(tree::typescript_files(
                    self.source,
                    &layout.cdk_root,
                    settings,
                ))
                .filter(|path| self.is_barrel(path)),
        )
    }

    /// Every source, test-support and infrastructure-as-code barrel holds only re-exports, exposes
    /// no private entrypoint and re-exports with a wildcard only a child folder.
    fn contents(&self, findings: &mut Findings) -> gate_sdk::Result<()> {
        let layout = self.layout;
        let test_prefix = folder_prefix(&layout.test_root);

        for barrel in self.barrels() {
            let Some(file) = project::parsed(self.typescript, barrel)? else {
                continue;
            };
            let program = file.program();
            let owned = program
                .directives
                .first()
                .map(|directive| directive.span.start)
                .or_else(|| {
                    program
                        .body
                        .iter()
                        .find(|statement| !is_export_declaration(statement))
                        .map(statement_start)
                });

            if let Some(start) = owned {
                findings.push_keyed(
                    "reexport-only",
                    barrel,
                    Some(file.line(start)),
                    "owned-code",
                    format!(
                        "{barrel} must contain only re-export declarations; code owners require \
                         their own files"
                    ),
                );
            }

            let folder = barrel.rsplit_once('/').map_or("", |(folder, _)| folder);

            for statement in &program.body {
                let (module, clause) = match statement {
                    Statement::ExportAllDeclaration(export) => {
                        (&export.source, export.exported.is_some())
                    }
                    Statement::ExportFromDeclaration(export) if !export.specifiers.is_empty() => {
                        (&export.source, true)
                    }
                    _ => continue,
                };
                let specifier = module.value.as_str();
                let line = Some(file.line(statement.span().start));
                let resolved = self.typescript.resolve(barrel, specifier)?;

                if let Some(target) = &resolved
                    && !barrel.starts_with(&test_prefix)
                    && self.private.contains(target)
                {
                    findings.push_keyed(
                        "private-entrypoint",
                        barrel,
                        line,
                        specifier,
                        format!(
                            "{barrel} must not expose the private {} entrypoint",
                            private_specifier(folder, target)
                        ),
                    );
                }

                if clause {
                    continue;
                }

                let child = specifier
                    .strip_prefix("./")
                    .and_then(|rest| child_folder(folder, rest))
                    .filter(|child| {
                        tree::files(self.source, child, self.typescript.settings())
                            .next()
                            .is_some()
                    });
                let inside = child.as_ref().is_some_and(|child| {
                    resolved
                        .as_ref()
                        .is_some_and(|target| target.starts_with(&format!("{child}/")))
                });

                if !inside {
                    findings.push_keyed(
                        "wildcard",
                        barrel,
                        line,
                        specifier,
                        format!(
                            "{barrel} re-exports {specifier} with a wildcard; use named file \
                             exports or a child-folder wildcard"
                        ),
                    );
                }
            }
        }

        Ok(())
    }

    /// The module specifiers a barrel re-exports from, a relative one without its script
    /// extension; a re-export of no names counts as none and `double_quoted` counts only
    /// double-quoted ones.
    fn exported_specifiers(&self, file: &SourceFile<'_>, double_quoted: bool) -> BTreeSet<String> {
        let mut specifiers = BTreeSet::new();

        for statement in &file.program().body {
            let module: &StringLiteral<'_> = match statement {
                Statement::ExportAllDeclaration(export) => &export.source,
                Statement::ExportFromDeclaration(export) if !export.specifiers.is_empty() => {
                    &export.source
                }
                _ => continue,
            };

            if double_quoted && !file.source()[module.span.start as usize..].starts_with('"') {
                continue;
            }

            let value = module.value.as_str();
            let value = if value.starts_with('.') {
                self.options
                    .script_extensions
                    .iter()
                    .find_map(|extension| value.strip_suffix(extension.as_str()))
                    .unwrap_or(value)
            } else {
                value
            };

            specifiers.insert(value.to_owned());
        }

        specifiers
    }
}

/// Whether a top-level statement is an export declaration of the compiler: `export { … }`,
/// `export … from` or `export *`, not an exported declaration.
fn is_export_declaration(statement: &Statement<'_>) -> bool {
    matches!(
        statement,
        Statement::ExportAllDeclaration(_)
            | Statement::ExportFromDeclaration(_)
            | Statement::ExportNamedDeclaration(_)
    )
}

/// The folder `./rest` names from `folder` when it is a strict descendant of `folder`.
fn child_folder(folder: &str, rest: &str) -> Option<String> {
    let mut segments: Vec<&str> = folder.split('/').collect();

    for segment in rest.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            name => segments.push(name),
        }
    }

    let target = segments.join("/");

    target
        .strip_prefix(folder)
        .is_some_and(|tail| tail.starts_with('/'))
        .then_some(target)
}

/// The specifier of `target` relative to `folder`, without `.ts`, as the message names it.
fn private_specifier(folder: &str, target: &str) -> String {
    let from: Vec<&str> = folder
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let to: Vec<&str> = target.split('/').collect();
    let shared = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts: Vec<&str> = vec![".."; from.len() - shared];
    parts.extend(&to[shared..]);

    let relative = parts.join("/");
    let relative = relative.strip_suffix(".ts").unwrap_or(&relative);

    if relative.starts_with('.') {
        relative.to_owned()
    } else {
        format!("./{relative}")
    }
}

//! `nestjs/kind-imports`: every source and test file loads each declaration from the module the
//! layout gives it, the barrel of the kind folder that declares it, or its file.
//!
//! A kind folder is a folder named by one of `kinds` that is not a structural position (the source
//! root, a top-level owner, a module plane) and whose files other than barrels, at any depth, all
//! carry its kind (`<name>.<kind>.ts` or `<kind>.ts`, declaration files included), below no other
//! kind folder. For each load of a tracked `.ts` file below the source root or the test root, the
//! rule resolves the specifier through `layout.path-aliases`, follows each name the load takes to
//! its declaring file through the barrels' re-exports ([`gate_typescript::exports`]), and reports:
//!
//! - `kind-barrel`: a name loaded from another module than its kind barrel, its `internal` barrel,
//!   or its file (a file in no kind folder, or one no barrel exports);
//! - `own-kind-folder`: inside a kind folder, a file of that folder loaded through a barrel, or by
//!   a non-relative specifier where a relative path with at most one `../` reaches it;
//! - `resource-relative`: a relative path that leaves the importer's resource;
//! - `index-segment`: a specifier whose last segment is `index`;
//! - `dynamic-import`: an `import()` whose names come from more than one module, or that defers a
//!   module the file already loads statically;
//! - `mock-target`: a mock call naming a module no source file loads under the form, or a hoisted
//!   mock call naming one module twice in a file.

mod model;

use std::collections::{BTreeMap, BTreeSet, HashSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Rule, Run};
use gate_typescript::loads::{Form, Load, Names, file_loads};

use self::model::{Edges, Model, Target, below, closure, name, parent};
use crate::layout::{Layout, PluginOptions};
use crate::shared::modules::{source_target, stem};
use crate::shared::paths::{self, Form as PathForm};
use crate::shared::report::Findings;
use crate::shared::{project, tree};

/// The rule.
#[derive(Debug, Default)]
pub struct KindImports;

/// The options of `nestjs/kind-imports`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The kinds whose folders are kind folders, each one folder name; `layout.exclusive-kind-folders`
    /// when not set.
    pub kinds: Option<Vec<String>>,
    /// The file name of a folder's barrel.
    pub barrel_name: String,
    /// The child of a kind folder whose files its own barrel exports.
    pub internal_folder: String,
    /// The module that lists the migrations, as `nestjs/migrations` names it.
    pub registry: String,
    /// The specifier the registry imports the migrations from, as `nestjs/migrations` names it.
    pub registry_import: String,
    /// The calls, by dotted name, whose first argument names a module to mock or read, below the
    /// test root.
    pub mock_calls: Vec<String>,
    /// The calls of `mock-calls` that the test runner hoists and keeps once per module.
    pub hoisted_mock_calls: Vec<String>,
    /// The folder whose files are entities, as `nestjs/import-aliases` names it.
    pub entity_folder: String,
    /// The kind of an enum file, as `nestjs/import-aliases` names it.
    pub enum_kind: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            kinds: None,
            barrel_name: "index.ts".to_owned(),
            internal_folder: "internal".to_owned(),
            registry: "src/migration/typeorm/migrations.ts".to_owned(),
            registry_import: "./generated".to_owned(),
            mock_calls: ["vi.mock", "vi.doMock", "vi.importActual", "vi.importMock"]
                .map(str::to_owned)
                .to_vec(),
            hoisted_mock_calls: vec!["vi.mock".to_owned()],
            entity_folder: "entity".to_owned(),
            enum_kind: "enum".to_owned(),
        }
    }
}

impl Rule<crate::Nest> for KindImports {
    type Options = Options;

    const NAME: &'static str = "kind-imports";
    const TITLE: &'static str =
        "Each declaration is loaded through the barrel of its kind folder, or by its file";
    const ASPECTS: &'static [&'static str] = &[
        "dynamic-import",
        "index-segment",
        "kind-barrel",
        "mock-target",
        "own-kind-folder",
        "resource-relative",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            let kinds = check_options(run.options(), &layout)?;
            let typescript = shared.typescript()?;
            let mut findings = Findings::default();
            Checker::check(run.options(), &layout, &kinds, typescript, &mut findings)?;
            findings.report(run, help);
            Ok(())
        })
    }
}

/// The help line of each aspect.
fn help(aspect: &str) -> &'static str {
    match aspect {
        "kind-barrel" => {
            "load each name from the module the layout gives it: the barrel of the kind folder that declares it, or its file"
        }
        "own-kind-folder" => {
            "inside a kind folder, load its files by their own paths, relative while one ../ reaches them"
        }
        "resource-relative" => "load another resource through its owner alias",
        "index-segment" => "name the folder, not its index file",
        "dynamic-import" => {
            "give each import() one module, and defer only modules the file does not load statically"
        }
        _ => "mock the module the source under test loads, once per file",
    }
}

/// Checks the options and returns the kinds.
fn check_options(options: &Options, layout: &Layout) -> gate_sdk::Result<Vec<String>> {
    let kinds = match &options.kinds {
        Some(kinds) => {
            paths::check_all("kinds", kinds, PathForm::Name)?;
            kinds.clone()
        }
        None => layout.exclusive_kind_folders.clone(),
    };
    if kinds.is_empty() {
        return Err(Error::config(if options.kinds.is_some() {
            "option `kinds` is empty; list the kinds whose folders hold one kind of file, such as [\"dto\", \"function\"]"
                .to_owned()
        } else {
            "option `kinds` is not set and layout.exclusive-kind-folders is empty; list the kinds whose folders hold one kind of file"
                .to_owned()
        }));
    }
    duplicates("kinds", &kinds)?;
    paths::check("barrel-name", &options.barrel_name, PathForm::Name)?;
    paths::check("internal-folder", &options.internal_folder, PathForm::Name)?;
    paths::check("registry", &options.registry, PathForm::File)?;
    paths::check("entity-folder", &options.entity_folder, PathForm::Name)?;
    paths::check("enum-kind", &options.enum_kind, PathForm::Name)?;
    if options.registry_import.is_empty() {
        return Err(Error::config(
            "option `registry-import` is empty; write the specifier the registry imports the migrations from, such as \"./generated\"",
        ));
    }
    for (index, call) in options.mock_calls.iter().enumerate() {
        let dotted = !call.is_empty()
            && call.split('.').all(|part| {
                part.chars().next().is_some_and(|first| {
                    first.is_ascii_alphabetic() || first == '_' || first == '$'
                }) && part
                    .chars()
                    .all(|letter| letter.is_ascii_alphanumeric() || letter == '_' || letter == '$')
            });
        if !dotted {
            return Err(Error::config(format!(
                "option `mock-calls[{index}]` is {call:?}, which is not a dotted name; write a callee such as \"vi.mock\""
            )));
        }
    }
    duplicates("mock-calls", &options.mock_calls)?;
    duplicates("hoisted-mock-calls", &options.hoisted_mock_calls)?;
    for (index, call) in options.hoisted_mock_calls.iter().enumerate() {
        if !options.mock_calls.contains(call) {
            return Err(Error::config(format!(
                "option `hoisted-mock-calls[{index}]` is {call:?}, which `mock-calls` does not list; list it there too, or remove it"
            )));
        }
    }
    Ok(kinds)
}

/// Fails when `values` names one value twice.
fn duplicates(option: &str, values: &[String]) -> gate_sdk::Result<()> {
    let mut seen = HashSet::new();
    for (index, value) in values.iter().enumerate() {
        if !seen.insert(value) {
            return Err(Error::config(format!(
                "option `{option}[{index}]` is {value:?}, which the list already holds; remove the repeat"
            )));
        }
    }
    Ok(())
}

/// One file's loads, each with the module it resolves to below the source root.
struct Read {
    path: String,
    loads: Vec<(Load, Option<String>)>,
}

struct Checker<'c, 'a> {
    options: &'c Options,
    model: Model<'c, 'a>,
    /// The resolved value loads of every file read.
    edges: Edges,
    /// The files of a kind folder that another file of it, other than a barrel, loads.
    loaded_inside: HashSet<String>,
}

impl<'c, 'a> Checker<'c, 'a> {
    fn check(
        options: &'c Options,
        layout: &'c Layout,
        kinds: &[String],
        typescript: &'c gate_typescript::TypeScript<'a>,
        findings: &mut Findings,
    ) -> gate_sdk::Result<()> {
        let source = typescript.source();
        let settings = typescript.settings();
        let sources: HashSet<&'a str> =
            tree::typescript_files(source, &layout.source_root, settings).collect();
        let files = tree::distinct(
            tree::typescript_files(source, &layout.source_root, settings)
                .chain(tree::typescript_files(source, &layout.test_root, settings)),
        );
        let model = Model::new(
            typescript,
            layout,
            kinds,
            &options.barrel_name,
            &options.internal_folder,
            sources,
        );
        let calls: Vec<&str> = options.mock_calls.iter().map(String::as_str).collect();

        let mut read = Vec::new();
        for path in files {
            let Some(file) = project::parsed(typescript, path)? else {
                continue;
            };
            let loads = file_loads(&file, &calls)
                .into_iter()
                .map(|load| {
                    let module = model.resolve(path, &load.specifier);
                    (load, module)
                })
                .collect();
            read.push(Read {
                path: path.to_owned(),
                loads,
            });
        }

        let mut edges = Edges::new();
        let mut loaded_inside = HashSet::new();
        for file in &read {
            let kind = model
                .in_source(&file.path)
                .then(|| model.kind_folder(&file.path))
                .flatten();
            for (load, module) in &file.loads {
                let Some(module) = module else { continue };
                if is_value(load) {
                    edges
                        .entry(file.path.clone())
                        .or_default()
                        .push(module.clone());
                }
                // A load in the form: a file of the importer's own kind folder, by its file.
                if let Some(kind) = kind
                    && !model.is_barrel(&file.path)
                    && !model.is_barrel(module)
                    && *module != file.path
                    && model.kind_folder(module) == Some(kind)
                    && !matches!(load.form, Form::Call(_))
                {
                    loaded_inside.insert(module.clone());
                }
            }
        }

        let checker = Checker {
            options,
            model,
            edges,
            loaded_inside,
        };
        for file in &read {
            checker.file(file, findings)?;
        }
        Ok(())
    }

    fn file(&self, file: &Read, findings: &mut Findings) -> gate_sdk::Result<()> {
        let path = file.path.as_str();
        let in_source = self.model.in_source(path);
        let in_test = below(self.model.layout.test_root.trim_end_matches('/'), path);
        let mut hoisted: BTreeMap<&str, Vec<&Load>> = BTreeMap::new();
        let mut statics: Option<HashSet<String>> = None;

        for (load, module) in &file.loads {
            let Some(module) = module.as_deref() else {
                continue;
            };
            if let Form::Call(_) = load.form
                && !in_test
            {
                continue;
            }
            if self.skipped(path, load) {
                continue;
            }
            let line = u32::try_from(load.line).ok();
            let specifier = load.specifier.as_str();
            let mut push = |aspect: &'static str, message: String| {
                findings.push_keyed(aspect, path, line, specifier, message);
            };

            if name(stem(specifier.trim_end_matches('/'))) == "index" {
                push(
                    "index-segment",
                    format!("{path} loads {specifier}; drop the /index segment"),
                );
            }

            if let Form::Call(index) = load.form {
                let call = self.options.mock_calls[index].as_str();
                if self
                    .options
                    .hoisted_mock_calls
                    .iter()
                    .any(|known| known == call)
                {
                    hoisted.entry(module).or_default().push(load);
                }
                if !self.loaded_under_the_form(module)? {
                    let fix = match self.model.kind_folder(module) {
                        Some(kind) => format!("name the kind barrel {kind}"),
                        None => "name the kind barrel or file that the source loads".to_owned(),
                    };
                    push(
                        "mock-target",
                        format!(
                            "{path} names {specifier} in {call}, which no source file loads under the form; {fix}"
                        ),
                    );
                }
                continue;
            }
            if load.form == Form::SideEffect {
                continue;
            }
            let direct_child = self.direct_child(path, load);

            let targets = self.targets(path, load, module)?;
            if !direct_child {
                self.form(path, load, module, &targets, &mut push);
            }

            if in_source
                && !direct_child
                && is_relative(specifier)
                && specifier.split('/').filter(|part| *part == "..").count() <= 1
                && self.model.in_source(module)
                && self.model.owner(module) == self.model.owner(path)
            {
                let own = self.model.resource(parent(path));
                if own != self.model.resource(parent(module)) {
                    push(
                        "resource-relative",
                        format!(
                            "{path} loads {specifier}, which leaves its resource {own}; use the owner alias"
                        ),
                    );
                }
            }

            if load.form == Form::Dynamic {
                let modules: BTreeSet<String> = targets
                    .iter()
                    .map(|taken| self.shown(taken.target.module()))
                    .collect();
                if modules.len() > 1 {
                    push(
                        "dynamic-import",
                        format!(
                            "{path} loads {specifier} with import(); its names come from {} modules ({}); use one import() per module",
                            modules.len(),
                            modules.into_iter().collect::<Vec<_>>().join(", ")
                        ),
                    );
                }
                if in_source {
                    let statics = statics.get_or_insert_with(|| closure(&self.edges, path));
                    let deferred_loaded = statics.contains(module)
                        || targets
                            .iter()
                            .any(|taken| statics.contains(taken.declaring.as_str()));
                    if deferred_loaded {
                        push(
                            "dynamic-import",
                            format!(
                                "{path} defers {specifier} with import() but already loads it statically; place the deferred module behind a barrel that {path} does not load statically"
                            ),
                        );
                    }
                }
            }
        }

        for loads in hoisted.values() {
            if let [first, _, ..] = loads.as_slice() {
                let call = match first.form {
                    Form::Call(index) => self.options.mock_calls[index].as_str(),
                    _ => continue,
                };
                let specifier = first.specifier.as_str();
                findings.push_keyed(
                    "mock-target",
                    path,
                    u32::try_from(first.line).ok(),
                    specifier,
                    format!(
                        "{path} names {specifier} in {call} more than once; the test runner keeps only the last factory, so merge the factories"
                    ),
                );
            }
        }
        Ok(())
    }

    /// The `kind-barrel` and `own-kind-folder` findings of one load.
    fn form(
        &self,
        path: &str,
        load: &Load,
        module: &str,
        targets: &[Taken],
        push: &mut impl FnMut(&'static str, String),
    ) {
        let specifier = load.specifier.as_str();
        // Groups by module path, then wording: the order of a message.
        let mut own: BTreeMap<(&str, String), BTreeSet<&str>> = BTreeMap::new();
        let mut other: BTreeMap<(&str, String), BTreeSet<&str>> = BTreeMap::new();
        for Taken { name, target, .. } in targets {
            if target.holds(module) {
                continue;
            }
            match target {
                Target::File { own: true, .. } => own
                    .entry((target.module(), target.wording()))
                    .or_default()
                    .insert(name),
                _ => other
                    .entry((target.module(), target.wording()))
                    .or_default()
                    .insert(name),
            };
        }

        if !other.is_empty() {
            push(
                "kind-barrel",
                format!("{path} loads {specifier}; import {}", groups(&other)),
            );
        }
        if !own.is_empty() {
            let kind = self.model.kind_folder(path).unwrap_or_default();
            push(
                "own-kind-folder",
                format!(
                    "{path} loads {specifier} from inside its kind folder {kind}; import {}",
                    groups(&own)
                ),
            );
        }

        // A file of the importer's own kind folder, in the form, named past a relative path.
        if other.is_empty()
            && own.is_empty()
            && !is_relative(specifier)
            && self.model.in_source(path)
            && let Some(kind) = self.model.kind_folder(path)
            && self.model.kind_folder(module) == Some(kind)
            && !self.model.is_barrel(module)
            && (below(parent(path), module) || below(parent(parent(path)), module))
        {
            push(
                "own-kind-folder",
                format!(
                    "{path} loads {specifier} by an owner alias; a relative path with at most one ../ reaches {module} in its own kind folder"
                ),
            );
        }
    }

    /// Each name the load takes that has a declaring file, with that file and its target.
    fn targets(&self, path: &str, load: &Load, module: &str) -> gate_sdk::Result<Vec<Taken>> {
        let exports = self.model.module_exports(module)?;
        let names: Vec<&str> = match &load.names {
            Names::All => exports.names.keys().map(String::as_str).collect(),
            Names::Listed(names) => names.iter().map(String::as_str).collect(),
        };
        let mut targets = Vec::new();
        for name in names {
            if let Some(Some(declaring)) = exports.names.get(name) {
                targets.push(Taken {
                    name: name.to_owned(),
                    declaring: declaring.clone(),
                    target: self.model.target(path, name, declaring)?,
                });
            }
        }
        Ok(targets)
    }

    /// Whether a mock call naming `module` names one the form makes the source load.
    fn loaded_under_the_form(&self, module: &str) -> gate_sdk::Result<bool> {
        let model = &self.model;
        Ok(model.is_kind_barrel(module)
            || model.is_internal_barrel(module)
            || (!model.is_barrel(module) && model.kind_folder(module).is_none())
            || (model.kind_folder(module).is_some() && model.is_private(module)?)
            || self.loaded_inside.contains(module))
    }

    /// A barrel shown by its folder, a file by its path.
    fn shown(&self, module: &str) -> String {
        if self.model.is_barrel(module) {
            parent(module).to_owned()
        } else {
            module.to_owned()
        }
    }

    /// Whether `load` of the barrel `path` re-exports a direct child: `./<one segment>`.
    fn direct_child(&self, path: &str, load: &Load) -> bool {
        self.model.is_barrel(path)
            && matches!(
                load.form,
                Form::ExportFrom | Form::ExportAll | Form::ExportAllAs
            )
            && load.specifier.strip_prefix("./").is_some_and(|rest| {
                !rest.is_empty() && !rest.contains('/') && rest != "." && rest != ".."
            })
    }

    /// Whether another rule owns the load: the registry's import of its migrations
    /// (`nestjs/migrations`), and an entity's load of an enum file
    /// (`nestjs/import-aliases#entity-enum-barrel`).
    fn skipped(&self, path: &str, load: &Load) -> bool {
        if path == self.options.registry && load.specifier == self.options.registry_import {
            return true;
        }
        let layout = self.model.layout;
        if !self.model.in_source(path) || name(parent(path)) != self.options.entity_folder {
            return false;
        }
        let Some(target) = source_target(layout, path, &load.specifier) else {
            return false;
        };
        std::path::Path::new(name(stem(&target)))
            .extension()
            .is_some_and(|extension| extension == self.options.enum_kind.as_str())
    }
}

/// One name a load takes, its declaring file and its target.
struct Taken {
    name: String,
    declaring: String,
    target: Target,
}

/// `{names} {wording}` per group, groups joined by ` and `.
fn groups(groups: &BTreeMap<(&str, String), BTreeSet<&str>>) -> String {
    groups
        .iter()
        .map(|((_, wording), names)| {
            format!(
                "{} {wording}",
                names.iter().copied().collect::<Vec<_>>().join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join(" and ")
}

/// Whether a load takes a runtime value: the edges of a static closure.
fn is_value(load: &Load) -> bool {
    !load.type_only
        && matches!(
            load.form,
            Form::Import
                | Form::SideEffect
                | Form::ExportFrom
                | Form::ExportAll
                | Form::ExportAllAs
                | Form::ImportEquals
                | Form::Require
        )
}

/// Whether a specifier is relative.
fn is_relative(specifier: &str) -> bool {
    specifier == "."
        || specifier == ".."
        || specifier.starts_with("./")
        || specifier.starts_with("../")
}

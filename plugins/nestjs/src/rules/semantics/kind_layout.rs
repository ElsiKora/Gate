//! `nestjs/kind-layout`: source and unit test files sit where their kind belongs.
//!
//! The folders below `layout.source-root` that hold a tracked file, and the tracked `.ts` files
//! (declaration files included), outside `[languages.typescript] skip-folders`, are judged:
//!
//! - `nested-kind-folder`: a folder with a `layout.role-folders` segment below the source root and
//!   a `layout.kind-folders` segment after it;
//! - `mixed-kind-folder`: a folder whose last segment is a `layout.exclusive-kind-folders` entry
//!   below a folder of another such kind;
//! - `utility-placement`: a `utility-folder` folder outside `utility-root`;
//! - `controller-placement`: a file below `layout.modules-root` inside a `controller-folder`
//!   folder, or named `<kebab>.<controller-folder>.ts`;
//! - `owner-file-placement`: an `owner-files` file inside an exclusive kind or role folder;
//! - `subscriber-placement`: a file named `<subscriber-folder>.ts`, or a
//!   `*.<subscriber-folder>.ts` file outside a `subscriber-folder` folder;
//! - `subscriber-name`: a subscriber directly in a `subscriber-folder` folder named by a lifecycle
//!   name or a lifecycle prefix followed by a kebab word;
//! - `unit-namespace`: a file below `unit-root` whose first folder is a folder directly below
//!   `layout.modules-root`, not one of `unit-categories`, not below one of
//!   `unit-exempt-prefixes`, and not a folder of its own below the source root.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};

use crate::layout::{Layout, PluginOptions};
use crate::shared::{project, tree};

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/kind-layout`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The name of a folder of helpers, allowed only below `utility-root`.
    pub utility_folder: String,
    /// The only folder whose descendants may be helper folders.
    pub utility_root: String,
    /// The controller kind: no folder of this name below the modules root, and no
    /// `<kebab>.<controller-folder>.ts` file there.
    pub controller_folder: String,
    /// The files that sit at an owner root, never inside a kind or role folder.
    pub owner_files: Vec<String>,
    /// The subscriber kind: the folder subscribers live in and the kind of their files.
    pub subscriber_folder: String,
    /// Subscriber names that describe a framework layer or lifecycle operation.
    pub lifecycle_subscriber_names: Vec<String>,
    /// Prefixes that, followed by a kebab word, name a subscriber by lifecycle.
    pub lifecycle_subscriber_prefixes: Vec<String>,
    /// The root of the unit tests.
    pub unit_root: String,
    /// Folders directly below the unit root that are categories of their own, not module areas.
    pub unit_categories: Vec<String>,
    /// Path prefixes below the unit root that are never judged.
    pub unit_exempt_prefixes: Vec<String>,
}

/// Owned strings of a list of literals.
fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

impl Default for Options {
    fn default() -> Self {
        Self {
            utility_folder: "utility".to_owned(),
            utility_root: "src/shared".to_owned(),
            controller_folder: "controller".to_owned(),
            owner_files: strings(&["controller.ts", "module.ts", "service.ts"]),
            subscriber_folder: "subscriber".to_owned(),
            lifecycle_subscriber_names: strings(&[
                "create", "delete", "function", "route", "update",
            ]),
            lifecycle_subscriber_prefixes: strings(&["on-after-", "on-before-"]),
            unit_root: "test/unit".to_owned(),
            unit_categories: strings(&["api", "fixture", "scripts"]),
            unit_exempt_prefixes: strings(&["test/unit/infrastructure/cdk/"]),
        }
    }
}

/// One finding before it is reported: aspect, path, key, message and help.
type Pending = (&'static str, String, Option<&'static str>, String, String);

impl gate_sdk::Rule for Rule {
    type Options = Options;

    const NAME: &'static str = "kind-layout";
    const TITLE: &'static str =
        "Kind folders, controllers, subscribers and unit tests sit where their kind belongs";
    const ASPECTS: &'static [&'static str] = &[
        "controller-placement",
        "mixed-kind-folder",
        "nested-kind-folder",
        "owner-file-placement",
        "subscriber-name",
        "subscriber-placement",
        "unit-namespace",
        "utility-placement",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        {
            use crate::shared::paths::Form::{Folder, Name};
            let o = run.options();
            crate::shared::paths::check("utility-folder", &o.utility_folder, Name)?;
            crate::shared::paths::check("utility-root", &o.utility_root, Folder)?;
            crate::shared::paths::check("controller-folder", &o.controller_folder, Name)?;
            crate::shared::paths::check("subscriber-folder", &o.subscriber_folder, Name)?;
            crate::shared::paths::check("unit-root", &o.unit_root, Folder)?;
        }

        let layout = PluginOptions::read(run)?.layout;
        let source = project::source(run);
        let settings = project::settings(run)?;
        let judge = Judge {
            layout: &layout,
            options: run.options(),
        };
        let mut pending = Vec::new();

        for folder in tree::folders(&source, &layout.source_root, &settings) {
            judge.folder(&folder, &mut pending);
        }

        for path in tree::typescript_files(&source, &layout.modules_root, &settings) {
            judge.controller(path, &mut pending);
        }

        for path in tree::typescript_files(&source, &layout.source_root, &settings) {
            judge.owner_file(path, &mut pending);
            judge.subscriber(path, &mut pending);
        }

        let areas: Vec<String> = tree::entries(&source, &layout.modules_root, &tree::UNSKIPPED)
            .into_iter()
            .filter_map(|(name, folder)| folder.then_some(name))
            .collect();

        for path in tree::typescript_files(&source, &judge.options.unit_root, &settings) {
            let area = path
                .split('/')
                .nth(segments(&judge.options.unit_root).len());

            if let Some(area) = area
                && areas.iter().any(|known| known == area)
                && tree::files(&source, &join(&layout.source_root, area), &tree::UNSKIPPED)
                    .next()
                    .is_none()
            {
                judge.unit(path, area, &mut pending);
            }
        }

        for (aspect, path, key, message, help) in pending {
            let reported = run
                .report()
                .finding(aspect, Location::file(path), message)
                .help(help);

            if let Some(key) = key {
                reported.key(key);
            }
        }

        Ok(())
    }
}

/// The segments of a path; none for the empty path.
fn segments(path: &str) -> Vec<&str> {
    if path.is_empty() {
        Vec::new()
    } else {
        path.split('/').collect()
    }
}

/// `name` below `folder`; `name` itself below the empty folder.
fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

/// The last segment of a path.
fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// Whether a name is one or more lowercase letters, digits and hyphens.
fn is_kebab_word(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Whether `name` is one of `list`.
fn listed(list: &[String], name: &str) -> bool {
    list.iter().any(|entry| entry == name)
}

/// The layout and options the judgements read.
struct Judge<'a> {
    layout: &'a Layout,
    options: &'a Options,
}

impl Judge<'_> {
    /// The folder findings: `nested-kind-folder`, `mixed-kind-folder` and `utility-placement`.
    fn folder(&self, folder: &str, pending: &mut Vec<Pending>) {
        if self.nests_kind_folder(folder) {
            pending.push((
                "nested-kind-folder",
                folder.to_owned(),
                None,
                format!("{folder} nests a kind folder inside a role folder"),
                "move the kind folder out of the role folder".to_owned(),
            ));
        }

        if let Some((outer, inner)) = self.mixed_kind_folder(folder) {
            pending.push((
                "mixed-kind-folder",
                folder.to_owned(),
                None,
                format!("{folder} nests a {inner} kind folder inside a {outer} kind folder"),
                format!("move the {inner} folder out of the {outer} folder"),
            ));
        }

        let utility_root = &self.options.utility_root;
        let below_utility_root = crate::layout::folder_prefix(utility_root);

        if folder
            .rsplit_once('/')
            .is_some_and(|(_, name)| *name == self.options.utility_folder)
            && !folder.starts_with(&below_utility_root)
        {
            pending.push((
                "utility-placement",
                folder.to_owned(),
                Some("utility-folder"),
                format!(
                    "{folder} is a utility folder outside {utility_root}; helpers there are function/<name>.function.ts files"
                ),
                "turn the helpers into function/<name>.function.ts files".to_owned(),
            ));
        }
    }

    /// Whether a folder has a kind folder after a role folder below the source root.
    fn nests_kind_folder(&self, folder: &str) -> bool {
        let folders: Vec<&str> = folder.split('/').collect();

        folders
            .iter()
            .enumerate()
            .skip(segments(&self.layout.source_root).len())
            .any(|(index, segment)| {
                listed(&self.layout.role_folders, segment)
                    && folders[index + 1..]
                        .iter()
                        .any(|later| listed(&self.layout.kind_folders, later))
            })
    }

    /// The outer and inner kind of a folder whose last segment is an exclusive kind folder below
    /// an exclusive kind folder of another kind, such as `src/common/utility/type`.
    fn mixed_kind_folder<'f>(&self, folder: &'f str) -> Option<(&'f str, &'f str)> {
        let kinds = &self.layout.exclusive_kind_folders;
        let mut folders = folder.split('/').rev();
        let inner = folders.next().filter(|last| listed(kinds, last))?;
        let outer = folders.find(|segment| listed(kinds, segment))?;

        (outer != inner).then_some((outer, inner))
    }

    /// The `controller-placement` findings of a file below the modules root.
    fn controller(&self, path: &str, pending: &mut Vec<Pending>) {
        let controller = &self.options.controller_folder;

        if path.contains(&format!("/{controller}/")) {
            pending.push((
                "controller-placement",
                path.to_owned(),
                None,
                format!(
                    "{path} is inside a controller kind folder; controller.ts belongs at the owning resource root"
                ),
                "move the controller to controller.ts at the owning resource root".to_owned(),
            ));
        }

        if file_name(path)
            .strip_suffix(&format!(".{controller}.ts"))
            .is_some_and(is_kebab_word)
        {
            pending.push((
                "controller-placement",
                path.to_owned(),
                None,
                format!(
                    "{path} uses a prefixed controller filename; use controller.ts at the owning resource root"
                ),
                "rename the file to controller.ts at the owning resource root".to_owned(),
            ));
        }
    }

    /// The `owner-file-placement` finding of an owner file inside an exclusive kind or role
    /// folder.
    fn owner_file(&self, path: &str, pending: &mut Vec<Pending>) {
        let name = file_name(path);

        if !listed(&self.options.owner_files, name) {
            return;
        }

        if let Some(folder) = path.split('/').rev().skip(1).find(|folder| {
            listed(&self.layout.exclusive_kind_folders, folder)
                || listed(&self.layout.role_folders, folder)
        }) {
            pending.push((
                "owner-file-placement",
                path.to_owned(),
                None,
                format!(
                    "{path} is inside a {folder} folder; {name} belongs at the owning resource root"
                ),
                format!("move {name} to the owning resource root"),
            ));
        }
    }

    /// The `subscriber-placement` and `subscriber-name` findings of a source file.
    fn subscriber(&self, path: &str, pending: &mut Vec<Pending>) {
        let subscriber = &self.options.subscriber_folder;
        let name = file_name(path);
        let suffix = format!(".{subscriber}.ts");

        if name == format!("{subscriber}.ts")
            || (name.ends_with(&suffix)
                && !path
                    .split('/')
                    .rev()
                    .skip(1)
                    .any(|folder| folder == subscriber))
        {
            pending.push((
                "subscriber-placement",
                path.to_owned(),
                None,
                format!("{path} is a subscriber outside a subscriber kind folder"),
                format!("move the subscriber into a {subscriber} folder"),
            ));
        }

        if self.lifecycle_subscriber(path) {
            pending.push((
                "subscriber-name",
                path.to_owned(),
                None,
                format!(
                    "{path} names a subscriber by framework layer or lifecycle instead of responsibility"
                ),
                "name the subscriber by the responsibility it carries".to_owned(),
            ));
        }
    }

    /// Whether a file is a subscriber directly in a subscriber folder named by a framework layer
    /// or lifecycle operation, such as `subscriber/route.subscriber.ts` or
    /// `subscriber/on-before-create.subscriber.ts`.
    fn lifecycle_subscriber(&self, path: &str) -> bool {
        let subscriber = &self.options.subscriber_folder;
        let Some((folder, name)) = path.rsplit_once('/') else {
            return false;
        };
        let Some(name) = name.strip_suffix(&format!(".{subscriber}.ts")) else {
            return false;
        };

        folder.ends_with(&format!("/{subscriber}"))
            && (listed(&self.options.lifecycle_subscriber_names, name)
                || self
                    .options
                    .lifecycle_subscriber_prefixes
                    .iter()
                    .any(|prefix| {
                        name.strip_prefix(prefix.as_str())
                            .is_some_and(is_kebab_word)
                    }))
    }

    /// The `unit-namespace` finding of a unit test of the module area `area`, unless it is a
    /// category or below an exempt prefix.
    fn unit(&self, path: &str, area: &str, pending: &mut Vec<Pending>) {
        let options = self.options;

        if listed(&options.unit_categories, area)
            || options
                .unit_exempt_prefixes
                .iter()
                .any(|prefix| path.starts_with(prefix.as_str()))
        {
            return;
        }

        let unit_root = &options.unit_root;
        let modules = file_name(&self.layout.modules_root);
        let rest = if unit_root.is_empty() {
            path
        } else {
            &path[unit_root.len() + 1..]
        };
        let target = join(&join(unit_root, modules), rest);

        pending.push((
            "unit-namespace",
            path.to_owned(),
            Some("namespace"),
            format!("{path} omits the {modules} namespace; use {target}"),
            format!("move the test to {target}"),
        ));
    }
}

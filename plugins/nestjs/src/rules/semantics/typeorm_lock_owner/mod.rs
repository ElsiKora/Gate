//! `nestjs/typeorm-lock-owner`: every `TypeORM` row lock of a read takes its options from one
//! owner function, through the read shapes the rule recognises.
//!
//! The rule reads every tracked `.ts` file below `roots`. It first checks that `owner` exports
//! exactly one function ([`owner`]). Then each file is walked once for its sites ([`sites`]): a
//! `lock` property of a read's options ([`reads`]), a `mode` property whose value folds to a
//! row-lock mode ([`values`]), a `setLock` or `setOnLocked` call on a query builder, and an owner
//! call that is not a `lock` value. Each site gets the first aspect that applies.

mod owner;
mod reads;
mod sites;
mod values;

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use gate_sdk::Run;
use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_typescript::TypeScript;
use gate_typescript::owners::{Owner, SourceContext, Sources};

use crate::layout::PluginOptions;
use crate::shared::paths::{self, Form};
use crate::shared::report::Findings;

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/typeorm-lock-owner`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The one file that builds lock options; it exports exactly one function.
    pub owner: String,
    /// The folders whose tracked `.ts` files are read; by default `layout.source-root`.
    #[serde(default)]
    pub roots: Option<Vec<String>>,
    /// The package of the service base class. Absent means `layout.automator-package`.
    #[serde(default)]
    pub framework_package: Option<String>,
    /// The base class whose type argument names a service's entity.
    #[serde(default = "defaults::service_base")]
    pub service_base: String,
    /// The service methods whose options take the step-context form.
    #[serde(default = "defaults::service_read_methods")]
    pub service_read_methods: Vec<String>,
    /// The service method whose result's `getRepository(<Entity>)` is a service read's repository.
    #[serde(default = "defaults::step_context_method")]
    pub step_context_method: String,
    /// The repository and entity-manager methods whose argument is a find-options object.
    #[serde(default = "defaults::read_methods")]
    pub read_methods: Vec<String>,
    /// The lock modes that lock rows.
    #[serde(default = "defaults::lock_modes")]
    pub lock_modes: Vec<String>,
}

/// The defaults of the options that have one.
mod defaults {
    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    pub fn service_base() -> String {
        "ApiServiceBase".to_owned()
    }

    pub fn service_read_methods() -> Vec<String> {
        strings(&["get", "getList", "getMany"])
    }

    pub fn step_context_method() -> String {
        "getApiFunctionStepContext".to_owned()
    }

    pub fn read_methods() -> Vec<String> {
        strings(&[
            "count",
            "exists",
            "find",
            "findAndCount",
            "findOne",
            "findOneOrFail",
        ])
    }

    pub fn lock_modes() -> Vec<String> {
        strings(&[
            "for_key_share",
            "for_no_key_update",
            "pessimistic_read",
            "pessimistic_write",
        ])
    }
}

/// The lock modes `TypeORM` defines that lock no row: a `lock` whose mode folds to one of them
/// (and is not listed in `lock-modes`) is no site.
const ROW_FREE_MODES: [&str; 2] = ["dirty_read", "optimistic"];

/// The aspects, sorted.
const ASPECTS: &[&str] = &[
    "builder",
    "lock-object",
    "lock-value",
    "owner",
    "receiver",
    "row-list",
    "unreadable",
];

/// What the checks of one run share: the options, the owner, the packages and the sources.
pub(crate) struct Context<'r, 'a> {
    /// The owner file.
    pub owner: &'r str,
    /// The owner's exported function: its export name and the name messages print.
    pub function: owner::Function,
    /// The service base class and its package.
    pub service_base: &'r str,
    pub framework_package: String,
    /// The `TypeORM` package.
    pub typeorm_package: String,
    pub service_read_methods: &'r [String],
    pub step_context_method: &'r str,
    pub read_methods: &'r [String],
    pub lock_modes: &'r [String],
    /// The modes that lock no row.
    pub row_free_modes: Vec<&'static str>,
    pub typescript: &'r TypeScript<'a>,
    /// The sources of each root.
    pub roots: Vec<Rc<Sources<'a>>>,
    /// Whether an export (file and name) is the owner's function, as answered so far.
    pub exports: RefCell<HashMap<(String, String), bool>>,
}

impl<'a> Context<'_, 'a> {
    /// The context of a file below a root.
    pub fn source(&self, path: &str) -> Option<&SourceContext<'a>> {
        self.roots.iter().find_map(|sources| sources.get(path))
    }

    /// The owner of `name` exported by the file at `path`, chased through the sources of the
    /// root that holds it.
    pub fn declaration(&self, path: &str, name: &str) -> Option<Owner> {
        self.roots
            .iter()
            .find(|sources| sources.get(path).is_some())
            .and_then(|sources| sources.owner(Some(path), name))
    }
}

impl Options {
    /// Rejects a path that is not normal, an empty name and an empty list or list entry.
    fn validate(&self) -> gate_sdk::Result<()> {
        paths::check("owner", &self.owner, Form::File)?;
        paths::check_all("roots", self.roots.iter().flatten(), Form::Folder)?;

        for (option, value) in [
            ("service-base", self.service_base.as_str()),
            ("step-context-method", self.step_context_method.as_str()),
        ] {
            if value.is_empty() {
                return Err(gate_sdk::Error::config(format!(
                    "the option `{option}` is empty; name it, or leave the option out for its default"
                )));
            }
        }

        if matches!(&self.roots, Some(roots) if roots.is_empty()) {
            return Err(gate_sdk::Error::config(
                "the option `roots` is empty; list at least one folder, or leave the option out to read layout.source-root",
            ));
        }

        for (option, values, item) in [
            ("lock-modes", &self.lock_modes, "lock mode"),
            ("read-methods", &self.read_methods, "read method"),
            (
                "service-read-methods",
                &self.service_read_methods,
                "service method",
            ),
        ] {
            if values.is_empty() {
                return Err(gate_sdk::Error::config(format!(
                    "the option `{option}` is empty; list at least one {item}, or leave the option out for its default"
                )));
            }

            if values.iter().any(String::is_empty) {
                return Err(gate_sdk::Error::config(format!(
                    "the option `{option}` has an empty entry; remove it, or name a {item} there"
                )));
            }
        }

        Ok(())
    }
}

impl gate_sdk::Rule<crate::Nest> for Rule {
    type Options = Options;

    const NAME: &'static str = "typeorm-lock-owner";
    const TITLE: &'static str =
        "Every TypeORM row lock of a read takes its options from the one lock owner";
    const ASPECTS: &'static [&'static str] = ASPECTS;
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            run.options().validate()?;
            let options = run.options();
            let framework_package = super::package::resolve(
                "framework-package",
                options.framework_package.as_deref(),
                &layout.automator_package,
            )?;
            let roots = options
                .roots
                .clone()
                .unwrap_or_else(|| vec![layout.source_root.clone()]);
            let typescript = shared.typescript()?;
            let sources = roots
                .iter()
                .map(|root| shared.contexts(root))
                .collect::<gate_sdk::Result<Vec<_>>>()?;
            let mut findings = Findings::default();
            let function = sources
                .iter()
                .find_map(|sources| sources.get(&options.owner))
                .and_then(owner::function);

            let Some(function) = function else {
                let configuration = run
                    .options_file()
                    .or(run.config_file())
                    .unwrap_or("gate.toml")
                    .to_owned();
                findings.push_keyed(
                    "owner",
                    configuration,
                    None,
                    "owner",
                    format!(
                        "{} is not a tracked TypeScript file that exports exactly one function, so no row lock can be checked against it.",
                        options.owner
                    ),
                );
                findings.report(run, help);
                return Ok(());
            };

            let context = Context {
                owner: &options.owner,
                function,
                service_base: &options.service_base,
                framework_package,
                typeorm_package: layout.typeorm_package.clone(),
                service_read_methods: &options.service_read_methods,
                step_context_method: &options.step_context_method,
                read_methods: &options.read_methods,
                lock_modes: &options.lock_modes,
                row_free_modes: ROW_FREE_MODES
                    .into_iter()
                    .filter(|mode| !options.lock_modes.iter().any(|listed| listed == mode))
                    .collect(),
                typescript,
                roots: sources,
                exports: RefCell::default(),
            };

            for finding in pending(&context) {
                findings.push_keyed(
                    finding.aspect,
                    finding.path,
                    finding.line,
                    finding.key,
                    finding.message,
                );
            }

            findings.report(run, help);
            Ok(())
        })
    }
}

/// The findings of every file below the roots, in path and line order.
fn pending(context: &Context<'_, '_>) -> Vec<sites::Pending> {
    let mut pending = Vec::new();
    let mut seen = BTreeSet::new();

    for sources in &context.roots {
        for path in context.typescript.files() {
            if sources.unparsed(path) && seen.insert((*path).to_owned()) {
                pending.push(sites::Pending::file(
                    "unreadable",
                    path,
                    format!("{path} does not parse, so its row locks are not checked."),
                ));
            }
        }

        for source in sources.iter() {
            if seen.insert(source.path.clone()) {
                pending.extend(sites::check(context, source));
            }
        }
    }

    pending.sort_by(|left, right| {
        (&left.path, left.line, left.start).cmp(&(&right.path, right.line, right.start))
    });
    pending
}

/// What to do about a finding of `aspect`.
fn help(aspect: &str) -> &'static str {
    match aspect {
        "owner" => "point `owner` at the tracked file that exports the one lock function",
        "unreadable" => "fix the syntax error so the file's row locks can be checked",
        "receiver" => "pass the repository the read runs on as the lock function's first argument",
        "row-list" => {
            "pass a list of distinct repositories only in a query builder's setFindOptions"
        }
        "builder" => {
            "lock a query builder through setFindOptions({ loadEagerRelations: false, lock: … })"
        }
        _ => "take the lock from the lock function, as the value of the read's lock property",
    }
}

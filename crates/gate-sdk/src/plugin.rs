//! A plugin's declaration and the three calls the host makes: `describe`, `check`, `generate`.
//!
//! The functions here are what the component exports call (see [`crate::plugin!`]); they take a
//! [`Host`], so the same code runs natively against a fixture tree.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::git::Git;
use crate::host::Host;
use crate::report::{Finding, Report};
use crate::rule::{
    Call, CallState, Enter, Entered, Generate, GeneratedFile, Generator, GeneratorInfo, Rule,
    RuleInfo, Run,
};
use crate::{Error, Result};

/// How the call runs one rule: the host, the rule's run and the call's shared state.
type RunRule<S> = fn(&dyn Host, &RuleRun, &Shared<'_, S>) -> Result<Vec<Finding>>;

/// One rule of a plugin, erased to what the host calls. `S` is the plugin's [`CallState`].
pub struct RuleEntry<S: CallState = ()> {
    name: &'static str,
    info: fn() -> RuleInfo,
    run: RunRule<S>,
}

impl<S: CallState> RuleEntry<S> {
    /// The entry of rule `R`.
    #[must_use]
    pub const fn of<R: Rule<S>>() -> Self {
        Self {
            name: R::NAME,
            info: R::info,
            run: run_rule::<R, S>,
        }
    }
}

impl<S: CallState> Clone for RuleEntry<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: CallState> Copy for RuleEntry<S> {}

impl<S: CallState> std::fmt::Debug for RuleEntry<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("RuleEntry")
            .field(&self.name)
            .finish()
    }
}

/// One generator of a plugin, erased to what the host calls.
#[derive(Clone, Copy)]
pub struct GeneratorEntry {
    name: &'static str,
    info: fn() -> GeneratorInfo,
    run: fn(&dyn Host, &str, &str, &Value) -> Result<GenerateOutput>,
}

impl GeneratorEntry {
    /// The entry of generator `G`.
    #[must_use]
    pub const fn of<G: Generator>() -> Self {
        Self {
            name: G::NAME,
            info: G::info,
            run: run_generator::<G>,
        }
    }
}

impl std::fmt::Debug for GeneratorEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("GeneratorEntry")
            .field(&self.name)
            .finish()
    }
}

/// A named set of rule settings a configuration can extend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    /// JSON: rule name -> `{ "level": .., "levels": { <aspect>: .. }, "options": .. }`, each key
    /// optional. A level of an aspect the rule declares applies only where the configuration gives
    /// the rule or that aspect no level: the project's `"git/x" = "note"` makes every aspect a
    /// note, a preset's `warning` aspect too.
    pub rules: &'static str,
}

/// The options of a plugin that has none of its own, for [`crate::plugin!`]'s `options`: only an
/// empty `[plugins.<name>] options` table, or none, passes.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NoOptions {}

/// Everything a plugin declares; [`crate::plugin!`] builds it as a `static`. `S` is the state the
/// rules of one call share ([`CallState`]); `()` for a plugin without one.
pub struct PluginSpec<S: CallState = ()> {
    /// The plugin's name, lowercase kebab: the prefix of its rule ids (`typescript/barrel-exports`).
    pub name: &'static str,
    pub version: &'static str,
    /// The JSON Schema of the `[plugins.<name>] options`, generated from the `options` type of
    /// [`crate::plugin!`].
    pub options_schema: fn() -> String,
    pub rules: &'static [RuleEntry<S>],
    pub generators: &'static [GeneratorEntry],
    pub presets: &'static [Preset],
}

impl<S: CallState> Clone for PluginSpec<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: CallState> Copy for PluginSpec<S> {}

impl<S: CallState> std::fmt::Debug for PluginSpec<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginSpec")
            .field("name", &self.name)
            .field("version", &self.version)
            .field("rules", &self.rules)
            .field("generators", &self.generators)
            .field("presets", &self.presets)
            .finish_non_exhaustive()
    }
}

/// A preset as `describe` declares it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresetInfo {
    pub name: String,
    /// JSON: rule name -> `{ "level": .., "options": .. }`.
    pub rules: String,
}

/// What `describe` answers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    /// The JSON Schema of the `[plugins.<name>] options`, which the host validates the
    /// configuration against when the plugin loads.
    pub options_schema: String,
    pub rules: Vec<RuleInfo>,
    pub generators: Vec<GeneratorInfo>,
    pub presets: Vec<PresetInfo>,
}

/// One enabled rule of a `check` call.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuleRun {
    pub name: String,
    /// JSON, already validated by the host against the rule's schema; empty means `{}`.
    pub options: String,
    /// The tracked file the options came from. The core fills it with the repository path; the
    /// rule receives it relative to its scope's root, as it addresses every path.
    pub options_file: Option<String>,
    pub exceptions: Vec<String>,
}

/// One `check` call: every enabled rule of the plugin for one project.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CheckRequest {
    /// The project's root relative to the repository; empty for the repository.
    pub project: String,
    pub rules: Vec<RuleRun>,
    /// JSON: the `[plugins.<name>]` options; empty means `{}`.
    pub plugin_options: String,
    /// JSON: the `[languages.<id>]` settings; empty means `{}`.
    pub languages: String,
    pub norms: Vec<String>,
}

/// What `generate` answers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GenerateOutput {
    /// The files the generator writes.
    pub files: Vec<GeneratedFile>,
    /// What the generator reports while it runs.
    pub findings: Vec<Finding>,
}

/// The parts of a `check` request every rule of it shares.
pub(crate) struct Shared<'r, S: CallState> {
    call: Call<'r>,
    /// The state the call's rules share; dropped when the call ends.
    state: &'r (dyn Enter<S> + 'r),
}

/// The declaration of `spec`.
#[must_use]
pub fn describe<S: CallState>(spec: &PluginSpec<S>) -> PluginInfo {
    PluginInfo {
        name: spec.name.to_owned(),
        version: spec.version.to_owned(),
        options_schema: (spec.options_schema)(),
        rules: spec.rules.iter().map(|rule| (rule.info)()).collect(),
        generators: spec
            .generators
            .iter()
            .map(|generator| (generator.info)())
            .collect(),
        presets: spec
            .presets
            .iter()
            .map(|preset| PresetInfo {
                name: preset.name.to_owned(),
                rules: preset.rules.to_owned(),
            })
            .collect(),
    }
}

/// Runs every rule `request` names over `host` and returns their findings in rule order.
/// The call's state ([`CallState`]) is built once, before the first rule, and dropped after the
/// last.
///
/// # Errors
/// A rule the plugin does not declare, options that do not match a rule's type, or a rule's own
/// error; the message names the rule.
pub fn check<S: CallState>(
    spec: &PluginSpec<S>,
    host: &dyn Host,
    request: &CheckRequest,
) -> Result<Vec<Finding>> {
    let plugin_options = json(&request.plugin_options, "the [plugins] options")?;
    let languages = json(&request.languages, "the [languages] settings")?;
    let call = Call {
        host,
        project: &request.project,
        plugin_options: &plugin_options,
        languages: &languages,
        norms: &request.norms,
    };
    let owner = S::own(&call);
    let state = Entered::<S>(S::state(&owner));
    let shared = Shared {
        call,
        state: &state,
    };
    let mut findings = Vec::new();

    for rule in &request.rules {
        let entry = spec
            .rules
            .iter()
            .find(|entry| entry.name == rule.name)
            .ok_or_else(|| {
                Error::new(format!(
                    "plugin {} has no rule {:?}; its rules are: {}",
                    spec.name,
                    rule.name,
                    spec.rules
                        .iter()
                        .map(|entry| entry.name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?;
        findings.extend((entry.run)(host, rule, &shared)?);
    }

    Ok(findings)
}

/// Runs generator `name` with `options` (JSON; empty means `{}`) for `project`; `plugin_options`
/// (JSON; empty means `{}`) are the `[plugins.<name>]` options, as a check call receives them.
///
/// # Errors
/// A generator the plugin does not declare, options that do not match, or the generator's error.
pub fn generate<S: CallState>(
    spec: &PluginSpec<S>,
    host: &dyn Host,
    name: &str,
    project: &str,
    options: &str,
    plugin_options: &str,
) -> Result<GenerateOutput> {
    let entry = spec
        .generators
        .iter()
        .find(|entry| entry.name == name)
        .ok_or_else(|| Error::new(format!("plugin {} has no generator {name:?}", spec.name)))?;
    let plugin_options = json(plugin_options, "the [plugins] options")?;
    (entry.run)(host, project, options, &plugin_options)
}

fn json(text: &str, what: &str) -> Result<Value> {
    if text.trim().is_empty() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    serde_json::from_str(text).map_err(|error| Error::new(format!("{what} are not JSON: {error}")))
}

fn options<T: serde::de::DeserializeOwned>(text: &str, what: &str) -> Result<T> {
    let text = if text.trim().is_empty() { "{}" } else { text };
    serde_json::from_str(text).map_err(|error| {
        Error::config(format!(
            "the options of {what} do not match its options type: {error}"
        ))
    })
}

/// `error` of `what` (`rule <name>` or `generator <name>`), of the same kind, with `what` named.
fn named(what: &str, error: Error) -> Error {
    let message = if error.is_config() {
        format!("{what}: {error}")
    } else {
        format!("{what} failed: {error}")
    };
    error.with_message(message)
}

fn run_rule<R: Rule<S>, S: CallState>(
    host: &dyn Host,
    rule: &RuleRun,
    shared: &Shared<'_, S>,
) -> Result<Vec<Finding>> {
    let what = format!("rule {}", R::NAME);
    let mut run = Run {
        call: shared.call,
        git: if R::READS_HISTORY || R::READS_MESSAGE {
            host.git()
                .map(|git| Git::new(git).reading_message(R::READS_MESSAGE))
        } else {
            None
        },
        report: Report::new(R::NAME),
        options: options::<R::Options>(&rule.options, &what)?,
        options_file: rule.options_file.clone(),
        exceptions: rule.exceptions.clone(),
        state: shared.state,
    };
    R::default()
        .check(&mut run)
        .map_err(|error| named(&what, error))?;
    Ok(run.report.into_findings())
}

fn run_generator<G: Generator>(
    host: &dyn Host,
    project: &str,
    text: &str,
    plugin_options: &Value,
) -> Result<GenerateOutput> {
    let mut run = Generate {
        host,
        report: Report::new(G::NAME),
        options: options::<G::Options>(text, &format!("generator {}", G::NAME))?,
        plugin_options: plugin_options.clone(),
        project: project.to_owned(),
        files: Vec::new(),
    };
    G::default()
        .generate(&mut run)
        .map_err(|error| named(&format!("generator {}", G::NAME), error))?;
    Ok(GenerateOutput {
        files: run.files,
        findings: run.report.into_findings(),
    })
}

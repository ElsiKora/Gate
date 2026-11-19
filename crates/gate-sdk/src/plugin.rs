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
use crate::rule::{Generate, GeneratedFile, Generator, GeneratorInfo, Rule, RuleInfo, Run};
use crate::{Error, Result};

/// One rule of a plugin, erased to what the host calls.
#[derive(Clone, Copy)]
pub struct RuleEntry {
    name: &'static str,
    info: fn() -> RuleInfo,
    run: fn(&dyn Host, &RuleRun, &Shared<'_>) -> Result<Vec<Finding>>,
}

impl RuleEntry {
    /// The entry of rule `R`.
    #[must_use]
    pub const fn of<R: Rule>() -> Self {
        Self {
            name: R::NAME,
            info: R::info,
            run: run_rule::<R>,
        }
    }
}

impl std::fmt::Debug for RuleEntry {
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
    run: fn(&dyn Host, &str, &str) -> Result<GenerateOutput>,
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
    /// JSON: rule name -> `{ "level": .., "options": .. }`.
    pub rules: &'static str,
}

/// The options of a plugin that has none of its own, for [`crate::plugin!`]'s `options`: only an
/// empty `[plugins.<name>] options` table, or none, passes.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NoOptions {}

/// Everything a plugin declares; [`crate::plugin!`] builds it as a `static`.
#[derive(Clone, Copy, Debug)]
pub struct PluginSpec {
    /// The plugin's name, lowercase kebab: the prefix of its rule ids (`typescript/barrel-exports`).
    pub name: &'static str,
    pub version: &'static str,
    /// The JSON Schema of the `[plugins.<name>] options`, generated from the `options` type of
    /// [`crate::plugin!`].
    pub options_schema: fn() -> String,
    pub rules: &'static [RuleEntry],
    pub generators: &'static [GeneratorEntry],
    pub presets: &'static [Preset],
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
pub(crate) struct Shared<'r> {
    project: &'r str,
    plugin_options: Value,
    languages: Value,
    norms: &'r [String],
}

/// The declaration of `spec`.
#[must_use]
pub fn describe(spec: &PluginSpec) -> PluginInfo {
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
///
/// # Errors
/// A rule the plugin does not declare, options that do not match a rule's type, or a rule's own
/// error; the message names the rule.
pub fn check(spec: &PluginSpec, host: &dyn Host, request: &CheckRequest) -> Result<Vec<Finding>> {
    let shared = Shared {
        project: &request.project,
        plugin_options: json(&request.plugin_options, "the [plugins] options")?,
        languages: json(&request.languages, "the [languages] settings")?,
        norms: &request.norms,
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

/// Runs generator `name` with `options` (JSON; empty means `{}`) for `project`.
///
/// # Errors
/// A generator the plugin does not declare, options that do not match, or the generator's error.
pub fn generate(
    spec: &PluginSpec,
    host: &dyn Host,
    name: &str,
    project: &str,
    options: &str,
) -> Result<GenerateOutput> {
    let entry = spec
        .generators
        .iter()
        .find(|entry| entry.name == name)
        .ok_or_else(|| Error::new(format!("plugin {} has no generator {name:?}", spec.name)))?;
    (entry.run)(host, project, options)
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
        Error::new(format!(
            "the options of {what} do not match its options type: {error}"
        ))
    })
}

fn run_rule<R: Rule>(host: &dyn Host, rule: &RuleRun, shared: &Shared<'_>) -> Result<Vec<Finding>> {
    let what = format!("rule {}", R::NAME);
    let mut run = Run {
        host,
        git: if R::READS_HISTORY {
            host.git().map(Git::new)
        } else {
            None
        },
        report: Report::new(R::NAME),
        options: options::<R::Options>(&rule.options, &what)?,
        options_file: rule.options_file.clone(),
        exceptions: rule.exceptions.clone(),
        project: shared.project.to_owned(),
        plugin_options: &shared.plugin_options,
        languages: &shared.languages,
        norms: shared.norms,
    };
    R::default()
        .check(&mut run)
        .map_err(|error| Error::new(format!("rule {} failed: {error}", R::NAME)))?;
    Ok(run.report.into_findings())
}

fn run_generator<G: Generator>(
    host: &dyn Host,
    project: &str,
    text: &str,
) -> Result<GenerateOutput> {
    let mut run = Generate {
        host,
        report: Report::new(G::NAME),
        options: options::<G::Options>(text, &format!("generator {}", G::NAME))?,
        project: project.to_owned(),
        files: Vec::new(),
    };
    G::default()
        .generate(&mut run)
        .map_err(|error| Error::new(format!("generator {} failed: {error}", G::NAME)))?;
    Ok(GenerateOutput {
        files: run.files,
        findings: run.report.into_findings(),
    })
}

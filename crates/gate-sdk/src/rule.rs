//! Rules and generators, and what one run of them sees.

use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::git::Git;
use crate::host::Host;
use crate::report::Report;
use crate::tree::Tree;
use crate::{Error, Result};

/// What a rule sees.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Scope {
    /// Its project's subtree, paths relative to the project root; one run per project.
    #[default]
    Project,
    /// The whole repository; one run per check.
    Repository,
}

/// A rule as `describe` declares it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleInfo {
    pub name: String,
    pub title: String,
    pub aspects: Vec<String>,
    pub scope: Scope,
    pub reads_history: bool,
    pub languages: Vec<String>,
    /// The JSON Schema of the options, generated from [`Rule::Options`].
    pub options_schema: String,
}

/// A key of the plugin's options (`[plugins.<name>] options`) that a rule reads and the plugin's
/// schema leaves optional, because other rules, or other configurations, do not read it. The core
/// checks it before anything runs: when the rule is enabled, the options lack `key` and they match
/// `when`, the run fails with a configuration error that names the rule, the key and `why`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Need {
    /// The key, as written in the options (`activation`).
    pub key: &'static str,
    /// A JSON Schema the plugin's options match when the rule reads `key`; `None` for always.
    pub when: Option<&'static str>,
    /// What reads it and what to do, after "which <rule> reads": `"the rule reads history after
    /// it: write the 40-hex id of the commit the policy starts after"`.
    pub why: &'static str,
}

/// The key of the rule's options schema that carries its [`Rule::NEEDS`], an extension keyword
/// that schema validation ignores: a list of `{ "key", "when", "why" }`.
pub const NEEDS_KEYWORD: &str = "x-gate-needs";

/// One rule. The type is built with [`Default`] for every run; its options arrive typed.
pub trait Rule: Default + 'static {
    /// The rule's options, read from configuration with `serde`; their JSON Schema, generated
    /// with `schemars`, is what the host validates the configuration against.
    type Options: DeserializeOwned + JsonSchema;

    /// The rule's name inside the plugin, lowercase kebab: `barrel-exports`.
    const NAME: &'static str;
    /// One line that says what the rule holds true.
    const TITLE: &'static str;
    /// The named parts the rule reports (`field-inventory#hidden`); empty when it has one.
    const ASPECTS: &'static [&'static str] = &[];
    const SCOPE: Scope = Scope::Project;
    /// The rule reads commits; only then does [`Run::git`] answer.
    const READS_HISTORY: bool = false;
    /// The languages whose files the rule reads (`["typescript"]`), so the host can skip a run
    /// with no such file.
    const LANGUAGES: &'static [&'static str] = &[];
    /// The keys of the plugin's options the rule reads that the plugin's schema leaves optional.
    const NEEDS: &'static [Need] = &[];

    /// Checks the run's tree and reports findings; `Err` only for an environment failure.
    ///
    /// # Errors
    /// An input the rule cannot work with (an unreadable options file, a malformed document the
    /// rule depends on), with a message that says what to do.
    fn check(&self, run: &mut Run<'_, Self::Options>) -> Result<()>;

    /// The declaration `describe` answers.
    #[must_use]
    fn info() -> RuleInfo {
        RuleInfo {
            name: Self::NAME.to_owned(),
            title: Self::TITLE.to_owned(),
            aspects: Self::ASPECTS
                .iter()
                .map(|aspect| (*aspect).to_owned())
                .collect(),
            scope: Self::SCOPE,
            reads_history: Self::READS_HISTORY,
            languages: Self::LANGUAGES
                .iter()
                .map(|language| (*language).to_owned())
                .collect(),
            options_schema: with_needs(&schema::<Self::Options>(), Self::NEEDS),
        }
    }
}

/// `schema` with `needs` under [`NEEDS_KEYWORD`]; `schema` itself when there are none. A `when`
/// that is not JSON is carried as a string, which the core reports as a broken plugin.
pub(crate) fn with_needs(schema: &str, needs: &[Need]) -> String {
    if needs.is_empty() {
        return schema.to_owned();
    }
    let Ok(serde_json::Value::Object(mut object)) = serde_json::from_str(schema) else {
        return schema.to_owned();
    };
    let list = needs
        .iter()
        .map(|need| {
            let when = need.when.map_or(serde_json::Value::Null, |when| {
                serde_json::from_str(when)
                    .unwrap_or_else(|_| serde_json::Value::String(when.to_owned()))
            });
            serde_json::json!({ "key": need.key, "when": when, "why": need.why })
        })
        .collect();
    object.insert(NEEDS_KEYWORD.to_owned(), serde_json::Value::Array(list));
    serde_json::Value::Object(object).to_string()
}

pub(crate) fn schema<T: JsonSchema>() -> String {
    serde_json::to_string(&schemars::schema_for!(T)).unwrap_or_else(|_| "{}".to_owned())
}

/// One run of one rule over one project (or the repository).
pub struct Run<'h, O> {
    pub(crate) host: &'h dyn Host,
    pub(crate) git: Option<Git<'h>>,
    pub(crate) report: Report,
    pub(crate) options: O,
    pub(crate) options_file: Option<String>,
    pub(crate) exceptions: Vec<String>,
    pub(crate) project: String,
    pub(crate) plugin_options: &'h serde_json::Value,
    pub(crate) languages: &'h serde_json::Value,
    pub(crate) norms: &'h [String],
}

impl<O: std::fmt::Debug> std::fmt::Debug for Run<'_, O> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Run")
            .field("project", &self.project)
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl<'h, O> Run<'h, O> {
    /// The tracked files this run may read.
    #[must_use]
    pub fn tree(&self) -> Tree<'h> {
        Tree::new(self.host)
    }

    /// The repository's history.
    ///
    /// # Errors
    /// When the rule does not declare [`Rule::READS_HISTORY`] or the host offers no history.
    pub fn git(&self) -> Result<Git<'h>> {
        self.git.ok_or_else(|| {
            Error::new("this rule reads git history but does not declare READS_HISTORY = true")
        })
    }

    /// Where findings go.
    pub fn report(&mut self) -> &mut Report {
        &mut self.report
    }

    /// The rule's options, validated by the host against their schema.
    #[must_use]
    pub fn options(&self) -> &O {
        &self.options
    }

    /// The tracked file the options came from, readable through [`Run::tree`].
    #[must_use]
    pub fn options_file(&self) -> Option<&str> {
        self.options_file.as_deref()
    }

    /// The targets of this rule's exceptions, for a rule whose result depends on them; the host
    /// applies exceptions to findings itself.
    #[must_use]
    pub fn exceptions(&self) -> &[String] {
        &self.exceptions
    }

    /// The project's root relative to the repository; empty for the repository.
    #[must_use]
    pub fn project(&self) -> &str {
        &self.project
    }

    /// The `[plugins.<name>]` options every rule of the plugin shares, already validated by the
    /// host against the schema of the `options` type [`crate::plugin!`] declares; `T` is that
    /// type.
    ///
    /// # Errors
    /// When they do not match `T`, naming the key.
    pub fn plugin_options<T: DeserializeOwned>(&self) -> Result<T> {
        T::deserialize(self.plugin_options)
            .map_err(|error| Error::config(format!("the [plugins] options do not match: {error}")))
    }

    /// The `[languages.<id>]` settings, when the configuration has them.
    ///
    /// # Errors
    /// When they do not match `T`.
    pub fn language<T: DeserializeOwned>(&self, id: &str) -> Result<Option<T>> {
        self.languages
            .get(id)
            .map(|value| {
                T::deserialize(value).map_err(|error| {
                    Error::config(format!(
                        "the [languages.{id}] settings do not match: {error}"
                    ))
                })
            })
            .transpose()
    }

    /// Every norm the configuration maps, for rules about rule documents.
    #[must_use]
    pub fn norms(&self) -> &[String] {
        self.norms
    }

    /// A diagnostic line for `--verbose`; never part of a finding.
    pub fn log(&self, message: &str) {
        self.host.log(message);
    }
}

/// A generator as `describe` declares it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratorInfo {
    /// The generator's name, unique in its plugin ([`Generator::NAME`]).
    pub name: String,
    /// One line on what it writes ([`Generator::TITLE`]).
    pub title: String,
    /// The JSON Schema of its options.
    pub options_schema: String,
    /// The project-relative globs the generator owns ([`Generator::OWNS`]).
    pub owns: Vec<String>,
}

/// A file a generator writes, at a path relative to the project root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedFile {
    /// The path, relative to the project root, below it; never a file the core owns (git's
    /// directory, `gate.toml`, `gate.lock`, `gate.exceptions.toml`, `bin/gate`, `.githooks`).
    pub path: String,
    /// The bytes to write.
    pub contents: Vec<u8>,
}

/// One generator of files (hooks, CI, agent adapters, documents).
pub trait Generator: Default + 'static {
    type Options: DeserializeOwned + JsonSchema;
    const NAME: &'static str;
    const TITLE: &'static str;
    /// The globs, relative to the project root, whose files the generator owns: `gate sync`
    /// deletes a tracked file under one that no generator produced, and `gate sync --check`
    /// reports it. Empty when the generator only writes the files it names. The core refuses a
    /// glob with a `.git` segment, one that matches everything (`*`, `**`), and one that covers a
    /// file the core owns.
    const OWNS: &'static [&'static str] = &[];

    /// Writes the generator's files into `run`; findings it reports surface as
    /// `gate/sync#<generator>`.
    ///
    /// # Errors
    /// An input the generator cannot work with.
    fn generate(&self, run: &mut Generate<'_, Self::Options>) -> Result<()>;

    /// The declaration `describe` answers.
    #[must_use]
    fn info() -> GeneratorInfo {
        GeneratorInfo {
            name: Self::NAME.to_owned(),
            title: Self::TITLE.to_owned(),
            options_schema: schema::<Self::Options>(),
            owns: Self::OWNS.iter().map(|glob| (*glob).to_owned()).collect(),
        }
    }
}

/// One run of one generator.
pub struct Generate<'h, O> {
    pub(crate) host: &'h dyn Host,
    pub(crate) report: Report,
    pub(crate) options: O,
    pub(crate) project: String,
    pub(crate) files: Vec<GeneratedFile>,
}

impl<O: std::fmt::Debug> std::fmt::Debug for Generate<'_, O> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Generate")
            .field("project", &self.project)
            .field("options", &self.options)
            .field("files", &self.files.len())
            .finish_non_exhaustive()
    }
}

impl<'h, O> Generate<'h, O> {
    /// The tracked files the generator may read.
    #[must_use]
    pub fn tree(&self) -> Tree<'h> {
        Tree::new(self.host)
    }

    /// Where findings go.
    pub fn report(&mut self) -> &mut Report {
        &mut self.report
    }

    #[must_use]
    pub fn options(&self) -> &O {
        &self.options
    }

    /// The project's root relative to the repository; empty for the repository.
    #[must_use]
    pub fn project(&self) -> &str {
        &self.project
    }

    /// Writes `contents` at `path`, relative to the project root like every path a plugin
    /// names; a later write of the same path replaces it.
    pub fn write(&mut self, path: impl Into<String>, contents: impl Into<Vec<u8>>) {
        let path = path.into();
        self.files.retain(|file| file.path != path);
        self.files.push(GeneratedFile {
            path,
            contents: contents.into(),
        });
    }
}

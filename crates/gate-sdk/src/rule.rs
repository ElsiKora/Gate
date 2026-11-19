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
    /// The rule reads the commit message being written ([`Rule::READS_MESSAGE`]).
    pub reads_message: bool,
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

/// One rule. The type is built with [`Default`] for every run; its options arrive typed. `S` is
/// the state the rules of one call share ([`CallState`]); a plugin without one keeps the default
/// `()` and writes `impl Rule for`.
pub trait Rule<S: CallState = ()>: Default + 'static {
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
    /// The rule reads the commit message being written (`gate check --commit-message`); only then
    /// does [`Git::message`] answer, and a `--commit-message` run runs only such rules. The message
    /// belongs to the repository: a rule that declares it has [`Scope::Repository`], or its plugin
    /// does not load.
    const READS_MESSAGE: bool = false;
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
    fn check(&self, run: &mut Run<'_, Self::Options, S>) -> Result<()>;

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
            reads_message: Self::READS_MESSAGE,
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

/// What one `check` call is, alike for every rule of it: the host's tree, the project, the
/// plugin's options and the language settings. [`CallState::own`] builds the call's state from it;
/// a rule sees the same through its [`Run`].
#[derive(Clone, Copy)]
pub struct Call<'h> {
    pub(crate) host: &'h dyn Host,
    pub(crate) project: &'h str,
    pub(crate) plugin_options: &'h serde_json::Value,
    pub(crate) languages: &'h serde_json::Value,
    pub(crate) norms: &'h [String],
}

impl std::fmt::Debug for Call<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Call")
            .field("project", &self.project)
            .finish_non_exhaustive()
    }
}

impl<'h> Call<'h> {
    /// The tracked files the call may read.
    #[must_use]
    pub fn tree(&self) -> Tree<'h> {
        Tree::new(self.host)
    }

    /// The project's root relative to the repository; empty for the repository.
    #[must_use]
    pub fn project(&self) -> &'h str {
        self.project
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
    pub fn norms(&self) -> &'h [String] {
        self.norms
    }

    /// A diagnostic line for `--verbose`; never part of a finding.
    pub fn log(&self, message: &str) {
        self.host.log(message);
    }
}

/// The state the rules of one `check` call share, which a plugin declares in [`crate::plugin!`]
/// (`call: <type>`): work every rule would otherwise do again on the same tree, such as reading
/// and parsing its files. The call builds the [`Self::Owner`] from the [`Call`] once, before its
/// first rule, and the [`Self::State`] from the owner; a rule reaches the state through
/// [`Run::with_call`]. Both are dropped when the call ends, so no call sees another's state.
/// What the state holds must depend only on the call, never on one rule's options.
///
/// The state may borrow the owner, which is how a parsed file can borrow the arena it was parsed
/// into. A plugin with nothing to share keeps the default `()`.
///
/// ```
/// use std::cell::RefCell;
/// use std::collections::HashMap;
///
/// use gate_sdk::{Call, CallState, Location, NoOptions, Rule, Run, Tree};
///
/// /// The text of each file, read once per call whichever rule asks first.
/// pub struct Texts;
///
/// pub struct Read<'h> {
///     tree: Tree<'h>,
///     texts: RefCell<HashMap<String, Option<String>>>,
/// }
///
/// impl Read<'_> {
///     fn text(&self, path: &str) -> Option<String> {
///         let mut texts = self.texts.borrow_mut();
///         texts.entry(path.to_owned()).or_insert_with(|| self.tree.text(path)).clone()
///     }
/// }
///
/// impl CallState for Texts {
///     type Owner<'h> = Read<'h>;
///     type State<'o> = &'o Read<'o>;
///
///     fn own<'h>(call: &Call<'h>) -> Read<'h> {
///         Read { tree: call.tree(), texts: RefCell::default() }
///     }
///
///     fn state<'o>(owner: &'o Read<'_>) -> &'o Read<'o> {
///         owner
///     }
/// }
///
/// #[derive(Default)]
/// pub struct NoTabs;
///
/// impl Rule<Texts> for NoTabs {
///     type Options = NoOptions;
///     const NAME: &'static str = "no-tabs";
///     const TITLE: &'static str = "Tracked text files are indented without tabs";
///
///     fn check(&self, run: &mut Run<'_, NoOptions, Texts>) -> gate_sdk::Result<()> {
///         run.with_call(|run, read| {
///             for path in run.tree().paths() {
///                 if read.text(&path).is_some_and(|text| text.contains('\t')) {
///                     run.report().finding("", Location::file(&path), "a tab is left in");
///                 }
///             }
///             Ok(())
///         })
///     }
/// }
///
/// gate_sdk::plugin! {
///     name: "texts",
///     version: "0.1.0",
///     options: NoOptions,
///     call: Texts,
///     rules: [NoTabs],
///     generators: [],
///     presets: [],
/// }
/// ```
pub trait CallState: 'static {
    /// What the call owns, built from the [`Call`] before its first rule runs.
    type Owner<'h>;
    /// What the rules see, built from the owner and free to borrow it.
    type State<'o>;

    /// The owner of one call.
    fn own<'h>(call: &Call<'h>) -> Self::Owner<'h>;

    /// The state of one call, borrowing its owner.
    fn state<'o>(owner: &'o Self::Owner<'_>) -> Self::State<'o>;
}

/// No state: the rules of the call share nothing but the call itself.
impl CallState for () {
    type Owner<'h> = ();
    type State<'o> = ();

    fn own<'h>(_: &Call<'h>) -> Self::Owner<'h> {}

    fn state<'o>((): &'o Self::Owner<'_>) -> Self::State<'o> {}
}

/// The state of one call with the borrow of its owner hidden, so a [`Run`] holds it by reference
/// and hands it to a rule inside [`Run::with_call`].
pub(crate) trait Enter<S: CallState> {
    fn enter(&self, body: &mut dyn for<'a, 'o> FnMut(&'a S::State<'o>));
}

/// The state of one call, as the call holds it until its last rule ends.
pub(crate) struct Entered<'o, S: CallState>(pub(crate) S::State<'o>);

impl<S: CallState> Enter<S> for Entered<'_, S> {
    fn enter(&self, body: &mut dyn for<'a, 'o> FnMut(&'a S::State<'o>)) {
        body(&self.0);
    }
}

/// One run of one rule over one project (or the repository). `S` is the plugin's
/// [`CallState`]; `()` for a plugin without one.
pub struct Run<'h, O, S: CallState = ()> {
    pub(crate) call: Call<'h>,
    pub(crate) git: Option<Git<'h>>,
    pub(crate) report: Report,
    pub(crate) options: O,
    pub(crate) options_file: Option<String>,
    pub(crate) exceptions: Vec<String>,
    pub(crate) state: &'h (dyn Enter<S> + 'h),
}

impl<O: std::fmt::Debug, S: CallState> std::fmt::Debug for Run<'_, O, S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Run")
            .field("project", &self.call.project)
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl<'h, O, S: CallState> Run<'h, O, S> {
    /// The tracked files this run may read.
    #[must_use]
    pub fn tree(&self) -> Tree<'h> {
        self.call.tree()
    }

    /// Runs `body` with this run and the state every rule of the call shares ([`CallState`]),
    /// and answers what `body` answers. The state is borrowed only inside `body`.
    ///
    /// # Errors
    /// What `body` answers.
    pub fn with_call<T>(
        &mut self,
        body: impl for<'a, 'o> FnOnce(&mut Self, &'a S::State<'o>) -> Result<T>,
    ) -> Result<T> {
        let state = self.state;
        let mut body = Some(body);
        let mut answer = None;
        state.enter(&mut |shared| {
            if let Some(body) = body.take() {
                answer = Some(body(self, shared));
            }
        });
        answer.unwrap_or_else(|| {
            Err(Error::new(
                "the call's state was not handed to the rule: a bug in gate-sdk, not in the \
                 configuration; report it with the plugin's name and version",
            ))
        })
    }

    /// The repository's history, and the commit message being written.
    ///
    /// # Errors
    /// When the rule declares neither [`Rule::READS_HISTORY`] nor [`Rule::READS_MESSAGE`], or the
    /// host offers no history.
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
        self.call.project
    }

    /// The `[plugins.<name>]` options every rule of the plugin shares, already validated by the
    /// host against the schema of the `options` type [`crate::plugin!`] declares; `T` is that
    /// type.
    ///
    /// # Errors
    /// When they do not match `T`, naming the key.
    pub fn plugin_options<T: DeserializeOwned>(&self) -> Result<T> {
        self.call.plugin_options()
    }

    /// The `[languages.<id>]` settings, when the configuration has them.
    ///
    /// # Errors
    /// When they do not match `T`.
    pub fn language<T: DeserializeOwned>(&self, id: &str) -> Result<Option<T>> {
        self.call.language(id)
    }

    /// Every norm the configuration maps, for rules about rule documents.
    #[must_use]
    pub fn norms(&self) -> &[String] {
        self.call.norms
    }

    /// A diagnostic line for `--verbose`; never part of a finding.
    pub fn log(&self, message: &str) {
        self.call.log(message);
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
    /// A seed: the core writes it only when nothing exists at its path, never compares it, and
    /// never deletes it; once written it belongs to the project ([`Generate::seed`]).
    pub seed: bool,
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
    pub(crate) plugin_options: serde_json::Value,
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

    /// The `[plugins.<name>]` options the plugin's rules share, as [`Call::plugin_options`] gives
    /// them to a rule; `T` is the `options` type [`crate::plugin!`] declares.
    ///
    /// # Errors
    /// When they do not match `T`, naming the key.
    pub fn plugin_options<T: DeserializeOwned>(&self) -> Result<T> {
        T::deserialize(&self.plugin_options)
            .map_err(|error| Error::config(format!("the [plugins] options do not match: {error}")))
    }

    /// Writes `contents` at `path`, relative to the project root like every path a plugin
    /// names; a later write or seed of the same path replaces it.
    pub fn write(&mut self, path: impl Into<String>, contents: impl Into<Vec<u8>>) {
        self.push(path.into(), contents.into(), false);
    }

    /// Seeds `contents` at `path`: `gate sync` writes it only when nothing exists at the path,
    /// and `gate sync --check` never reports it, so a person's edit is never overwritten. A seed
    /// under a glob the generator owns ([`Generator::OWNS`]) fails the call.
    pub fn seed(&mut self, path: impl Into<String>, contents: impl Into<Vec<u8>>) {
        self.push(path.into(), contents.into(), true);
    }

    fn push(&mut self, path: String, contents: Vec<u8>, seed: bool) {
        self.files.retain(|file| file.path != path);
        self.files.push(GeneratedFile {
            path,
            contents,
            seed,
        });
    }
}

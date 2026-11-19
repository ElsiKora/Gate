//! Write Gate rules and generators in Rust and build them as one WebAssembly component.
//!
//! A plugin is a `cdylib` crate built for `wasm32-wasip2`. It declares its rules and generators
//! with [`plugin!`]; each rule is a type that implements [`Rule`]:
//!
//! ```ignore
//! use gate_sdk::{Location, Rule, Run, Scope};
//!
//! #[derive(Default)]
//! pub struct NoTodo;
//!
//! #[derive(Default, serde::Deserialize, schemars::JsonSchema)]
//! pub struct Options {
//!     /// The marker to look for.
//!     #[serde(default = "marker")]
//!     marker: String,
//! }
//!
//! impl Rule for NoTodo {
//!     type Options = Options;
//!     const NAME: &'static str = "no-todo";
//!     const TITLE: &'static str = "Tracked text files carry no TODO marker";
//!
//!     fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
//!         for path in run.tree().paths() {
//!             let Some(text) = run.tree().text(&path) else { continue };
//!             for (index, line) in text.lines().enumerate() {
//!                 if line.contains(&run.options().marker) {
//!                     run.report()
//!                         .finding("", Location::line(&path, index + 1), "a TODO marker is left in")
//!                         .help("finish the work or open an issue for it");
//!                 }
//!             }
//!         }
//!         Ok(())
//!     }
//! }
//!
//! gate_sdk::plugin! {
//!     name: "hello",
//!     version: env!("CARGO_PKG_VERSION"),
//!     options: gate_sdk::NoOptions,
//!     rules: [NoTodo],
//!     generators: [],
//!     presets: [],
//! }
//! ```
//!
//! What a rule may see is decided by the host, not by the plugin: [`Tree`] lists and reads only the
//! tracked files the call is granted, [`Git`] answers only for rules that declare
//! [`Rule::READS_HISTORY`], and nothing else exists (no file system, network, clock or
//! environment). The level, norm and exceptions of a finding come from configuration.
//!
//! Work the rules of one call would each do again on the same tree, such as parsing its files,
//! goes into the plugin's [`CallState`]: built once per call, shared by its rules through
//! [`Run::with_call`], and dropped when the call ends.
//!
//! The same rule code runs natively against a fixture directory through a [`Host`] implementation,
//! which is how `gate-test` tests rules.

mod git;
mod host;
mod plugin;
mod report;
mod rule;
mod tree;

#[doc(hidden)]
#[expect(
    clippy::same_length_and_capacity,
    clippy::too_many_arguments,
    reason = "the code wit-bindgen generates for the imports and exports of wit/gate.wit"
)]
pub mod bindings;

pub use self::git::{Commit, CommitRange, Git, SyncState};
pub use self::host::{GitHost, Host, TreeHost};
pub use self::plugin::{
    CheckRequest, GenerateOutput, GeneratorEntry, NoOptions, PluginInfo, PluginSpec, Preset,
    PresetInfo, RuleEntry, RuleRun, check, describe, generate,
};
pub use self::report::{FileLocation, Finding, Location, Report, Span};
pub use self::rule::{
    Call, CallState, Generate, GeneratedFile, Generator, GeneratorInfo, NEEDS_KEYWORD, Need, Rule,
    RuleInfo, Run, Scope,
};
pub use self::tree::Tree;

/// Re-exported so a plugin derives [`schemars::JsonSchema`] and the [`serde`] traits for its
/// options with the versions the SDK was built with.
pub use {schemars, serde, serde_json};

/// Why a rule or generator failed: the call fails, and the host names the plugin and this
/// message. [`Error::config`] is an error in the configuration the call was given, which fails the
/// run as a configuration error; [`Error::new`] is an environment failure. A violation is never an
/// error; it is a [`Finding`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    configuration: bool,
    message: String,
}

impl Error {
    /// An environment failure: a read, a parse or a git call that failed, with a message that says
    /// what is wrong and what to do.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            configuration: false,
            message: message.into(),
        }
    }

    /// A configuration error: an option or setting the call was given cannot be used. The message
    /// names the option and says what to write instead.
    #[must_use]
    pub fn config(message: impl Into<String>) -> Self {
        Self {
            configuration: true,
            message: message.into(),
        }
    }

    /// Whether this is a configuration error ([`Error::config`]).
    #[must_use]
    pub fn is_config(&self) -> bool {
        self.configuration
    }

    /// The message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The same kind of error with another message, for a caller that adds context.
    #[must_use]
    pub fn with_message(self, message: impl Into<String>) -> Self {
        Self {
            configuration: self.configuration,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

/// The result of a rule or generator.
pub type Result<T> = std::result::Result<T, Error>;

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
    Generate, GeneratedFile, Generator, GeneratorInfo, NEEDS_KEYWORD, Need, Rule, RuleInfo, Run,
    Scope,
};
pub use self::tree::Tree;

/// Re-exported so a plugin derives [`schemars::JsonSchema`] and the [`serde`] traits for its
/// options with the versions the SDK was built with.
pub use {schemars, serde, serde_json};

/// An environment failure: the call fails, and the host names the plugin and this message. A
/// violation is never an error; it is a [`Finding`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error(String);

impl Error {
    /// An error with a message that says what is wrong and what to do.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// The message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// The result of a rule or generator.
pub type Result<T> = std::result::Result<T, Error>;

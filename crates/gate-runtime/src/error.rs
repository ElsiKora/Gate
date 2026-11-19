//! Runtime errors; each names the plugin.

/// What went wrong.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    /// The runtime could not be set up (engine, cache directory, linker).
    Setup,
    /// The component could not be compiled or instantiated, or its declaration is invalid.
    Load,
    /// The call asked for something the plugin does not declare.
    Request,
    /// The plugin trapped (a panic, an out-of-bounds access, a host failure, a refused git call).
    Trap,
    /// The call hit the fuel limit: it ran too long.
    OutOfFuel,
    /// The call ran past the deadline: wall-clock time, which a loaded machine may also cause.
    Deadline,
    /// The call hit the memory limit (its linear memories and tables together) or the read-set
    /// limit.
    OutOfMemory,
    /// The plugin answered `environment`: an environment failure it reports, which may pass by the
    /// next call.
    Reported,
    /// The plugin answered `configuration`: the options or settings the call was given cannot be
    /// used. It recurs for the same configuration.
    Configuration,
    /// The plugin broke the interface: a finding outside the repository, of a rule the call did not run or an aspect the rule does
    /// not declare, at a commit that is not a full id; a git call with a revision where a full
    /// commit id belongs; a generated file outside its project or one the core owns.
    Plugin,
}

/// A runtime error: the plugin, the kind, and a message that says what to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub(crate) plugin: String,
    pub(crate) kind: ErrorKind,
    pub(crate) message: String,
}

impl Error {
    pub(crate) fn new(plugin: &str, kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            plugin: plugin.to_owned(),
            kind,
            message: message.into(),
        }
    }

    /// The plugin's name; empty for a setup error.
    #[must_use]
    pub fn plugin(&self) -> &str {
        &self.plugin
    }

    /// What went wrong.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// What is wrong and what to do, without the plugin's name (the `Display` form adds it).
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.plugin.is_empty() {
            write!(formatter, "plugin runtime: {}", self.message)
        } else {
            write!(formatter, "plugin {}: {}", self.plugin, self.message)
        }
    }
}

impl std::error::Error for Error {}

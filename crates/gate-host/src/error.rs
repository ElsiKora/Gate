//! The one error type of the host.

use std::fmt;

/// What kind of failure an [`Error`] is. Both kinds end a run with exit code 2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    /// A `gate.toml` (or a file it names) is wrong: the message names the file, the key and what is
    /// expected.
    Config,
    /// The environment failed: git is missing, a revision does not exist, a file cannot be read.
    Environment,
}

/// A failure that stops a run. The message says what is wrong and what to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    kind: ErrorKind,
    message: String,
}

impl Error {
    /// A configuration error.
    #[must_use]
    pub fn config(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Config,
            message: message.into(),
        }
    }

    /// An environment error.
    #[must_use]
    pub fn environment(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Environment,
            message: message.into(),
        }
    }

    /// The kind of failure.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// The message, without the kind.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            ErrorKind::Config => "configuration error",
            ErrorKind::Environment => "environment error",
        };
        write!(formatter, "{kind}: {}", self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::environment(error.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::environment(format!("cannot write JSON: {error}"))
    }
}

/// The result type of the host.
pub type Result<T, E = Error> = std::result::Result<T, E>;

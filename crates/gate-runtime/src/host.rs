//! What the runtime needs from the core: the snapshot of tracked files and, for history rules, git.
//! The core implements these traits over its own snapshot and git modules.

use gate_sdk::{Commit, CommitRange, SyncState};

/// A failure of the core while it answers a plugin; the call fails with this message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostError(String);

impl HostError {
    /// A failure with a message that says what is wrong.
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

impl std::fmt::Display for HostError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for HostError {}

/// The bytes of a tracked file and the id of its contents (a git blob id), recorded in the read set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Blob {
    pub id: String,
    pub bytes: Vec<u8>,
}

/// The snapshot of one run: tracked files only, repository-relative slash-separated paths.
pub trait Tree: Send + Sync {
    /// Every tracked path, sorted.
    fn paths(&self) -> &[String];

    /// The contents of a tracked path; `None` when it is not tracked.
    ///
    /// # Errors
    /// The file is tracked but cannot be read.
    fn read(&self, path: &str) -> Result<Option<Blob>, HostError>;

    /// The paths the run's change selection names, when the run has one.
    fn changed(&self) -> Option<&[String]>;
}

/// The repository's history, for calls whose rules declare `reads-history`. Paths are
/// repository-relative.
#[expect(
    clippy::missing_errors_doc,
    reason = "every method fails only when git fails"
)]
pub trait History: Send + Sync {
    fn head(&self) -> Result<Option<String>, HostError>;
    fn branch(&self) -> Result<Option<String>, HostError>;
    /// The run's range under `--rev` or `--push`.
    fn range(&self) -> Result<Option<CommitRange>, HostError>;
    /// The commit a ref or revision names.
    fn resolve(&self, rev: &str) -> Result<Option<String>, HostError>;
    /// The tree id of a commit.
    fn tree(&self, commit: &str) -> Result<String, HostError>;
    /// The tree of git's merge of two commits; `None` on conflict.
    fn merge_tree(&self, first: &str, second: &str) -> Result<Option<String>, HostError>;
    /// The commit ids the revisions of a `log` range name (`git rev-parse <range>`), recorded in
    /// the read set so a moved ref re-runs the call.
    fn revisions(&self, range: &str) -> Result<Vec<String>, HostError>;
    fn log(&self, range: &str) -> Result<Vec<Commit>, HostError>;
    fn changed_paths(&self, parent: &str, commit: &str) -> Result<Vec<String>, HostError>;
    fn read_at(&self, commit: &str, path: &str) -> Result<Option<Vec<u8>>, HostError>;
    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool, HostError>;
    /// What `gate sync` produces at a commit and which of it differs, repository-relative.
    fn sync_state(&self, commit: &str) -> Result<SyncState, HostError>;
    /// The commit message under review (`--commit-message`), as its file holds it.
    fn message(&self) -> Result<Option<String>, HostError> {
        Ok(None)
    }
    /// Whether a git hook started the run (`--hook`).
    fn hook(&self) -> Result<bool, HostError> {
        Ok(false)
    }
}

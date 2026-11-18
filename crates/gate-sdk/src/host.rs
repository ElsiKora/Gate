//! What the SDK needs from the host. In a component the host is the Gate core, reached through the
//! `tree`, `git` and `log` imports of `wit/gate.wit`; natively (`gate-test`) it is a fixture tree.

use crate::git::{Commit, CommitRange, SyncState};

/// The read-only snapshot of one call, within its grant.
pub trait TreeHost {
    /// The tracked paths under `prefix` (empty for all), sorted, relative to the call's root.
    fn list(&self, prefix: &str) -> Vec<String>;
    /// The bytes of a tracked path; `None` when it is not tracked or not granted.
    fn read(&self, path: &str) -> Option<Vec<u8>>;
    /// The paths the run's change selection names, when the run has one.
    fn changed(&self) -> Option<Vec<String>>;
}

/// Git history; offered only to rules that declare `reads-history`.
pub trait GitHost {
    /// The commit the snapshot stands on.
    fn head(&self) -> Option<String>;
    /// The checked-out branch; `None` when detached.
    fn branch(&self) -> Option<String>;
    /// The run's range under `--rev` or `--push`.
    fn range(&self) -> Option<CommitRange>;
    /// The commit a ref or revision names.
    fn resolve(&self, rev: &str) -> Option<String>;
    /// The tree id of `commit`.
    fn tree(&self, commit: &str) -> String;
    /// The tree of git's merge of two commits; `None` on conflict.
    fn merge_tree(&self, first: &str, second: &str) -> Option<String>;
    /// The commits of a revision range, newest first.
    fn log(&self, range: &str) -> Vec<Commit>;
    /// The paths `commit` changed against its parent `parent`.
    fn changed_paths(&self, parent: &str, commit: &str) -> Vec<String>;
    /// The bytes of `path` at `commit`; `None` when absent there.
    fn read_at(&self, commit: &str, path: &str) -> Option<Vec<u8>>;
    /// Whether `ancestor` is an ancestor of `descendant`.
    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool;
    /// What `gate sync` produces at `commit`, and which of it differs.
    fn sync_state(&self, commit: &str) -> SyncState;
    /// The commit message under review (`gate check --commit-message <file>`), as the file holds
    /// it; `None` for a run without one. A native fixture without a message keeps the default.
    fn message(&self) -> Option<String> {
        None
    }
    /// Whether a git hook started the run (`gate check --hook`); the default is `false`.
    fn hook(&self) -> bool {
        false
    }
}

/// Everything a call may reach.
pub trait Host: TreeHost {
    /// The git interface, or `None` when the host offers none (a native fixture without history).
    fn git(&self) -> Option<&dyn GitHost>;
    /// A diagnostic line for `--verbose`; never part of a finding.
    fn log(&self, message: &str);
}

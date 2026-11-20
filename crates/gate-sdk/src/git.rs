//! Git history, for rules that declare [`crate::Rule::READS_HISTORY`]. The commit message being
//! written is [`crate::Run::message`], for rules that declare [`crate::Rule::READS_MESSAGE`].

use crate::host::GitHost;

/// One commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Commit {
    /// The full commit id.
    pub id: String,
    /// The full ids of the parents, first parent first; empty for a root commit.
    pub parents: Vec<String>,
    /// The raw full message, as `git log --format=%B` prints it.
    pub message: String,
    /// Strict ISO 8601.
    pub author_date: String,
}

impl Commit {
    /// The first line of the message.
    #[must_use]
    pub fn subject(&self) -> &str {
        self.message.lines().next().unwrap_or_default()
    }

    /// The message after its first line, without the blank lines that separate it from the
    /// subject and without trailing whitespace; empty when the message is one line.
    #[must_use]
    pub fn body(&self) -> &str {
        self.message.split_once('\n').map_or("", |(_, rest)| {
            rest.trim_start_matches(['\n', '\r']).trim_end()
        })
    }
}

/// The run's range under `--rev` or `--push`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitRange {
    /// The base commit; `None` for a new branch.
    pub base: Option<String>,
    /// The head commit.
    pub head: String,
    /// The remote ref the push updates (`refs/heads/main`) under `--push`; `None` under `--rev`.
    /// A push of several refs runs the history rules once per ref, each with its own range.
    pub target_ref: Option<String>,
}

/// What `gate sync` produces at a commit, and which of it differs; [`Git::sync_state`] answers it.
///
/// Paths are relative to the call's root, in the plugin's form, sorted, and only paths of the
/// grant. Every path of `drift` is in `produced`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SyncState {
    /// Every path `gate sync` writes or owns at the commit: the outputs of its generators, the
    /// tracked files under a glob a generator owns, and the core's hook and CI files. A tracked
    /// path that is not here is written by hand.
    pub produced: Vec<String>,
    /// The produced paths that `gate sync --check` reports at the commit: an output that is
    /// missing, differs or is not executable there, and a tracked file under an owned glob that no
    /// generator writes (stale). Empty at a commit whose `gate.toml` is not a configuration of
    /// this core (before the repository moved to Gate): files another tool wrote cannot be judged by
    /// this core's generators, so there only `produced` tells a generated path from a hand-written
    /// one.
    pub drift: Vec<String>,
}

/// The repository's history as the host answers it.
///
/// `tree`, `merge_tree`, `changed_paths`, `read_at` and `is_ancestor` take full commit ids (40 or
/// 64 lowercase hex digits): resolve a ref or revision with [`Git::resolve`] first; anything else
/// fails the call. Paths are relative to the call's root, as in [`crate::Tree`], and a path the call
/// is not granted reads as absent.
///
/// Each answer is recorded in the call's read set. An answer about commit ids records nothing more
/// to re-check, so a cached result that asked only about commit ids stays valid while the commits
/// exist; an answer that goes through a ref (`head`, `branch`, `range`, `resolve`, `log` of a
/// range) records the commits the ref named, and the call runs again when the ref moves.
#[derive(Clone, Copy)]
pub struct Git<'h> {
    host: &'h dyn GitHost,
}

impl std::fmt::Debug for Git<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Git").finish_non_exhaustive()
    }
}

impl<'h> Git<'h> {
    /// Git over a host.
    #[must_use]
    pub fn new(host: &'h dyn GitHost) -> Self {
        Self { host }
    }

    /// The commit the snapshot stands on; `None` in a repository without commits.
    #[must_use]
    pub fn head(&self) -> Option<String> {
        self.host.head()
    }

    /// The checked-out branch; `None` when detached.
    #[must_use]
    pub fn branch(&self) -> Option<String> {
        self.host.branch()
    }

    /// The run's range under `--rev` or `--push`; `None` for other runs.
    #[must_use]
    pub fn range(&self) -> Option<CommitRange> {
        self.host.range()
    }

    /// The commit a ref or revision names (`rev-parse --verify <rev>^{commit}`); `None` when it
    /// names no commit.
    #[must_use]
    pub fn resolve(&self, rev: &str) -> Option<String> {
        self.host.resolve(rev)
    }

    /// The tree id of `commit`, a full commit id.
    #[must_use]
    pub fn tree(&self, commit: &str) -> String {
        self.host.tree(commit)
    }

    /// The tree of git's merge of the commits `first` and `second` (`git merge-tree
    /// --write-tree`), both full ids; `None` when the merge conflicts.
    #[must_use]
    pub fn merge_tree(&self, first: &str, second: &str) -> Option<String> {
        self.host.merge_tree(first, second)
    }

    /// The commits of a revision range (`a..b`, or one revision for its ancestry), newest first.
    #[must_use]
    pub fn log(&self, range: &str) -> Vec<Commit> {
        self.host.log(range)
    }

    /// The paths `commit` changed against its parent `parent` (both full ids) that the call is
    /// granted, relative to the call's root.
    #[must_use]
    pub fn changed_paths(&self, parent: &str, commit: &str) -> Vec<String> {
        self.host.changed_paths(parent, commit)
    }

    /// The bytes of `path` (relative to the call's root) at `commit` (a full id); `None` when it is
    /// absent there or the call is not granted it.
    #[must_use]
    pub fn read_at(&self, commit: &str, path: &str) -> Option<Vec<u8>> {
        self.host.read_at(commit, path)
    }

    /// Whether the commit `ancestor` is an ancestor of (or equal to) the commit `descendant`, both
    /// full ids.
    #[must_use]
    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        self.host.is_ancestor(ancestor, descendant)
    }

    /// What `gate sync` produces at `commit`, a full id, and which of it differs: see
    /// [`SyncState`]. The generators run on the commit's tree with the commit's
    /// configuration and the plugins its `gate.lock` pins; when that `gate.toml` does not parse or
    /// validate in this core (before the repository moved to Gate), with the run's own configuration
    /// and plugins, and then `drift` is empty. A generator's error does not fail the call: the
    /// paths it could not produce are in the answer (a generator that fails or whose output
    /// `gate sync` would refuse, and a pinned plugin that does not load under this core, whose
    /// generators answer the paths they write under the run's configuration while the CI workflow
    /// answers as drifting). A pinned plugin that cannot be fetched or does not match its pin, a
    /// failure of git, and a failure that may pass by the next run (the machine, a time limit, a
    /// trap while a plugin loads) fail the call as an environment error rather than answer. Both
    /// lists are empty when the commit has no `gate.toml`.
    #[must_use]
    pub fn sync_state(&self, commit: &str) -> SyncState {
        self.host.sync_state(commit)
    }

    /// The commit message under review, for [`crate::Run::message`].
    pub(crate) fn message(&self) -> Option<String> {
        self.host.message()
    }

    /// Whether a git hook started the run: the generated hooks pass `gate check --hook`, and a run
    /// by hand or in CI does not. A rule that applies only while a developer works, such as a
    /// branch-name policy that refuses `main`, asks this.
    #[must_use]
    pub fn hook(&self) -> bool {
        self.host.hook()
    }
}

#[cfg(test)]
mod tests {
    use super::Commit;

    fn commit(message: &str) -> Commit {
        Commit {
            id: "c".to_owned(),
            parents: Vec::new(),
            message: message.to_owned(),
            author_date: String::new(),
        }
    }

    #[test]
    fn subject_and_body_come_from_the_raw_message() {
        let full = commit("fix(host): one\n\nwhy it changed\nand more\n");
        assert_eq!(full.subject(), "fix(host): one");
        assert_eq!(full.body(), "why it changed\nand more");
        let short = commit("chore: one line\n");
        assert_eq!(short.subject(), "chore: one line");
        assert_eq!(short.body(), "");
        assert_eq!(commit("").subject(), "");
    }
}

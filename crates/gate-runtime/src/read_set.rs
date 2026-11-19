//! What one call listed, read and asked git: the inputs its result depends on.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use gate_sdk::{CommitRange, SyncState};
use sha2::{Digest as _, Sha256};

use crate::host::{History, HostError};

/// One question a call asked git.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GitQuery {
    Head,
    Branch,
    Range,
    Resolve {
        rev: String,
    },
    Tree {
        commit: String,
    },
    MergeTree {
        first: String,
        second: String,
    },
    Log {
        range: String,
    },
    ChangedPaths {
        parent: String,
        commit: String,
    },
    ReadAt {
        commit: String,
        path: String,
    },
    IsAncestor {
        ancestor: String,
        descendant: String,
    },
    SyncState {
        commit: String,
    },
    Message,
    Hook,
}

impl GitQuery {
    /// The bytes of the strings the question holds.
    pub(crate) fn bytes(&self) -> usize {
        match self {
            Self::Head | Self::Branch | Self::Range | Self::Message | Self::Hook => 0,
            Self::Resolve { rev: one }
            | Self::Tree { commit: one }
            | Self::Log { range: one }
            | Self::SyncState { commit: one } => one.len(),
            Self::MergeTree {
                first: one,
                second: two,
            }
            | Self::ChangedPaths {
                parent: one,
                commit: two,
            }
            | Self::ReadAt {
                commit: one,
                path: two,
            }
            | Self::IsAncestor {
                ancestor: one,
                descendant: two,
            } => one.len() + two.len(),
        }
    }
}

/// The recorded inputs of a call. With the grant, rules and options of the call, they decide its
/// result: a plugin reaches nothing else.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReadSet {
    /// The repository prefixes the call's listings covered, one per granted path a listing reached:
    /// `apps/api/` for everything under the root `apps/api`, `docs/ledger` for all of the granted
    /// path `docs/ledger`, `apps/api/src/` for a listing of `src/` from that root. A prefix that
    /// reaches no granted path lists nothing and is not recorded.
    pub listed: BTreeSet<String>,
    /// The repository paths the call read, with the blob id read; `None` for a granted path that is
    /// not tracked.
    pub read: BTreeMap<String, Option<String>>,
    /// Whether the call asked for the run's change selection.
    pub changed: bool,
    /// What the call asked git, each with the commit ids (or, for `branch`, the name; for `range`,
    /// base, head and the pushed ref; for `message`, the message; for `hook`, `true` or `false`) its answer depended on: an answer that goes through a ref (`head`, `branch`, `range`, `resolve`, `log`
    /// of a range) records what the ref named, so a moved ref re-runs the call. An answer about
    /// commit ids never changes and records nothing.
    pub git: BTreeMap<GitQuery, Vec<String>>,
}

impl ReadSet {
    /// The number of entries: listed prefixes, read paths and git questions.
    pub(crate) fn entries(&self) -> usize {
        self.listed.len() + self.read.len() + self.git.len()
    }

    /// Adds what `other` recorded.
    pub fn merge(&mut self, other: Self) {
        self.listed.extend(other.listed);
        self.read.extend(other.read);
        self.changed |= other.changed;
        self.git.extend(other.git);
    }
}

/// What a call records for a `range` answer: base, head and the pushed ref, those present.
pub(crate) fn range_ids(range: Option<&CommitRange>) -> Vec<String> {
    range
        .iter()
        .flat_map(|range| {
            range
                .base
                .iter()
                .chain([&range.head])
                .chain(range.target_ref.iter())
        })
        .cloned()
        .collect()
}

/// What a call records for a `sync-state` answer: one digest of both path lists. An answer at a
/// commit whose own configuration this core does not read depends on the run's configuration, so
/// the result cache asks again and compares the digest.
pub(crate) fn sync_state_digest(state: &SyncState) -> String {
    let mut hasher = Sha256::new();
    for path in &state.produced {
        hasher.update(path.as_bytes());
        hasher.update([0]);
    }
    hasher.update([0]);
    for path in &state.drift {
        hasher.update(path.as_bytes());
        hasher.update([0]);
    }
    hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

/// What a call records for an `is-ancestor` answer (see [`GitQuery::depends_on`]).
#[must_use]
pub fn ancestry_record(holds: bool) -> Vec<String> {
    if holds {
        Vec::new()
    } else {
        vec!["no".to_owned()]
    }
}

impl GitQuery {
    /// Asks `history` this question again and answers what a call records for it: for a question
    /// that goes through a ref, the commit ids (or branch name) the ref names now; for a
    /// `sync-state`, the digest of its answer; for `is-ancestor`, nothing when it holds and `no`
    /// when it does not, since a commit the clone lacks answers no until a fetch brings it; for
    /// another question about commit ids, nothing, since its answer never changes. A stored result
    /// is reused only when every question answers as it was recorded.
    ///
    /// # Errors
    /// Fails when the core cannot answer.
    pub fn depends_on(&self, history: &dyn History) -> Result<Vec<String>, HostError> {
        Ok(match self {
            Self::Head => history.head()?.into_iter().collect(),
            Self::Branch => history.branch()?.into_iter().collect(),
            Self::Range => range_ids(history.range()?.as_ref()),
            Self::Resolve { rev } => history.resolve(rev)?.into_iter().collect(),
            Self::Log { range } => history.revisions(range)?,
            Self::SyncState { commit } => vec![sync_state_digest(&history.sync_state(commit)?)],
            Self::Message => history.message()?.into_iter().collect(),
            Self::Hook => vec![history.hook()?.to_string()],
            Self::IsAncestor {
                ancestor,
                descendant,
            } => ancestry_record(history.is_ancestor(ancestor, descendant)?),
            Self::Tree { .. }
            | Self::MergeTree { .. }
            | Self::ChangedPaths { .. }
            | Self::ReadAt { .. } => Vec::new(),
        })
    }

    /// Whether `recorded`, what a call recorded for this question, can never change, so a stored
    /// result need not ask it again: an answer about commit ids, except an `is-ancestor` no, which
    /// a fetch can turn into a yes.
    #[must_use]
    pub fn lasting(&self, recorded: &[String]) -> bool {
        match self {
            Self::Tree { .. }
            | Self::MergeTree { .. }
            | Self::ChangedPaths { .. }
            | Self::ReadAt { .. } => true,
            Self::IsAncestor { .. } => recorded.is_empty(),
            Self::Head
            | Self::Branch
            | Self::Range
            | Self::Resolve { .. }
            | Self::Log { .. }
            | Self::SyncState { .. }
            | Self::Message
            | Self::Hook => false,
        }
    }

    /// The question as words, the kind first: what the result cache stores.
    #[must_use]
    pub fn to_words(&self) -> Vec<String> {
        let words: Vec<&str> = match self {
            Self::Head => vec!["head"],
            Self::Branch => vec!["branch"],
            Self::Range => vec!["range"],
            Self::Resolve { rev } => vec!["resolve", rev],
            Self::Tree { commit } => vec!["tree", commit],
            Self::MergeTree { first, second } => vec!["merge-tree", first, second],
            Self::Log { range } => vec!["log", range],
            Self::ChangedPaths { parent, commit } => vec!["changed-paths", parent, commit],
            Self::ReadAt { commit, path } => vec!["read-at", commit, path],
            Self::IsAncestor {
                ancestor,
                descendant,
            } => vec!["is-ancestor", ancestor, descendant],
            Self::SyncState { commit } => vec!["sync-state", commit],
            Self::Message => vec!["message"],
            Self::Hook => vec!["hook"],
        };
        words.into_iter().map(str::to_owned).collect()
    }

    /// The question [`GitQuery::to_words`] wrote; `None` for words it never writes.
    #[must_use]
    pub fn from_words(words: &[String]) -> Option<Self> {
        let owned = |index: usize| words.get(index).cloned();
        let query = match (words.first().map(String::as_str), words.len()) {
            (Some("head"), 1) => Self::Head,
            (Some("branch"), 1) => Self::Branch,
            (Some("range"), 1) => Self::Range,
            (Some("resolve"), 2) => Self::Resolve { rev: owned(1)? },
            (Some("tree"), 2) => Self::Tree { commit: owned(1)? },
            (Some("merge-tree"), 3) => Self::MergeTree {
                first: owned(1)?,
                second: owned(2)?,
            },
            (Some("log"), 2) => Self::Log { range: owned(1)? },
            (Some("changed-paths"), 3) => Self::ChangedPaths {
                parent: owned(1)?,
                commit: owned(2)?,
            },
            (Some("read-at"), 3) => Self::ReadAt {
                commit: owned(1)?,
                path: owned(2)?,
            },
            (Some("is-ancestor"), 3) => Self::IsAncestor {
                ancestor: owned(1)?,
                descendant: owned(2)?,
            },
            (Some("sync-state"), 2) => Self::SyncState { commit: owned(1)? },
            (Some("message"), 1) => Self::Message,
            (Some("hook"), 1) => Self::Hook,
            _ => return None,
        };
        Some(query)
    }
}

#[cfg(test)]
mod tests {
    use super::GitQuery;

    #[test]
    fn every_question_reads_back_from_its_words() {
        let id = |c: char| c.to_string().repeat(40);
        let queries = [
            GitQuery::Head,
            GitQuery::Branch,
            GitQuery::Range,
            GitQuery::Resolve {
                rev: "main".to_owned(),
            },
            GitQuery::Tree { commit: id('a') },
            GitQuery::MergeTree {
                first: id('a'),
                second: id('b'),
            },
            GitQuery::Log {
                range: "main..HEAD".to_owned(),
            },
            GitQuery::ChangedPaths {
                parent: id('a'),
                commit: id('b'),
            },
            GitQuery::ReadAt {
                commit: id('a'),
                path: "a b/c".to_owned(),
            },
            GitQuery::IsAncestor {
                ancestor: id('a'),
                descendant: id('b'),
            },
            GitQuery::SyncState { commit: id('c') },
            GitQuery::Message,
            GitQuery::Hook,
        ];
        for query in queries {
            assert_eq!(GitQuery::from_words(&query.to_words()), Some(query));
        }
        assert_eq!(
            GitQuery::from_words(&["head".to_owned(), "x".to_owned()]),
            None
        );
    }
}

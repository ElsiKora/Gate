//! What one call listed, read and asked git: the inputs its result depends on.

use std::collections::{BTreeMap, BTreeSet};

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

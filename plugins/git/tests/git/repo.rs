//! A synthetic repository for the native runs: commits with their files, refs, the snapshot's
//! head, and what `gate sync` produces at a commit and which of it drifts.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use gate_sdk::{
    CheckRequest, Commit, CommitRange, Finding, GitHost, Host, Location, RuleRun, SyncState,
    TreeHost,
};

struct Entry {
    parents: Vec<String>,
    message: String,
    files: BTreeMap<String, Vec<u8>>,
    order: usize,
}

#[derive(Default)]
pub struct Repo {
    commits: HashMap<String, Entry>,
    refs: HashMap<String, String>,
    head: Option<String>,
    branch: Option<String>,
    range: Option<CommitRange>,
    sync: HashMap<String, SyncState>,
    message: Option<String>,
    hook: bool,
}

impl Repo {
    /// A commit on `parents` (the first one's files, with `changes` applied: `None` deletes).
    pub fn commit(
        &mut self,
        parents: &[&str],
        message: &str,
        changes: &[(&str, Option<&str>)],
    ) -> String {
        let order = self.commits.len();
        let id = format!("a{order:039x}");
        let mut files = parents
            .first()
            .and_then(|parent| self.commits.get(*parent))
            .map(|entry| entry.files.clone())
            .unwrap_or_default();
        for (path, contents) in changes {
            match contents {
                Some(text) => files.insert((*path).to_owned(), text.as_bytes().to_vec()),
                None => files.remove(*path),
            };
        }
        self.commits.insert(
            id.clone(),
            Entry {
                parents: parents.iter().map(|parent| (*parent).to_owned()).collect(),
                message: format!("{message}\n"),
                files,
                order,
            },
        );
        id
    }

    /// Puts `bytes`, which need not be UTF-8, at `path` in the commit `id`.
    pub fn set_bytes(&mut self, id: &str, path: &str, bytes: &[u8]) {
        if let Some(entry) = self.commits.get_mut(id) {
            entry.files.insert(path.to_owned(), bytes.to_vec());
        }
    }

    pub fn set_ref(&mut self, name: &str, id: &str) {
        self.refs.insert(name.to_owned(), id.to_owned());
    }

    pub fn set_head(&mut self, id: &str) {
        self.head = Some(id.to_owned());
    }

    /// The checked-out branch, as `git symbolic-ref --short HEAD` names it; `None` is detached.
    pub fn set_branch(&mut self, name: Option<&str>) {
        self.branch = name.map(str::to_owned);
    }

    /// The commit message `--commit-message` passes; `None` for a run without one.
    pub fn set_message(&mut self, message: Option<&str>) {
        self.message = message.map(str::to_owned);
    }

    /// Whether a git hook starts the run (`--hook`).
    pub fn set_hook(&mut self, hook: bool) {
        self.hook = hook;
    }

    /// The run's range, as `--push` passes it: `head` is the pushed tip, `target_ref` the remote
    /// ref it updates.
    pub fn set_range(&mut self, head: &str, target_ref: Option<&str>) {
        self.range = Some(CommitRange {
            base: None,
            head: head.to_owned(),
            target_ref: target_ref.map(str::to_owned),
        });
    }

    /// What `gate sync` produces at `id`, and which of it drifts there.
    pub fn set_sync(&mut self, id: &str, produced: &[&str], drift: &[&str]) {
        let owned = |paths: &[&str]| paths.iter().map(|path| (*path).to_owned()).collect();
        self.sync.insert(
            id.to_owned(),
            SyncState {
                produced: owned(produced),
                drift: owned(drift),
            },
        );
    }

    fn find(&self, revision: &str) -> Option<String> {
        let revision = revision.trim_end_matches("^{commit}");
        if self.commits.contains_key(revision) {
            return Some(revision.to_owned());
        }
        self.refs.get(revision).cloned()
    }

    fn ancestry(&self, revision: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut stack: Vec<String> = self.find(revision).into_iter().collect();
        while let Some(id) = stack.pop() {
            if seen.insert(id.clone())
                && let Some(entry) = self.commits.get(&id)
            {
                stack.extend(entry.parents.iter().cloned());
            }
        }
        seen
    }

    /// Runs rule `name` of the git plugin with `[plugins.git] options` `settings` and `options`.
    pub fn check(
        &self,
        name: &str,
        settings: &str,
        options: &str,
    ) -> gate_sdk::Result<Vec<Finding>> {
        let request = CheckRequest {
            project: String::new(),
            rules: vec![RuleRun {
                name: name.to_owned(),
                options: options.to_owned(),
                options_file: None,
                exceptions: Vec::new(),
            }],
            plugin_options: settings.to_owned(),
            languages: String::new(),
            norms: Vec::new(),
        };
        gate_sdk::check(&git::PLUGIN, self, &request)
    }
}

impl TreeHost for Repo {
    fn list(&self, _prefix: &str) -> Vec<String> {
        Vec::new()
    }

    fn read(&self, _path: &str) -> Option<Vec<u8>> {
        None
    }

    fn changed(&self) -> Option<Vec<String>> {
        None
    }
}

impl Host for Repo {
    fn git(&self) -> Option<&dyn GitHost> {
        Some(self)
    }

    fn log(&self, _message: &str) {}
}

impl GitHost for Repo {
    fn head(&self) -> Option<String> {
        self.head.clone()
    }

    fn branch(&self) -> Option<String> {
        self.branch.clone()
    }

    fn range(&self) -> Option<CommitRange> {
        self.range.clone()
    }

    fn resolve(&self, rev: &str) -> Option<String> {
        self.find(rev)
    }

    fn tree(&self, commit: &str) -> String {
        self.commits
            .get(commit)
            .map_or_else(String::new, |entry| tree_id(&entry.files))
    }

    /// A three-way merge by whole files from the newest common ancestor; `None` when both sides
    /// change one path differently.
    fn merge_tree(&self, first: &str, second: &str) -> Option<String> {
        let ours = self.ancestry(first);
        let base = self
            .ancestry(second)
            .into_iter()
            .filter(|id| ours.contains(id))
            .max_by_key(|id| self.commits.get(id).map_or(0, |entry| entry.order))?;
        let files = |id: &str| {
            self.commits
                .get(id)
                .map(|entry| entry.files.clone())
                .unwrap_or_default()
        };
        let (base, left, right) = (files(&base), files(first), files(second));
        let mut merged = left.clone();
        let paths: BTreeSet<&String> = base.keys().chain(left.keys()).chain(right.keys()).collect();
        for path in paths {
            let (was, mine, theirs) = (base.get(path), left.get(path), right.get(path));
            if mine == theirs || theirs == was {
                continue;
            }
            if mine != was {
                return None;
            }
            match theirs {
                Some(bytes) => merged.insert(path.clone(), bytes.clone()),
                None => merged.remove(path),
            };
        }
        Some(tree_id(&merged))
    }

    fn log(&self, range: &str) -> Vec<Commit> {
        let ids = match range.split_once("..") {
            Some((from, to)) => {
                let excluded = self.ancestry(from);
                self.ancestry(to)
                    .into_iter()
                    .filter(|id| !excluded.contains(id))
                    .collect()
            }
            None => self.ancestry(range),
        };
        let mut entries: Vec<(&String, &Entry)> = ids
            .iter()
            .filter_map(|id| self.commits.get_key_value(id))
            .collect();
        entries.sort_by_key(|(_, entry)| std::cmp::Reverse(entry.order));
        entries
            .into_iter()
            .map(|(id, entry)| Commit {
                id: id.clone(),
                parents: entry.parents.clone(),
                message: entry.message.clone(),
                author_date: "2026-01-01T00:00:00+00:00".to_owned(),
            })
            .collect()
    }

    fn changed_paths(&self, parent: &str, commit: &str) -> Vec<String> {
        let empty = BTreeMap::new();
        let files = |id: &str| self.commits.get(id).map_or(&empty, |entry| &entry.files);
        let (before, after) = (files(parent), files(commit));
        before
            .keys()
            .chain(after.keys())
            .filter(|path| before.get(*path) != after.get(*path))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn read_at(&self, commit: &str, path: &str) -> Option<Vec<u8>> {
        self.commits.get(commit)?.files.get(path).cloned()
    }

    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        self.find(ancestor)
            .is_some_and(|id| self.ancestry(descendant).contains(&id))
    }

    fn sync_state(&self, commit: &str) -> SyncState {
        self.sync.get(commit).cloned().unwrap_or_default()
    }

    fn message(&self) -> Option<String> {
        self.message.clone()
    }

    fn hook(&self) -> bool {
        self.hook
    }
}

fn tree_id(files: &BTreeMap<String, Vec<u8>>) -> String {
    format!("{files:?}")
}

/// Each finding as `aspect location message`.
pub fn lines(findings: &[Finding]) -> Vec<String> {
    findings
        .iter()
        .map(|finding| {
            let at = match &finding.location {
                Some(Location::Commit(id)) => format!("commit:{id}"),
                other => panic!("a git finding is on a commit: {other:?}"),
            };
            format!("{} {at} {}", finding.aspect, finding.message)
        })
        .collect()
}

/// The `[plugins.git] options` of the tests, with `activation`.
pub fn settings(activation: &str) -> String {
    format!(
        r#"{{"main": ["refs/remotes/origin/main", "refs/heads/main"], "activation": "{activation}",
            "release-subject": "^chore\\(release\\): (\\d+\\.\\d+\\.\\d+) \\[skip ci\\]$",
            "branch-pattern": "^(feat|fix|docs)/[a-z0-9-]+$", "branch-length": [8, 30],
            "documentation": ["docs/**", "*.md"],
            "dependabot": ["package.json", "package-lock.json"]}}"#
    )
}

/// The first twelve characters of a commit id.
pub fn short(id: &str) -> &str {
    &id[..12]
}

/// `settings` with the keys of the JSON object `extra` added, as `[plugins.git] options`.
pub fn merged(settings: &str, extra: &str) -> String {
    let mut settings: serde_json::Value = serde_json::from_str(settings).expect("settings");
    let extra: serde_json::Value = serde_json::from_str(extra).expect("extra keys");
    if let (Some(settings), Some(extra)) = (settings.as_object_mut(), extra.as_object()) {
        settings.extend(extra.clone());
    }
    settings.to_string()
}

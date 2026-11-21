//! The history the rules classify: main since activation, and the snapshot's commits ahead of it.

use std::collections::{HashMap, HashSet};

use gate_sdk::{Error, Git, Location, Result};
use regex::Regex;

use crate::forms::{Compiled, Kind, form_of};
use crate::settings::Settings;

/// One commit, with the message the rules read.
#[derive(Clone, Debug)]
pub(crate) struct Commit {
    pub(crate) id: String,
    pub(crate) parents: Vec<String>,
    /// The raw message (`%B`).
    pub(crate) message: String,
}

impl Commit {
    pub(crate) fn new(commit: gate_sdk::Commit) -> Self {
        Self {
            id: commit.id,
            parents: commit.parents,
            message: commit.message,
        }
    }

    /// The first twelve characters of the id, as findings name a commit.
    pub(crate) fn short(&self) -> &str {
        self.id.get(..12).unwrap_or(&self.id)
    }

    /// Where a finding about the commit points.
    pub(crate) fn location(&self) -> Location {
        Location::commit(self.id.clone())
    }
}

/// The commits the rules classify.
#[derive(Debug)]
pub(crate) struct History {
    /// The first-parent commits of `activation..main`, oldest first.
    pub(crate) mainline: Vec<Commit>,
    pub(crate) on_mainline: HashSet<String>,
    /// Every commit of `activation..main`, then the snapshot's commits that main does not have.
    pub(crate) commits: Vec<Commit>,
}

impl History {
    /// Reads the history of main since activation. Main is the tip of a push that updates one of
    /// the `main` references, or else the first reference of `main` that names a commit.
    ///
    /// # Errors
    /// When the settings have no activation commit, no reference of main names a commit, or the
    /// activation commit is not in its history.
    pub(crate) fn read(git: &Git<'_>, settings: &Settings, rule: &str) -> Result<Self> {
        let activation = settings.activation(rule)?;
        let pushed = git
            .range()
            .filter(|range| {
                range
                    .target_ref
                    .as_deref()
                    .is_some_and(|target| settings.is_main(target))
            })
            .map(|range| range.head);
        let main = match pushed {
            Some(head) => head,
            None => main_reference(git, settings)?,
        };
        if !git.is_ancestor(activation, &main) {
            return Err(Error::new(format!(
                "the activation commit {activation} is not an ancestor of main {main}; fetch the \
                 full history (fetch-depth: 0), or fix [plugins.git] options activation"
            )));
        }

        // Newest first; the first commit is the tip of main.
        let range = git.log(&format!("{activation}..{main}"));
        let by_id: HashMap<&str, &gate_sdk::Commit> = range
            .iter()
            .map(|commit| (commit.id.as_str(), commit))
            .collect();
        let mut chain = Vec::new();
        let mut cursor = range.first();
        while let Some(commit) = cursor {
            chain.push(Commit::new(commit.clone()));
            cursor = commit
                .parents
                .first()
                .and_then(|parent| by_id.get(parent.as_str()).copied());
        }
        chain.reverse();
        let on_mainline = chain.iter().map(|commit| commit.id.clone()).collect();

        let mut commits: Vec<Commit> = range.into_iter().rev().map(Commit::new).collect();
        if let Some(head) = git.head() {
            commits.extend(ahead(git, &main, &head));
        }

        Ok(Self {
            mainline: chain,
            on_mainline,
            commits,
        })
    }

    /// Whether `commit` is a release commit: on main's first-parent line, and its first form a
    /// release form; never when the forms have no release form.
    pub(crate) fn is_release(
        &self,
        commit: &Commit,
        forms: &[Compiled],
        release: Option<&Regex>,
    ) -> bool {
        self.on_mainline.contains(&commit.id)
            && form_of(forms, commit, release).is_some_and(|form| form.kind == Kind::Release)
    }
}

/// The commit main is at: the first reference of `[plugins.git] options main` that names a commit.
///
/// # Errors
/// When none of them does.
pub(crate) fn main_reference(git: &Git<'_>, settings: &Settings) -> Result<String> {
    settings
        .main
        .iter()
        .find_map(|reference| git.resolve(&format!("{reference}^{{commit}}")))
        .ok_or_else(|| {
            Error::new(format!(
                "none of the main references {:?} exists; fetch main, or fix [plugins.git] options \
                 main",
                settings.main
            ))
        })
}

/// The commits `head` adds to `main`, oldest first: those reachable from `head` and not from
/// `main` (`main..head`). The set a branch's own commits make, for every rule that judges them.
pub(crate) fn ahead(git: &Git<'_>, main: &str, head: &str) -> Vec<Commit> {
    git.log(&format!("{main}..{head}"))
        .into_iter()
        .rev()
        .map(Commit::new)
        .collect()
}

/// Whether `text` is a full commit id: 40 (SHA-1) or 64 (SHA-256) lower-case hex digits, as a
/// finding on a commit names it.
pub(crate) fn is_commit_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64)
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// The targets of `exceptions` of rule `rule` that are full commit ids, in order, without repeats.
///
/// # Errors
/// When a target is a commit id written in a form no finding names: a full-length (40 or 64 hex
/// digits) id with an upper-case digit, or a shorter hex target that `git` resolves to a commit.
/// A finding on a commit names its full lower-case id, so such an exception would never match
/// one, and the rule could not judge the commit it means. A hex target that names no commit, a
/// body line of digits for one, is no commit id and passes.
pub(crate) fn excepted_commits<'e>(
    git: &Git<'_>,
    rule: &str,
    exceptions: &'e [String],
) -> Result<Vec<&'e str>> {
    let mut ids: Vec<&str> = Vec::new();
    for target in exceptions {
        if is_commit_id(target) {
            if !ids.contains(&target.as_str()) {
                ids.push(target);
            }
            continue;
        }
        if !(7..=64).contains(&target.len()) || !target.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            continue;
        }
        if matches!(target.len(), 40 | 64) {
            return Err(Error::config(format!(
                "the exception for {rule} at {target:?} reads as an upper-case commit id, which no \
                 finding names; write it in lower case ({}) as its target",
                target.to_ascii_lowercase()
            )));
        }
        if let Some(id) = git.resolve(target) {
            return Err(Error::config(format!(
                "the exception for {rule} at {target:?} is a short commit id, which no finding \
                 names; write the commit's full id ({id}) as its target"
            )));
        }
    }
    Ok(ids)
}

/// The commit `id` when `head`'s history holds it, read alone (`git log <id>^!`, no walk of the
/// history); `None` when it does not, or there is no head: the commit was rewritten or dropped.
pub(crate) fn held_commit(git: &Git<'_>, id: &str, head: Option<&str>) -> Option<Commit> {
    let head = head?;
    if !git.is_ancestor(id, head) {
        return None;
    }
    git.log(&format!("{id}^!"))
        .into_iter()
        .next()
        .map(Commit::new)
}

/// The branch a reference names: `refs/heads/<branch>` and `refs/remotes/<remote>/<branch>` name
/// `<branch>`; any other reference is a branch name already.
pub(crate) fn branch(reference: &str) -> &str {
    if let Some(name) = reference.strip_prefix("refs/heads/") {
        return name;
    }
    reference
        .strip_prefix("refs/remotes/")
        .and_then(|rest| rest.split_once('/'))
        .map_or(reference, |(_, name)| name)
}

/// The version of a release commit's message: capture 1 of the release pattern on the whole
/// trimmed message (empty when the pattern has no group); `None` when it does not match.
pub(crate) fn release_version(release: &Regex, message: &str) -> Option<String> {
    let captures = release.captures(message.trim())?;
    Some(
        captures
            .get(1)
            .map_or_else(String::new, |version| version.as_str().to_owned()),
    )
}

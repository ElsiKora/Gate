//! The forms of the commits on main, `[plugins.git] options forms`: what each commit is, by its
//! parent count and subject, and which checks of `git/main-history` it passes. `git/no-ci-skip`
//! exempts the commits of the release forms.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Result};
use regex::Regex;

use crate::history::{Commit, release_version};
use crate::settings::pattern;

/// The forms of the policy when `[plugins.git] options` has none: an unedited merge of a branch,
/// a release commit, a direct commit.
#[must_use]
pub fn default_forms() -> Vec<Form> {
    vec![
        Form {
            kind: Kind::Merge,
            parents: Some(2),
            subject: Some("^Merge branch '([^']+)'$".to_owned()),
            checks: None,
        },
        Form {
            kind: Kind::Release,
            parents: Some(1),
            subject: None,
            checks: None,
        },
        Form {
            kind: Kind::Direct,
            parents: Some(1),
            subject: None,
            checks: None,
        },
    ]
}

/// Compiles the forms, named in errors by their place in `[plugins.git] options forms`.
///
/// # Errors
/// When the list is empty or a form is refused by [`compile`].
pub(crate) fn compile_all(forms: &[Form], release: Option<&Regex>) -> Result<Vec<Compiled>> {
    if forms.is_empty() {
        return Err(Error::config(
            "[plugins.git] options forms is empty; list the forms a commit on main may have, or \
             leave forms out for the default merge, release and direct forms",
        ));
    }
    forms
        .iter()
        .enumerate()
        .map(|(index, form)| compile(&place(index, form.kind), form, release))
        .collect()
}

/// Where errors name the form at `index`.
pub(crate) fn place(index: usize, kind: Kind) -> String {
    format!(
        "[plugins.git] options form {} ({})",
        index + 1,
        kind.aspect()
    )
}

/// The first form `commit` has.
pub(crate) fn form_of<'f>(
    forms: &'f [Compiled],
    commit: &Commit,
    release: Option<&Regex>,
) -> Option<&'f Compiled> {
    forms.iter().find(|form| {
        form.parents
            .is_none_or(|parents| parents == commit.parents.len())
            && match form.kind {
                Kind::Release => release
                    .is_some_and(|release| release_version(release, &commit.message).is_some()),
                Kind::Merge | Kind::Direct => form
                    .subject
                    .as_ref()
                    .is_none_or(|pattern| pattern.is_match(subject(&commit.message))),
            }
    })
}

/// The subject of a message: its first line.
pub(crate) fn subject(message: &str) -> &str {
    message.lines().next().unwrap_or_default()
}

/// One form a commit on main may have.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Form {
    /// What the commit is; its findings are reported under this aspect.
    pub kind: Kind,
    /// The number of parents the commit has; any number when absent.
    pub parents: Option<usize>,
    /// A regular expression the commit's subject (its first line) matches; any subject when
    /// absent. A release form takes none: it matches `release-subject` of `[plugins.git] options`.
    pub subject: Option<String>,
    /// The checks a commit of this form passes; every check of its kind when absent.
    pub checks: Option<Vec<Check>>,
}

/// The kind of a form, and the aspect of its findings.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(crate = "gate_sdk::serde", rename_all = "kebab-case")]
#[schemars(crate = "gate_sdk::schemars")]
pub enum Kind {
    /// A merge into main.
    Merge,
    /// A release commit.
    Release,
    /// A commit made on main itself.
    Direct,
}

impl Kind {
    pub(crate) fn aspect(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Release => "release",
            Self::Direct => "direct",
        }
    }

    /// The checks a form of this kind may run, which are its checks by default.
    pub(crate) fn checks(self) -> &'static [Check] {
        match self {
            Self::Merge => &[Check::Body, Check::Branch, Check::Sync, Check::Tree],
            Self::Release | Self::Direct => &[Check::Files],
        }
    }
}

/// A check of a form.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(crate = "gate_sdk::serde", rename_all = "kebab-case")]
#[schemars(crate = "gate_sdk::schemars")]
pub enum Check {
    /// A merge: no line after the subject.
    Body,
    /// A merge: capture 1 of the subject, the branch, is named by the policy or is Dependabot's.
    Branch,
    /// A merge: its second parent is not already on main.
    Sync,
    /// A merge: its tree is git's conflict-free merge of its parents.
    Tree,
    /// A release: only the release files and their versions change. A direct commit: only
    /// documentation and what `gate sync` produces in sync change.
    Files,
}

impl Check {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Branch => "branch",
            Self::Sync => "sync",
            Self::Tree => "tree",
            Self::Files => "files",
        }
    }
}

/// A form, compiled and checked.
pub(crate) struct Compiled {
    pub(crate) kind: Kind,
    pub(crate) parents: Option<usize>,
    pub(crate) subject: Option<Regex>,
    pub(crate) checks: Vec<Check>,
}

impl Compiled {
    pub(crate) fn runs(&self, check: Check) -> bool {
        self.checks.contains(&check)
    }
}

/// Compiles `form`, named `at` in errors, and checks that its checks have what they read.
///
/// # Errors
/// When a check is not one of its kind's, the branch check has no subject capture, the merge
/// checks lack two parents or the files check one, a release form has a subject or the settings
/// no release subject, or the subject does not compile.
pub(crate) fn compile(at: &str, form: &Form, release: Option<&Regex>) -> Result<Compiled> {
    let kind = form.kind;
    let allowed = kind.checks();
    let checks = form.checks.clone().unwrap_or_else(|| allowed.to_vec());
    if let Some(check) = checks.iter().find(|check| !allowed.contains(check)) {
        let names: Vec<&str> = allowed.iter().map(|check| check.name()).collect();
        return Err(Error::config(format!(
            "{at} has the check {}, which a {} form does not run; a {} form runs {}",
            check.name(),
            kind.aspect(),
            kind.aspect(),
            names.join(", ")
        )));
    }
    let subject = match (kind, form.subject.as_deref()) {
        (Kind::Release, Some(_)) => {
            return Err(Error::config(format!(
                "{at} has a subject; a release form matches the release commits by \
                 [plugins.git] options release-subject: remove subject"
            )));
        }
        (Kind::Release, None) if release.is_none() => {
            return Err(Error::config(format!(
                "[plugins.git] options has no release-subject; {at} matches the release commits \
                 by it: set it, or remove the release form"
            )));
        }
        (_, Some(text)) => Some(pattern(&format!("{at} subject"), text)?),
        (_, None) => None,
    };
    let runs = |check: Check| checks.contains(&check);
    if runs(Check::Branch)
        && subject
            .as_ref()
            .is_none_or(|subject| subject.captures_len() < 2)
    {
        return Err(Error::config(format!(
            "{at} runs the branch check, which reads the branch name from capture 1 of its \
             subject; give the form a subject that captures the branch name in group 1, or \
             remove branch from its checks"
        )));
    }
    if let Some(check) = [Check::Sync, Check::Tree]
        .into_iter()
        .find(|check| runs(*check) && form.parents != Some(2))
    {
        return Err(Error::config(format!(
            "{at} runs the {} check, which compares a merge's two parents; set parents = 2, or \
             remove {} from its checks",
            check.name(),
            check.name()
        )));
    }
    if runs(Check::Files) && form.parents != Some(1) {
        return Err(Error::config(format!(
            "{at} runs the files check, which compares the commit with its one parent; set \
             parents = 1, or remove files from its checks"
        )));
    }
    Ok(Compiled {
        kind,
        parents: form.parents,
        subject,
        checks,
    })
}

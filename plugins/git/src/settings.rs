//! `[plugins.git] options`: the policy the rules share, written once.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Need, Result};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use regex::Regex;

use crate::forms::{Compiled, Form, compile_all, default_forms};

/// The `[plugins.git] options`. `main` is required; each other key is required by the rule, or the
/// form of `git/main-history`, that reads it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Settings {
    /// The references of main, tried in order; the first whose history holds the activation
    /// commit is main (`refs/remotes/origin/main` before `refs/heads/main`).
    pub main: Vec<String>,
    /// The 40-hex commit from which the policy holds; it and its ancestors are not checked. The
    /// history rules require it.
    pub activation: Option<String>,
    /// A regular expression a release commit's whole message matches; capture 1 is the version.
    pub release_subject: Option<String>,
    /// A regular expression a merged branch name matches.
    pub branch_pattern: Option<String>,
    /// The shortest and longest merged branch name, in characters.
    pub branch_length: Option<[usize; 2]>,
    /// Globs of the files a direct commit may change.
    pub documentation: Option<Vec<String>>,
    /// Globs of the files a Dependabot branch may change.
    pub dependabot: Option<Vec<String>>,
    /// The forms a commit on main may have, tried in order; [`default_forms`] when absent.
    pub forms: Option<Vec<Form>>,
}

impl Settings {
    /// The settings of `run`, checked.
    ///
    /// # Errors
    /// When they are missing, do not match, `activation` is not a full commit id, `release-subject`
    /// does not compile or has no capture group, or `branch-length` has its minimum above its
    /// maximum.
    pub fn read<O>(run: &gate_sdk::Run<'_, O>) -> Result<Self> {
        let settings: Self = run.plugin_options().map_err(|error| {
            Error::config(format!(
                "{error}; the git rules read main and the policy keys from [plugins.git] options"
            ))
        })?;
        if settings.main.is_empty() {
            return Err(Error::config(
                "[plugins.git] options main is empty; name the references of main, for example \
                 [\"refs/remotes/origin/main\", \"refs/heads/main\"]",
            ));
        }
        if let Some(activation) = &settings.activation {
            let hex = activation.len() == 40
                && activation
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
            if !hex {
                return Err(Error::config(format!(
                    "[plugins.git] options activation {activation:?} is not a full commit id; \
                     write the 40 lowercase hexadecimal characters of the commit the policy \
                     starts after"
                )));
            }
        }
        if let Some(release) = settings.release()?
            && release.captures_len() < 2
        {
            return Err(Error::config(format!(
                "[plugins.git] options release-subject {:?} has no capture group; capture the \
                 version in group 1, for example '^chore\\(release\\): (\\d+\\.\\d+\\.\\d+)$'",
                release.as_str()
            )));
        }
        if let Some([shortest, longest]) = settings.branch_length
            && shortest > longest
        {
            return Err(Error::config(format!(
                "[plugins.git] options branch-length [{shortest}, {longest}] has its minimum above \
                 its maximum; write [shortest, longest]"
            )));
        }
        Ok(settings)
    }

    /// The activation commit, which the history rules require.
    ///
    /// # Errors
    /// When it is absent.
    pub fn activation(&self, rule: &str) -> Result<&str> {
        self.activation.as_deref().ok_or_else(|| {
            Error::config(format!(
                "[plugins.git] options has no activation; {rule} reads history after it: write \
                 the 40-hex id of the commit the policy starts after"
            ))
        })
    }

    /// Whether `reference` (a ref or a branch name) names the branch of one of the `main`
    /// references.
    #[must_use]
    pub fn is_main(&self, reference: &str) -> bool {
        let name = crate::history::branch(reference);
        self.main
            .iter()
            .any(|main| crate::history::branch(main) == name)
    }

    /// The forms, compiled; [`default_forms`] when the settings have none.
    ///
    /// # Errors
    /// When the forms are empty or a form is not coherent.
    pub(crate) fn forms(&self, release: Option<&Regex>) -> Result<Vec<Compiled>> {
        match &self.forms {
            Some(forms) => compile_all(forms, release),
            None => compile_all(&default_forms(), release),
        }
    }

    /// The release pattern; `None` when the settings have none.
    ///
    /// # Errors
    /// When it does not compile.
    pub fn release(&self) -> Result<Option<Regex>> {
        self.release_subject
            .as_deref()
            .map(|text| pattern("release-subject", text))
            .transpose()
    }
}

/// The options of `[plugins.git]` match this when their forms (or the default forms) hold a
/// release form.
const RELEASE_FORM: &str = r#"{"anyOf": [
    {"not": {"required": ["forms"]}},
    {"properties": {"forms": {"contains": {
        "required": ["kind"], "properties": {"kind": {"const": "release"}}
    }}}}
]}"#;

/// The options match this when their forms (or the default forms) hold a merge form that runs the
/// branch check.
const BRANCH_CHECK: &str = r#"{"anyOf": [
    {"not": {"required": ["forms"]}},
    {"properties": {"forms": {"contains": {
        "required": ["kind"], "properties": {"kind": {"const": "merge"}},
        "anyOf": [
            {"not": {"required": ["checks"]}},
            {"properties": {"checks": {"contains": {"const": "branch"}}}}
        ]
    }}}}
]}"#;

/// The options match this when their forms (or the default forms) hold a direct form that runs
/// the files check.
const FILES_CHECK: &str = r#"{"anyOf": [
    {"not": {"required": ["forms"]}},
    {"properties": {"forms": {"contains": {
        "required": ["kind"], "properties": {"kind": {"const": "direct"}},
        "anyOf": [
            {"not": {"required": ["checks"]}},
            {"properties": {"checks": {"contains": {"const": "files"}}}}
        ]
    }}}}
]}"#;

/// The history rules read the history after `activation`.
pub(crate) const ACTIVATION: Need = Need {
    key: "activation",
    when: None,
    why: "the rule reads history after it; write the 40-hex id of the commit the policy starts \
          after",
};

/// A release form matches the release commits by `release-subject`.
pub(crate) const RELEASE_SUBJECT: Need = Need {
    key: "release-subject",
    when: Some(RELEASE_FORM),
    why: "the release form (one of the default forms when forms is absent) matches the release \
          commits by it; set it, or write forms without a release form",
};

/// The keys the branch check of a merge form reads.
pub(crate) const BRANCH_KEYS: [Need; 3] = [
    branch_key("branch-pattern"),
    branch_key("branch-length"),
    branch_key("dependabot"),
];

const fn branch_key(key: &'static str) -> Need {
    Need {
        key,
        when: Some(BRANCH_CHECK),
        why: "the branch check of a merge form (the default merge form runs it) reads it; set \
              it, or remove branch from the merge form's checks",
    }
}

/// The key the files check of a direct form reads.
pub(crate) const DOCUMENTATION: Need = Need {
    key: "documentation",
    when: Some(FILES_CHECK),
    why: "the files check of a direct form (the default direct form runs it) reads it; set it, \
          or remove files from the direct form's checks",
};

/// A key of `[plugins.git] options` that `reader` (a rule, or a check of a form) needs.
///
/// # Errors
/// When the key is absent.
pub(crate) fn required<'s, T>(value: Option<&'s T>, key: &str, reader: &str) -> Result<&'s T> {
    value.ok_or_else(|| {
        Error::config(format!(
            "[plugins.git] options has no {key}; {reader} reads it: set it, or turn that check off"
        ))
    })
}

/// A regular expression of the settings or options.
///
/// # Errors
/// When it does not compile, naming the key.
pub(crate) fn pattern(key: &str, text: &str) -> Result<Regex> {
    Regex::new(text).map_err(|error| {
        Error::config(format!(
            "{key} {text:?} is not a valid regular expression: {error}"
        ))
    })
}

/// Globs whose `*` does not cross `/`.
///
/// # Errors
/// When one does not compile, naming it.
pub(crate) fn globs(key: &str, patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(
            GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map_err(|error| {
                    Error::config(format!("{key} glob {pattern:?} is not valid: {error}"))
                })?,
        );
    }
    builder
        .build()
        .map_err(|error| Error::config(format!("{key} globs do not build: {error}")))
}

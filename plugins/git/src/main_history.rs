//! `git/main-history`: every first-parent commit of main since activation has one of the policy's
//! forms (`[plugins.git] options forms`), and passes the checks of its form: by default an unedited `--no-ff` merge of a
//! well-named branch (`#merge`), a release commit that changes only the release files
//! (`#release`), or a direct commit that changes only documentation (`#direct`).

use std::collections::{BTreeMap, HashSet};

use gate_agents_format::parse_rule_file;
use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::serde_json::{self, Value};
use gate_sdk::{Git, Result, Rule, Run, Scope, SyncState};
use globset::GlobSet;
use regex::Regex;

use crate::forms::{Check, Compiled, Kind, form_of, place, subject};
use crate::history::{Commit, History, release_version};
use crate::settings::{Settings, globs, pattern, required};

/// The rule.
#[derive(Debug, Default)]
pub struct MainHistory;

/// The options of `git/main-history`; the shared policy is in `[plugins.git] options`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The branch prefix of Dependabot's branches, which need not match `branch-pattern`.
    pub dependabot_prefix: String,
    /// The files a release commit may change.
    pub release_files: Vec<String>,
    /// The directory of the agent rule files whose Enforcement/Status lines a direct commit may
    /// not change.
    pub rules_directory: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            dependabot_prefix: "dependabot/".to_owned(),
            release_files: ["package.json", "package-lock.json", "CHANGELOG.md"]
                .map(str::to_owned)
                .to_vec(),
            rules_directory: "docs/agents/rules".to_owned(),
        }
    }
}

/// What the branch check of a merge reads.
struct Branches {
    pattern: Regex,
    length: [usize; 2],
    dependabot: GlobSet,
}

/// Everything the classification reads, compiled once.
struct Policy<'r> {
    git: Git<'r>,
    forms: Vec<Compiled>,
    release: Option<Regex>,
    branches: Option<Branches>,
    /// Empty when no direct form runs the files check.
    documentation: GlobSet,
    options: &'r Options,
}

/// The findings of one commit: aspect and message.
type Found = Vec<(&'static str, String)>;

impl Rule for MainHistory {
    type Options = Options;

    const NAME: &'static str = "main-history";
    const TITLE: &'static str =
        "Every first-parent commit of main since activation follows the git policy";
    const ASPECTS: &'static [&'static str] = &["direct", "merge", "release"];
    const SCOPE: Scope = Scope::Repository;
    const READS_HISTORY: bool = true;

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        let settings = Settings::read(run)?;
        let options = run.options();
        let policy = Policy::new(run.git()?, &settings, options)?;
        let history = History::read(&policy.git, &settings, "git/main-history")?;

        let mut found = Vec::new();
        for commit in &history.mainline {
            match policy.form(commit) {
                Some(form) => found.push((
                    commit,
                    policy.classify(form, commit, &history.on_mainline),
                    None,
                )),
                None => found.push((
                    commit,
                    vec![(unmatched_aspect(commit), unmatched(commit))],
                    Some(UNMATCHED_HELP),
                )),
            }
        }

        for (commit, findings, unmatched) in found {
            for (aspect, message) in findings {
                run.report()
                    .finding(aspect, commit.location(), message)
                    .target(commit.id.clone())
                    .help(unmatched.unwrap_or_else(|| help(aspect)));
            }
        }
        Ok(())
    }
}

/// The help of a commit that has none of the forms.
const UNMATCHED_HELP: &str = "give the commit one of the forms [plugins.git] options forms lists; \
                              history on main is not rewritten, so a commit already there gets \
                              an exception";

/// A commit with no form is a merge when it has more than one parent, else a direct commit.
fn unmatched_aspect(commit: &Commit) -> &'static str {
    if commit.parents.len() > 1 {
        "merge"
    } else {
        "direct"
    }
}

/// The finding of a commit that has none of the forms.
fn unmatched(commit: &Commit) -> String {
    let parents = match commit.parents.len() {
        1 => "1 parent".to_owned(),
        count => format!("{count} parents"),
    };
    format!(
        "{} has none of the forms of [plugins.git] options forms: subject {:?}, {parents}",
        commit.short(),
        subject(&commit.message)
    )
}

fn help(aspect: &str) -> &'static str {
    match aspect {
        "merge" => {
            "merge a branch whose name follows the policy with `git merge --no-ff <branch>` and \
             keep git's message; resolve conflicts on the branch, not in the merge commit"
        }
        "release" => {
            "let the release commit change only the release files and their version fields; \
             anything else reaches main through a branch merge"
        }
        _ => {
            "a direct commit to main may change documentation only; move the change to a branch \
             and merge it"
        }
    }
}

impl<'r> Policy<'r> {
    /// The policy of `options`, its forms checked against what they need.
    ///
    /// # Errors
    /// When a form is not coherent (a check its kind does not run, a check without what it reads,
    /// a release form with a subject), a pattern or glob does not compile, or a key the forms read
    /// is absent from `[plugins.git] options`.
    fn new(git: Git<'r>, settings: &Settings, options: &'r Options) -> Result<Self> {
        let release = settings.release()?;
        let forms = settings.forms(release.as_ref())?;
        let mut branches = None;
        let mut documentation = None;
        for (index, compiled) in forms.iter().enumerate() {
            let at = place(index, compiled.kind);
            let reader = |check: Check| format!("the {} check of {at}", check.name());
            if compiled.kind == Kind::Merge && compiled.runs(Check::Branch) && branches.is_none() {
                let reader = reader(Check::Branch);
                branches = Some(Branches {
                    pattern: pattern(
                        "branch-pattern",
                        required(settings.branch_pattern.as_ref(), "branch-pattern", &reader)?,
                    )?,
                    length: *required(settings.branch_length.as_ref(), "branch-length", &reader)?,
                    dependabot: globs(
                        "dependabot",
                        required(settings.dependabot.as_ref(), "dependabot", &reader)?,
                    )?,
                });
            }
            if compiled.kind == Kind::Direct
                && compiled.runs(Check::Files)
                && documentation.is_none()
            {
                documentation = Some(globs(
                    "documentation",
                    required(
                        settings.documentation.as_ref(),
                        "documentation",
                        &reader(Check::Files),
                    )?,
                )?);
            }
        }
        Ok(Self {
            git,
            forms,
            release,
            branches,
            documentation: documentation.unwrap_or_else(GlobSet::empty),
            options,
        })
    }

    /// The first form `commit` has.
    fn form(&self, commit: &Commit) -> Option<&Compiled> {
        form_of(&self.forms, commit, self.release.as_ref())
    }

    /// `mainline` holds the first-parent commits of main since activation.
    fn classify(&self, form: &Compiled, commit: &Commit, mainline: &HashSet<String>) -> Found {
        match form.kind {
            Kind::Merge => self.merge(form, commit, mainline),
            Kind::Release => match (
                form.runs(Check::Files),
                commit.parents.first(),
                self.release
                    .as_ref()
                    .and_then(|release| release_version(release, &commit.message)),
            ) {
                (true, Some(parent), Some(version)) => self.release(commit, parent, &version),
                _ => Vec::new(),
            },
            Kind::Direct => match (form.runs(Check::Files), commit.parents.first()) {
                (true, Some(parent)) => self.direct(commit, parent),
                _ => Vec::new(),
            },
        }
    }

    fn merge(&self, form: &Compiled, commit: &Commit, mainline: &HashSet<String>) -> Found {
        let short = commit.short();
        let mut found = Vec::new();
        let first = commit.parents.first();

        let branch = self
            .branches
            .as_ref()
            .filter(|_| form.runs(Check::Branch))
            .map(|branches| {
                let name = form
                    .subject
                    .as_ref()
                    .and_then(|pattern| pattern.captures(subject(&commit.message)))
                    .and_then(|captures| captures.get(1));
                (branches, name)
            });
        if let Some((_, None)) = branch {
            found.push((
                "merge",
                format!(
                    "{short} subject {:?} names no branch: capture 1 of its form's subject is empty",
                    subject(&commit.message)
                ),
            ));
        }
        if let Some((branches, Some(name))) = branch {
            let name = name.as_str();
            let prefix = self.options.dependabot_prefix.as_str();
            let length = name.chars().count();
            let named = branches.pattern.is_match(name)
                && length >= branches.length[0]
                && length <= branches.length[1];
            if name.starts_with(prefix) {
                for file in first
                    .map(|first| self.git.changed_paths(first, &commit.id))
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|file| !branches.dependabot.is_match(file))
                {
                    found.push((
                        "merge",
                        format!("{short} merges a Dependabot branch that changes {file}"),
                    ));
                }
            } else if !named {
                found.push((
                    "merge",
                    format!(
                        "{short} merges branch {name}, which neither matches branch-pattern within \
                         branch-length nor starts with {prefix}"
                    ),
                ));
            }
        }

        if form.runs(Check::Body)
            && commit
                .message
                .trim_end()
                .lines()
                .skip(1)
                .any(|line| !line.trim().is_empty())
        {
            found.push(("merge", format!("{short} has an edited merge message body")));
        }
        if let [first, second] = commit.parents.as_slice() {
            // A second parent on main's first-parent line is an ancestor of the first parent: a
            // set lookup answers it; git is asked only when the lookup misses.
            if form.runs(Check::Sync)
                && (mainline.contains(second) || self.git.is_ancestor(second, first))
            {
                found.push((
                    "merge",
                    format!("{short} is a sync merge: its second parent is already on main"),
                ));
            }
            if form.runs(Check::Tree)
                && let Some(clean) = self.git.merge_tree(first, second)
                && clean != self.git.tree(&commit.id)
            {
                found.push((
                    "merge",
                    format!(
                        "{short} differs from git's conflict-free merge of its parents; the merge commit adds its own change"
                    ),
                ));
            }
        }
        found
    }

    fn release(&self, commit: &Commit, parent: &str, version: &str) -> Found {
        let short = commit.short();
        let mut found = Vec::new();
        let changed = self.git.changed_paths(parent, &commit.id);

        for file in changed
            .iter()
            .filter(|file| !self.options.release_files.contains(file))
        {
            found.push(("release", format!("release commit {short} changes {file}")));
        }

        if changed.iter().any(|file| file == "package-lock.json") {
            let lock = |id: &str| -> Option<Value> {
                self.json(id, "package-lock.json").map(|mut lock| {
                    if let Some(object) = lock.as_object_mut() {
                        object.remove("version");
                    }
                    if let Some(root) = lock
                        .pointer_mut("/packages/")
                        .and_then(Value::as_object_mut)
                    {
                        root.remove("version");
                    }
                    lock
                })
            };
            match (lock(parent), lock(&commit.id)) {
                (Some(before), Some(after)) => {
                    if before != after {
                        found.push((
                            "release",
                            format!(
                                "release commit {short} changes package-lock.json beyond its version fields"
                            ),
                        ));
                    }
                }
                // Absent on both sides: nothing to compare.
                (None, None)
                    if self.git.read_at(parent, "package-lock.json").is_none()
                        && self.git.read_at(&commit.id, "package-lock.json").is_none() => {}
                _ => found.push((
                    "release",
                    format!("release commit {short} has an unreadable package-lock.json"),
                )),
            }
        }

        if changed.iter().any(|file| file == "package.json") {
            match (
                self.json(parent, "package.json"),
                self.json(&commit.id, "package.json"),
            ) {
                (Some(mut before), Some(mut after)) => {
                    let released = after
                        .get("version")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    if released.as_deref() != Some(version) {
                        found.push((
                            "release",
                            format!(
                                "release commit {short} sets version {released:?}, not {version}"
                            ),
                        ));
                    }
                    for value in [&mut before, &mut after] {
                        if let Some(object) = value.as_object_mut() {
                            object.remove("version");
                        }
                    }
                    if before != after {
                        found.push((
                            "release",
                            format!("release commit {short} changes package.json beyond version"),
                        ));
                    }
                }
                _ => found.push((
                    "release",
                    format!("release commit {short} has an unreadable package.json"),
                )),
            }
        }

        found
    }

    fn direct(&self, commit: &Commit, parent: &str) -> Found {
        let short = commit.short();
        let mut found = Vec::new();
        let rules = format!("{}/", self.options.rules_directory.trim_end_matches('/'));
        let changed = self.git.changed_paths(parent, &commit.id);
        // Documentation is judged by its text alone. For the other paths: what `gate sync`
        // produces at this commit (generator outputs, tracked files under an owned glob, the
        // core's hook and CI files), and which of it differs there. Asked only when the commit
        // changes a path outside documentation, as most direct commits do not.
        let state = if changed.iter().all(|file| self.documentation.is_match(file)) {
            SyncState::default()
        } else {
            self.git.sync_state(&commit.id)
        };
        // A path the commit removes (a deletion, or the old side of a rename: changed paths are
        // listed without rename detection) is judged against what `gate sync` produced at the
        // parent. Asked only when such a path is not already settled at the commit.
        let mut before: Option<SyncState> = None;

        for file in changed {
            if !self.documentation.is_match(&file) {
                if state.drift.contains(&file) {
                    found.push((
                        "direct",
                        format!(
                            "direct commit {short} changes {file}, which differs from gate sync at that commit"
                        ),
                    ));
                } else if state.produced.contains(&file) {
                    // Generated and in sync: it changes with its sources.
                } else if self.git.read_at(&commit.id, &file).is_none()
                    && before
                        .get_or_insert_with(|| self.git.sync_state(parent))
                        .produced
                        .contains(&file)
                {
                    // Generated at the parent and gone with its sources: `gate sync` removes it.
                } else {
                    found.push((
                        "direct",
                        format!(
                            "direct commit {short} changes code path {file}; code reaches main through a branch merge"
                        ),
                    ));
                }
            } else if file.starts_with(&rules) && is_markdown(&file) {
                match (self.text(parent, &file), self.text(&commit.id, &file)) {
                    (Ok(before), Ok(after)) => {
                        let (before, after) = (before.unwrap_or_default(), after.unwrap_or_default());
                        if metadata_lines(&file, &before) != metadata_lines(&file, &after) {
                            found.push((
                                "direct",
                                format!(
                                    "direct commit {short} changes rule Enforcement/Status lines in {file}; that is code"
                                ),
                            ));
                        }
                    }
                    _ => found.push((
                        "direct",
                        format!(
                            "direct commit {short} changes {file}, which is not UTF-8 text; its Enforcement/Status lines cannot be compared"
                        ),
                    )),
                }
            }
        }

        found
    }

    /// The text of `path` at `commit`, its trailing white space removed, as `git show` printed it;
    /// `Ok(None)` when the file is absent there.
    ///
    /// # Errors
    /// [`NotText`] when the file is not UTF-8 text.
    fn text(&self, commit: &str, path: &str) -> std::result::Result<Option<String>, NotText> {
        self.git
            .read_at(commit, path)
            .map(|bytes| {
                String::from_utf8(bytes)
                    .map(|text| text.trim_end().to_owned())
                    .map_err(|_| NotText)
            })
            .transpose()
    }

    /// The JSON of `path` at `commit`; `None` when absent, not UTF-8 or not JSON.
    fn json(&self, commit: &str, path: &str) -> Option<Value> {
        self.text(commit, path)
            .ok()
            .flatten()
            .and_then(|text| serde_json::from_str(&text).ok())
    }
}

/// A file that is not UTF-8 text.
#[derive(Debug)]
struct NotText;

/// A `.md` file, the extension in any case.
fn is_markdown(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

/// Each rule id of a rule file and its Enforcement/Status line.
fn metadata_lines(path: &str, text: &str) -> BTreeMap<String, Option<String>> {
    parse_rule_file(path, text)
        .rules
        .into_iter()
        .map(|rule| (rule.id, rule.metadata.map(|(_, line)| line)))
        .collect()
}

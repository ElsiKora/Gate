//! Findings: what rules report, after the core has given them a level and a norm.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::baseline::{BASELINE_FILE, Baseline, BaselineEntry};
use crate::config::{CORE_PLUGIN, RuleId, RuleKey, ScopedException, Settings};

/// How much a finding matters. `off` is a configuration setting, never a finding's level.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Fails the gate.
    Error,
    /// Reported; does not fail the gate.
    Warning,
    /// Informs.
    Note,
}

impl fmt::Display for Level {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        })
    }
}

/// A span of lines and columns, 1-based and inclusive.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Span {
    /// The first line.
    pub line: u32,
    /// The first column.
    pub column: u32,
    /// The last line.
    pub end_line: u32,
    /// The last column.
    pub end_column: u32,
}

/// Where a finding is: a file, in JSON `{"path": .., "span": ..}`, or a commit a history rule
/// reports on, in JSON `{"commit": "<full id>"}`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(untagged)]
pub enum Location {
    /// A file.
    File {
        /// The path, relative to the repository root.
        path: String,
        /// The span in the file, when the finding has one.
        span: Option<Span>,
    },
    /// A commit.
    Commit {
        /// The commit's full id.
        commit: String,
    },
}

/// The digits of a commit id that text output shows.
pub const SHORT_ID: usize = 7;

impl Location {
    /// A file location.
    #[must_use]
    pub fn file(path: impl Into<String>, span: Option<Span>) -> Self {
        Self::File {
            path: path.into(),
            span,
        }
    }

    /// The file's path; `None` for a commit.
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::File { path, .. } => Some(path),
            Self::Commit { .. } => None,
        }
    }

    /// The span in the file; `None` for a commit or a whole file.
    #[must_use]
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::File { span, .. } => *span,
            Self::Commit { .. } => None,
        }
    }

    /// The path, or the commit's full id: what fingerprints, baselines and sorting use.
    #[must_use]
    pub fn key(&self) -> &str {
        match self {
            Self::File { path, .. } => path,
            Self::Commit { commit } => commit,
        }
    }
}

/// One finding.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Finding {
    /// The rule id, `<plugin>/<rule>`.
    pub rule: String,
    /// The aspect of the rule; `""` when the rule has one.
    pub aspect: String,
    /// The level configuration gives the rule's aspect.
    pub level: Level,
    /// What is wrong.
    pub message: String,
    /// Where it is.
    pub location: Option<Location>,
    /// What the finding is about, in the rule's documented grammar (for example
    /// `src/user.entity.ts:UserEntity.parent`, paths relative to the project root); exceptions
    /// match it. `None` when the finding is about its path alone.
    pub target: Option<String>,
    /// How to fix it.
    pub help: Option<String>,
    /// The repository's written rule the finding enforces, from configuration.
    pub norm: Option<String>,
    /// The project the finding belongs to; `None` for repository-scope rules.
    pub project: Option<String>,
    /// The plugin's short, stable identity of what the finding is about within its rule, aspect and
    /// path; with one, the fingerprint is made of it instead of the message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl Finding {
    /// A finding of `rule` at `level`, with nothing else set.
    #[must_use]
    pub fn new(rule: &RuleId, level: Level, message: impl Into<String>) -> Self {
        Self {
            rule: rule.to_string(),
            aspect: String::new(),
            level,
            message: message.into(),
            location: None,
            target: None,
            help: None,
            norm: None,
            project: None,
            key: None,
        }
    }

    /// The rule id with the aspect: `<plugin>/<rule>#<aspect>`, or the rule id alone.
    #[must_use]
    pub fn rule_with_aspect(&self) -> String {
        if self.aspect.is_empty() {
            self.rule.clone()
        } else {
            format!("{}#{}", self.rule, self.aspect)
        }
    }

    /// A stable id of the finding for baselines and SARIF: a digest of rule, aspect, path and the
    /// plugin's key, or the message when the plugin gives no key, so it survives moved lines (and,
    /// with a key, a reworded message).
    #[must_use]
    pub fn fingerprint(&self) -> String {
        fingerprint(
            &self.rule,
            &self.aspect,
            self.location.as_ref().map_or("", Location::key),
            self.identity(),
        )
    }

    /// What the fingerprint digests besides rule, aspect and path: the key, else the message.
    #[must_use]
    pub fn identity(&self) -> Identity<'_> {
        match &self.key {
            Some(key) => Identity::Key(key),
            None => Identity::Message(&self.message),
        }
    }

    fn sort_key(&self) -> (&str, Option<Span>, &str, &str, &str) {
        (
            self.location.as_ref().map_or("", Location::key),
            self.location.as_ref().and_then(Location::span),
            self.rule.as_str(),
            self.aspect.as_str(),
            self.message.as_str(),
        )
    }
}

/// What identifies a finding within its rule, aspect and path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Identity<'a> {
    /// The plugin's stable key.
    Key(&'a str),
    /// The message, when the plugin gives no key.
    Message(&'a str),
}

/// The fingerprint of a finding's rule, aspect, path and identity: the first 16 bytes of a sha256,
/// as 32 hex digits. Without a key it digests rule, aspect, path and message, each followed by a zero
/// byte. With a key it digests the word `key`, rule, aspect, path and key, each after its length in
/// eight bytes, big-endian; that input starts with a zero byte, which no rule id does, so a keyed
/// fingerprint never equals an unkeyed one, and no part can run into the next.
#[must_use]
pub fn fingerprint(rule: &str, aspect: &str, path: &str, identity: Identity<'_>) -> String {
    let mut digest = Sha256::new();
    match identity {
        Identity::Message(message) => {
            for part in [rule, aspect, path, message] {
                digest.update(part.as_bytes());
                digest.update([0]);
            }
        }
        Identity::Key(key) => {
            for part in ["key", rule, aspect, path, key] {
                digest.update(u64::try_from(part.len()).unwrap_or(u64::MAX).to_be_bytes());
                digest.update(part.as_bytes());
            }
        }
    }
    digest
        .finalize()
        .iter()
        .take(16)
        .fold(String::with_capacity(32), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// The deterministic order of findings: path, line, column, rule, aspect, message.
impl Ord for Finding {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key()
            .cmp(&other.sort_key())
            .then_with(|| self.level.cmp(&other.level))
            .then_with(|| self.project.cmp(&other.project))
            .then_with(|| self.target.cmp(&other.target))
            .then_with(|| self.help.cmp(&other.help))
            .then_with(|| self.norm.cmp(&other.norm))
            .then_with(|| self.key.cmp(&other.key))
    }
}

impl PartialOrd for Finding {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The sink rules report into. Order of arrival does not matter: [`Findings::finish`] sorts.
#[derive(Debug, Default)]
pub struct Findings {
    items: Vec<Finding>,
}

impl Findings {
    /// An empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one finding.
    pub fn push(&mut self, finding: Finding) {
        self.items.push(finding);
    }

    /// The number of findings so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether no finding was added.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Whether a finding at `error` was added (for `--fail-fast`).
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.items
            .iter()
            .any(|finding| finding.level == Level::Error)
    }

    /// Applies exceptions, then the baseline, and sorts.
    ///
    /// A finding that an exception matches is dropped; so is a finding whose fingerprint is in the
    /// baseline under the finding's own project (each entry accepts one finding). An exception or
    /// baseline entry that is due and matched nothing becomes a `gate/exceptions` or
    /// `gate/baseline` error, so both only ever shrink. One is due when its rule ran in its scope
    /// (`ran`) and judged what it covers there: the rule's whole scope, or, for a rule that
    /// narrowed its view, the exception's target or the entry's key among the targets it judged
    /// (an entry without a key is due whenever its rule ran).
    #[must_use]
    pub fn finish(
        self,
        exceptions: &[ScopedException],
        baseline: &Baseline,
        ran: &RulesRan,
    ) -> Vec<Finding> {
        let mut used = vec![false; exceptions.len()];
        let mut kept: Vec<Finding> = self
            .items
            .into_iter()
            .filter(|finding| {
                let mut matched = false;
                for (index, exception) in exceptions.iter().enumerate() {
                    if matches(exception, finding) {
                        used[index] = true;
                        matched = true;
                    }
                }
                !matched
            })
            .collect();

        let accepted = baseline.accept(&kept);
        let mut flags = accepted.findings.iter();
        kept.retain(|_| !flags.next().copied().unwrap_or(false));

        for (exception, used) in exceptions.iter().zip(used) {
            let rule = exception.rule.rule.to_string();
            let target = Some(exception.target.as_str());
            let due = ran.judged(exception.project.as_deref(), &rule, target)
                || (exception.is_root() && ran.judged(None, &rule, target));
            if !used && due {
                kept.push(unmatched(exception));
            }
        }

        for entry in baseline.stale(&accepted, ran) {
            kept.push(stale(entry));
        }

        kept.sort();
        kept.dedup();
        kept
    }
}

impl Findings {
    /// Drops the findings of plugin rules `filter` leaves out; the core's own stay.
    pub fn retain_rules(&mut self, filter: &RuleFilter) {
        self.items.retain(|finding| filter.allows(&finding.rule));
    }
}

impl Extend<Finding> for Findings {
    fn extend<I: IntoIterator<Item = Finding>>(&mut self, iterator: I) {
        self.items.extend(iterator);
    }
}

/// The rules that ran to completion, by scope: the plugin runtime records each (scope, rule) whose
/// calls all finished, so a rule that is off, not selected, or cut short by `--fail-fast` is not
/// in it, and its exceptions and baseline entries are not due. A rule that narrowed its view
/// (`Run::judges_only`) is recorded with the targets it judged.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RulesRan {
    pairs: BTreeSet<(Option<String>, String)>,
    narrowed: BTreeMap<(Option<String>, String), BTreeSet<String>>,
}

impl RulesRan {
    /// No rule ran.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `rule` ran for `project` (`None`: the repository scope).
    pub fn insert(&mut self, project: Option<&str>, rule: &str) {
        self.pairs
            .insert((project.map(str::to_owned), rule.to_owned()));
    }

    /// Whether `rule` ran for `project`.
    #[must_use]
    pub fn ran(&self, project: Option<&str>, rule: &str) -> bool {
        self.pairs
            .contains(&(project.map(str::to_owned), rule.to_owned()))
    }

    /// Records that `rule`, for `project`, judged only `targets`, not its whole scope.
    pub fn narrow(&mut self, project: Option<&str>, rule: &str, targets: Vec<String>) {
        self.narrowed
            .entry((project.map(str::to_owned), rule.to_owned()))
            .or_default()
            .extend(targets);
    }

    /// Whether `rule` ran for `project` and judged `target` there: its whole scope, or `target`
    /// among the targets it narrowed its view to. No target (a baseline entry without a key, which
    /// cannot be told apart) counts as judged whenever the rule ran.
    #[must_use]
    pub fn judged(&self, project: Option<&str>, rule: &str, target: Option<&str>) -> bool {
        if !self.ran(project, rule) {
            return false;
        }
        match self
            .narrowed
            .get(&(project.map(str::to_owned), rule.to_owned()))
        {
            None => true,
            Some(targets) => target.is_none_or(|target| targets.contains(target)),
        }
    }
}

impl RulesRan {
    /// Forgets the rules `filter` leaves out, so their exceptions and baseline entries are not due.
    pub fn retain_rules(&mut self, filter: &RuleFilter) {
        self.pairs.retain(|(_, rule)| filter.allows(rule));
        self.narrowed.retain(|(_, rule), _| filter.allows(rule));
    }
}

/// `gate check --rule`: the rules a run is narrowed to, each `<plugin>/<rule>` or `<plugin>/*`.
/// Empty: every rule. The core's own rules always run.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuleFilter {
    keys: Vec<RuleKey>,
}

impl RuleFilter {
    /// A filter of `keys`; no key: every rule.
    #[must_use]
    pub fn new(keys: Vec<RuleKey>) -> Self {
        Self { keys }
    }

    /// Parses one `--rule` value: `<plugin>/<rule>` or `<plugin>/*`.
    ///
    /// # Errors
    /// Fails, saying what is expected, for an aspect or anything that is not such a key.
    pub fn parse(value: &str) -> Result<RuleKey, String> {
        if value.contains('#') {
            return Err(format!(
                "{value:?}: --rule narrows a run to whole rules; write <plugin>/<rule> or <plugin>/*, without #<aspect>"
            ));
        }
        RuleKey::parse(value)
    }

    /// The keys, as given.
    #[must_use]
    pub fn keys(&self) -> &[RuleKey] {
        &self.keys
    }

    /// Whether the run is narrowed at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Whether `rule` (`<plugin>/<rule>`) runs.
    #[must_use]
    pub fn allows(&self, rule: &str) -> bool {
        let plugin = rule.split_once('/').map_or(rule, |(plugin, _)| plugin);
        self.keys.is_empty()
            || plugin == CORE_PLUGIN
            || self.keys.iter().any(|key| match key {
                RuleKey::Family(family) => family == plugin,
                RuleKey::Rule(id) => id.to_string() == rule,
            })
    }

    /// Whether any rule of `plugin` runs.
    #[must_use]
    pub fn allows_plugin(&self, plugin: &str) -> bool {
        self.keys.is_empty() || self.keys.iter().any(|key| key.plugin() == plugin)
    }
}

/// Gives the core's own findings the `norm` and `norms` the root `[rules]` maps them to (their
/// level stays the core's). Findings that already carry a norm keep it.
pub fn attach_core_norms(findings: &mut [Finding], root: &Settings) {
    for finding in findings {
        if finding.norm.is_some() || !is_core_rule(&finding.rule) {
            continue;
        }
        if let Ok(rule) = RuleId::parse(&finding.rule) {
            finding.norm = root.norm(&rule, &finding.aspect).map(str::to_owned);
        }
    }
}

fn is_core_rule(rule: &str) -> bool {
    rule.split_once('/')
        .is_some_and(|(plugin, _)| plugin == CORE_PLUGIN)
}

/// The id of the pseudo-rule that reports exceptions that match nothing.
pub const UNMATCHED_EXCEPTION_RULE: &str = "gate/exceptions";

/// The id of the pseudo-rule that reports baseline entries that match nothing.
pub const STALE_BASELINE_RULE: &str = "gate/baseline";

/// The id of the pseudo-rule that reports a run over its `--warn-after` or `--fail-after`.
pub const BUDGET_RULE: &str = "gate/budget";

/// Whether `exception` accepts `finding`: same scope (the project whose file declares the
/// exception; the root's file also accepts repository-scope findings), same rule, same aspect when the exception names
/// one, and the exception's target equals the finding's `target`, or its path when the finding has
/// no target.
#[must_use]
pub fn matches(exception: &ScopedException, finding: &Finding) -> bool {
    (finding.project == exception.project || (finding.project.is_none() && exception.is_root()))
        && finding.rule == exception.rule.rule.to_string()
        && exception
            .rule
            .aspect
            .as_ref()
            .is_none_or(|aspect| *aspect == finding.aspect)
        && match &finding.target {
            Some(target) => *target == exception.target,
            None => finding
                .location
                .as_ref()
                .is_some_and(|location| location.path() == exception.path.as_deref()),
        }
}

fn unmatched(exception: &ScopedException) -> Finding {
    let target = &exception.target;
    let line = u32::try_from(exception.line).unwrap_or(u32::MAX);
    Finding {
        rule: UNMATCHED_EXCEPTION_RULE.to_owned(),
        aspect: String::new(),
        level: Level::Error,
        message: format!(
            "the exception for {} at {target} matches no finding",
            exception.rule
        ),
        location: Some(Location::file(
            exception.file.clone(),
            Some(Span {
                line,
                column: 1,
                end_line: line,
                end_column: 1,
            }),
        )),
        target: None,
        help: Some("remove the exception; the finding it accepted is gone".to_owned()),
        norm: None,
        project: None,
        key: None,
    }
}

fn stale(entry: &BaselineEntry) -> Finding {
    Finding {
        rule: STALE_BASELINE_RULE.to_owned(),
        aspect: String::new(),
        level: Level::Error,
        message: format!(
            "the baseline entry for {} at {} matches no finding",
            entry.rule_with_aspect(),
            entry.path.as_deref().unwrap_or("(no path)")
        ),
        location: Some(Location::file(BASELINE_FILE, None)),
        target: None,
        help: Some(
            "run gate baseline prune to drop it; the finding it accepted is gone".to_owned(),
        ),
        norm: None,
        project: None,
        key: None,
    }
}

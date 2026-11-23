//! The core of Gate: configuration, projects, snapshots, change selection, findings and reports.
//!
//! The core holds no rule. A run opens a [`Workspace`] (the [`Snapshot`] of tracked files, the
//! loaded configuration, the discovered [`Projects`] and the [`Settings`] of each), selects the
//! projects a change touches ([`Selection`]), collects what plugins report into [`Findings`],
//! applies exceptions and the [`Baseline`], and renders a [`Report`].
//!
//! What the plugin runtime uses:
//!
//! - [`Snapshot`]: tracked paths ([`Snapshot::paths`], [`Snapshot::under`]), contents by path
//!   ([`Snapshot::read`]), blob ids ([`Snapshot::blob_id`]).
//! - [`Projects`]: names, roots ([`Project::root`]), ownership ([`Projects::owner`]).
//! - [`Workspace::settings`] and [`Workspace::repository`]: plugins, presets and rules per scope;
//!   [`Settings::setting`], [`Settings::norm`] and [`Settings::options`] give a finding its level,
//!   norm and the rule its options; [`Workspace::validate`] checks them against what plugins
//!   declare.
//! - [`ChangeSet`] and [`Selection`]: the changed paths and the projects that run.
//! - [`Findings`]: the sink.

pub mod baseline;
pub mod cache;
pub mod changes;
pub mod cli;
pub mod config;
pub mod doctor;
pub mod error;
pub mod fetch;
pub mod files;
pub mod findings;
pub mod git;
pub mod lock;
pub mod plugins;
pub mod projects;
pub mod report;
pub mod run;
pub mod snapshot;
pub mod sync;
pub mod tools;
pub mod workspace;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::plugins::Loading;

pub use baseline::{BASELINE_FILE, Baseline, BaselineEntry};
pub use cache::{Cache, Sha256Sum};
pub use changes::{ChangeSet, Decision, PushRange, Reason, Selection};
pub use config::{
    Config, ConfigFile, Kind, PluginName, PluginSettings, RuleId, SchemaValidator, ScopedException,
    Setting, SettingOrigin, Settings,
};
pub use error::{Error, ErrorKind, Result};
pub use fetch::{FetchConfig, Fetcher};
pub use findings::{
    Finding, Findings, Finished, Level, Location, RuleFilter, RulesRan, Span, attach_core_norms,
};
pub use git::Git;
pub use lock::{LOCK_FILE, Lock, Pin, Wanted};
pub use plugins::{
    Generated, GitAnswers, JsonSchemaValidator, Loads, PluginGenerators, PluginReads,
    PresetSetting, PresetSettings, SyncStates, preset_settings,
};
pub use projects::{Origin, Project, Projects};
pub use report::{EXIT_CLEAN, EXIT_ERRORS, EXIT_FAILURE, Format, Report, SelectionFormat};
pub use snapshot::{BlobId, Snapshot, Source};
pub use workspace::{Inputs, PresetOptions, RuleDeclaration, RuleScope, Workspace};

/// Which paths count as changed in a run.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ChangeRequest {
    /// None: every project runs.
    #[default]
    All,
    /// The index against `HEAD` (`--staged`).
    Staged,
    /// Since the common ancestor with a base revision (`--changed --base <rev>`).
    Since(String),
    /// A pushed range (`--push <base>..<tip>`), and the remote ref it is pushed to
    /// (`--target-ref <ref>`, which only `--push` takes).
    Push {
        /// `<base>..<tip>`.
        range: String,
        /// The remote ref, when the caller knows it: the pre-push hook's remote ref, the pull
        /// request's base branch or the pushed ref in CI.
        target_ref: Option<String>,
    },
}

/// Options passed through to the run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RunOptions {
    /// Print the time of each phase and plugin call.
    pub timings: bool,
    /// A run that takes longer, not counting compiling plugins, is a `gate/budget` warning.
    pub warn_after: Option<Duration>,
    /// A run that takes longer, not counting compiling plugins, is a `gate/budget` error; above
    /// `warn_after` when both are set.
    pub fail_after: Option<Duration>,
    /// Stop at the first error finding.
    pub fail_fast: bool,
}

/// `--plugin <name>=<path>`: a local build that replaces one pinned plugin for one run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginOverride {
    /// The plugin it replaces, by the name `[plugins]` gives it.
    pub name: PluginName,
    /// The `.wasm` file.
    pub path: PathBuf,
}

impl PluginOverride {
    /// Parses `<name>=<path>`.
    ///
    /// # Errors
    /// Fails, saying what is expected, when the value has no `=`, the name is not a plugin name, or
    /// the path is empty.
    pub fn parse(value: &str) -> std::result::Result<Self, String> {
        let (name, path) = value.split_once('=').ok_or_else(|| {
            format!("{value:?} is not <name>=<path>; for example layers=target/layers.wasm")
        })?;
        let name = PluginName::try_from(name.to_owned())?;
        if path.is_empty() {
            return Err(format!("{value:?}: the path after = is empty"));
        }
        Ok(Self {
            name,
            path: PathBuf::from(path),
        })
    }
}

/// What `gate check` was asked to do.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CheckRequest {
    /// Where contents come from. Ignored for [`ChangeRequest::Push`], whose tip is the source.
    pub source: Option<Source>,
    /// Which paths count as changed.
    pub changes: ChangeRequest,
    /// Options passed through.
    pub options: RunOptions,
    /// Plugins replaced for this run.
    pub overrides: Vec<PluginOverride>,
    /// Refuse every override (the generated CI jobs pass it).
    pub no_overrides: bool,
    /// Only select the projects (`--list-changed-projects`): no plugin runs, so none is required.
    pub list_changed_projects: bool,
    /// The commit message under review (`--commit-message <file>`, read by the caller), for the
    /// rules that check it.
    pub commit_message: Option<String>,
    /// A git hook started the run (`--hook`, which the generated hooks pass).
    pub hook: bool,
    /// The rules the run is narrowed to (`--rule`), and those it leaves out after that
    /// (`--skip-rule`); empty: every rule.
    pub rules: RuleFilter,
}

/// One phase of a run and how long it took.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Timing {
    /// The phase.
    pub phase: String,
    /// How long it took.
    pub duration: Duration,
}

/// The result of [`check`]: the workspace, the report, the timings and what the plugins read.
#[derive(Debug)]
pub struct Checked {
    /// The opened workspace.
    pub workspace: Workspace,
    /// The report.
    pub report: Report,
    /// The time of each phase.
    pub timings: Vec<Timing>,
    /// What each plugin call read, with repository paths: the read-set record of the result
    /// cache, as the workspace's inputs record what its configuration was read from.
    pub reads: Vec<PluginReads>,
    /// The `sync-state` answers the run computed and read from the cache.
    pub sync_states: SyncStates,
    /// The git questions about commit ids the run asked git and answered from stored answers.
    pub git_answers: GitAnswers,
    /// The generators `sync-state` ran, and those the result cache answered.
    pub generated: Generated,
    /// What the first cache write that failed said, when one did: the check is complete, but
    /// the next run cannot reuse its work.
    pub cache_warning: Option<String>,
    /// How the plugins loaded: each one loaded, those every call of which came from the cache,
    /// and the time compiling took.
    pub loads: Loads,
}

/// Runs `gate check` on the repository that contains `start`.
///
/// # Errors
/// Fails on a configuration or environment failure, on an override that is refused or names no
/// declared plugin, on a pinned plugin that cannot be fetched or verified, on options that break
/// their rule's schema, and on a plugin call that fails.
pub fn check(start: &Path, request: &CheckRequest) -> Result<Checked> {
    let started = Instant::now();
    let run = run(start, request)?;
    let mut timings = run.timings;
    let workspace = run.workspace;
    let selection = run.selection;
    // Rules outside --rule or under --skip-rule never count as run, and their findings are
    // dropped, whatever a plugin reports.
    let mut ran = run.ran.per_scope.clone();
    ran.retain_rules(&request.rules);
    let mut found = run.findings;
    found.retain_rules(&request.rules);
    let finished = found.finish_counted(workspace.exceptions(), workspace.baseline(), &ran);
    let mut findings = finished.findings;

    if let Some(finding) = over_budget(started.elapsed(), &run.ran.loads, &request.options) {
        findings.push(finding);
        findings.sort();
    }
    attach_core_norms(&mut findings, workspace.repository());

    let report = Report {
        mode: run.mode,
        rules: run.ran.rules,
        findings,
        baselined: finished.baselined,
        selection,
        overrides: run.overrides,
        skipped_rules: run.ran.skipped,
    };
    timings.push(Timing {
        phase: "total".to_owned(),
        duration: started.elapsed(),
    });

    Ok(Checked {
        workspace,
        report,
        timings,
        reads: run.ran.reads,
        sync_states: run.ran.sync_states,
        git_answers: run.ran.git_answers,
        generated: run.ran.generated,
        cache_warning: run.ran.cache_warning,
        loads: run.ran.loads,
    })
}

/// What `gate baseline write` wrote.
#[derive(Debug)]
pub struct BaselineWritten {
    /// The baseline.
    pub baseline: Baseline,
    /// The file, on disk.
    pub path: PathBuf,
    /// Whether git tracks the file (gate reads only tracked files).
    pub tracked: bool,
}

/// Runs a full check of the working tree and writes the fingerprints of its findings (after
/// exceptions, without the core's own) to `gate/baseline.json`, replacing it: for the first
/// baseline. Once findings are fixed, [`prune_baseline`] drops their entries and adds none. The
/// file is replaced whole while the run holds the baseline's guard ([`lock::Guard::take_baseline`]),
/// so a reader never sees part of it and a prune running meanwhile never overwrites it.
///
/// # Errors
/// Fails like [`check`], when the guard cannot be taken, and when the file cannot be written.
pub fn write_baseline(start: &Path) -> Result<BaselineWritten> {
    let request = CheckRequest {
        source: Some(Source::WorkTree),
        ..CheckRequest::default()
    };
    let run = run(start, &request)?;
    let findings = run.findings.finish(
        run.workspace.exceptions(),
        &Baseline::default(),
        &run.ran.per_scope,
    );
    let baseline = Baseline::from_findings(&findings);
    let root = run.workspace.snapshot().git().root().to_path_buf();
    let path = root.join(BASELINE_FILE);

    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).map_err(|error| {
            Error::environment(format!(
                "cannot create {}: {error}; check that {} is writable, then run gate baseline write again",
                directory.display(),
                root.display()
            ))
        })?;
    }
    let guard = lock::Guard::take_baseline(run.workspace.snapshot().git())?;
    files::replace(&path, baseline.to_json()?.as_bytes())?;
    drop(guard);
    let tracked = !run
        .workspace
        .snapshot()
        .git()
        .paths(&["ls-files", "-z", "--", BASELINE_FILE])?
        .is_empty();

    Ok(BaselineWritten {
        baseline,
        path,
        tracked,
    })
}

/// What `gate baseline prune` did.
#[derive(Debug)]
pub struct BaselinePruned {
    /// The entries it re-keyed, as they are now: written without a key for a finding that now has
    /// one.
    pub rekeyed: Vec<BaselineEntry>,
    /// The entries it removed: those that matched no finding although their rule ran.
    pub removed: Vec<BaselineEntry>,
    /// The entries the file holds now.
    pub kept: usize,
    /// The findings no entry accepts, which `gate check` reports; prune does not add them.
    pub unaccepted: usize,
    /// The file, on disk.
    pub path: PathBuf,
}

/// Runs a full check of the working tree and prunes `gate/baseline.json` against its findings: an
/// entry written without a key for a finding its plugin now keys is re-keyed, and the entries
/// `gate check` reports as stale (they match no finding although their rule ran, an aspect set to
/// `off` included) are removed. It never adds an entry, so a new finding stays reported. The file is
/// rewritten only when an entry changes. The check runs first; then, holding the baseline's guard
/// ([`lock::Guard::take_baseline`]), the run reads the file again and replaces it whole. It removes
/// or re-keys only entries the file held when the run started, which its check judged; an entry
/// another run wrote meanwhile is kept as it is, never overwritten or judged against a tree it may
/// not come from.
///
/// # Errors
/// Fails like [`check`], when the baseline does not load, when `gate/baseline.json` exists but git
/// does not track it (gate would read none of it; checked again under the guard), when the guard
/// cannot be taken, and when the file cannot be read or written.
pub fn prune_baseline(start: &Path) -> Result<BaselinePruned> {
    let request = CheckRequest {
        source: Some(Source::WorkTree),
        ..CheckRequest::default()
    };
    let run = run(start, &request)?;
    let root = run.workspace.snapshot().git().root().to_path_buf();
    let path = root.join(BASELINE_FILE);
    let tracked = !run
        .workspace
        .snapshot()
        .git()
        .paths(&["ls-files", "-z", "--", BASELINE_FILE])?
        .is_empty();
    let untracked = || {
        Error::config(format!(
            "{BASELINE_FILE} exists but git does not track it, and gate reads tracked files only; run git add {BASELINE_FILE} first, then gate baseline prune"
        ))
    };
    if path.exists() && !tracked {
        return Err(untracked());
    }
    let findings = run.findings.finish(
        run.workspace.exceptions(),
        &Baseline::default(),
        &run.ran.per_scope,
    );
    // Read under the guard: a baseline another run wrote while this one checked is the starting
    // point, and of it only the entries this run's check judged are pruned.
    let guard = lock::Guard::take_baseline(run.workspace.snapshot().git())?;
    let current = match std::fs::read_to_string(&path) {
        Ok(_) if !tracked => return Err(untracked()),
        Ok(text) => Baseline::parse(&text)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Baseline::default(),
        Err(error) => {
            return Err(Error::environment(format!(
                "cannot read {}: {error}; check that it is readable, then run gate baseline prune again",
                path.display()
            )));
        }
    };
    let pruned = current.pruned_since(run.workspace.baseline(), &findings, &run.ran.per_scope);
    let unaccepted = pruned
        .baseline
        .accept(&findings)
        .findings
        .iter()
        .zip(&findings)
        .filter(|(accepted, finding)| !**accepted && !baseline::is_core(&finding.rule))
        .count();
    if !pruned.removed.is_empty() || !pruned.rekeyed.is_empty() {
        files::replace(&path, pruned.baseline.to_json()?.as_bytes())?;
    }
    drop(guard);

    Ok(BaselinePruned {
        kept: pruned.baseline.len(),
        rekeyed: pruned.rekeyed,
        removed: pruned.removed,
        unaccepted,
        path,
    })
}

struct Run {
    workspace: Workspace,
    selection: Selection,
    findings: Findings,
    ran: plugins::Ran,
    mode: String,
    overrides: Vec<String>,
    timings: Vec<Timing>,
}

fn run(start: &Path, request: &CheckRequest) -> Result<Run> {
    let mut timings = Vec::new();
    let mut phase = |name: &str, since: Instant| {
        timings.push(Timing {
            phase: name.to_owned(),
            duration: since.elapsed(),
        });
    };

    if request.no_overrides
        && let Some(first) = request.overrides.first()
    {
        return Err(Error::config(format!(
            "--plugin {}={} is refused by --no-overrides; the gate jobs check the pinned plugins only, drop one of the two",
            first.name,
            first.path.display()
        )));
    }

    let git = Git::discover(start)?;
    let mut changes = None;
    let mut pushed = None;
    let source = match &request.changes {
        ChangeRequest::Push { range, target_ref } => {
            let (set, range) = ChangeSet::push(&git, range, target_ref.as_deref())?;
            changes = Some(set);
            pushed = Some(range.clone());
            Source::Commit(range.tip)
        }
        _ => request.source.clone().unwrap_or(match request.changes {
            ChangeRequest::Staged => Source::Index,
            _ => Source::WorkTree,
        }),
    };

    let at = Instant::now();
    let snapshot = Snapshot::open(&git, source.clone())?;
    phase("snapshot", at);

    let at = Instant::now();
    let workspace = Workspace::from_snapshot(snapshot)?;
    lock::check_core(&workspace, lock::CORE_VERSION)?;
    let (overrides, files) = resolve_overrides(&workspace, &request.overrides)?;
    let lock = lock::verify_with(&workspace, &files.keys().cloned().collect())?;
    phase("configuration", at);

    let at = Instant::now();
    match &request.changes {
        ChangeRequest::All | ChangeRequest::Push { .. } => {}
        ChangeRequest::Staged => changes = Some(ChangeSet::staged(&git)?),
        ChangeRequest::Since(base) => changes = Some(ChangeSet::since(&git, base, &source)?),
    }
    let selection = match &changes {
        None => Selection::full(workspace.projects()),
        Some(set) => Selection::select(workspace.projects(), set, workspace.inputs()),
    };
    phase("selection", at);

    let at = Instant::now();
    let mut findings = Findings::new();
    let loaded = if request.list_changed_projects {
        None
    } else {
        // Only the plugins a selected scope declares and --rule allows run, each when a call of it
        // misses the result cache; every declared plugin's declaration is checked (see load).
        let needed = plugins::needed(&workspace, &selection, &request.rules);
        plugins::load(&workspace, lock.as_ref(), &files, &needed, Loading::OnCall)?.map(Arc::new)
    };
    phase("load", at);
    let at = Instant::now();
    let ran = match loaded {
        None => {
            // No plugin is loaded: --rule and --skip-rule know only the rules [rules] names.
            workspace.check_rule_filter(&request.rules, &|_, _, _| None)?;
            plugins::Ran::default()
        }
        Some(loaded) => {
            loaded.validate(&workspace, &request.rules)?;
            let range = commit_range(&git, &source, &request.changes, pushed)?;
            loaded.run(
                &workspace,
                &selection,
                changes.as_ref(),
                plugins::RunGit {
                    range,
                    message: request.commit_message.clone(),
                    hook: request.hook,
                },
                &request.rules,
                &mut findings,
            )?
        }
    };
    phase("plugins", at);

    let mode = mode(
        &source,
        changes.as_ref(),
        &request.changes,
        request.commit_message.is_some(),
    );
    Ok(Run {
        workspace,
        selection,
        findings,
        ran,
        mode,
        overrides,
        timings,
    })
}

/// The files of the overrides, by plugin name.
type OverrideFiles = BTreeMap<PluginName, (PathBuf, Vec<u8>)>;

/// Checks each override names a declared plugin and reads its file: the report's lines
/// (`<name>=<path> sha256 <hex>`) and the files.
fn resolve_overrides(
    workspace: &Workspace,
    overrides: &[PluginOverride],
) -> Result<(Vec<String>, OverrideFiles)> {
    let mut lines = Vec::new();
    let mut files = BTreeMap::new();

    for item in overrides {
        let declared = std::iter::once(workspace.repository())
            .chain(
                workspace
                    .projects()
                    .iter()
                    .filter_map(|project| workspace.settings(&project.name)),
            )
            .any(|settings| settings.plugins.contains_key(&item.name));
        if !declared {
            return Err(Error::config(format!(
                "--plugin {}: no gate.toml declares plugin {}; an override replaces a declared plugin, check the name in [plugins]",
                item.name, item.name
            )));
        }

        let bytes = std::fs::read(&item.path).map_err(|error| {
            Error::environment(format!(
                "--plugin {}={}: cannot read the file: {error}; give the path of a built plugin",
                item.name,
                item.path.display()
            ))
        })?;
        let digest = Sha256::digest(&bytes);
        let hex = digest.iter().fold(String::new(), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        });
        lines.push(format!(
            "{}={} sha256 {hex}",
            item.name,
            item.path.display()
        ));
        files.insert(item.name.clone(), (item.path.clone(), bytes));
    }

    Ok((lines, files))
}

/// The run's range as history rules see it: the pushed range under `--push`; under `--rev`, the
/// revision, with the base of `--changed --base`, else its first parent; a root commit has no
/// base, as a new branch.
fn commit_range(
    git: &Git,
    source: &Source,
    changes: &ChangeRequest,
    pushed: Option<PushRange>,
) -> Result<Option<gate_sdk::CommitRange>> {
    if let Some(pushed) = pushed {
        return Ok(Some(gate_sdk::CommitRange {
            base: pushed.base,
            head: pushed.tip,
            target_ref: pushed.target_ref,
        }));
    }
    let Source::Commit(revision) = source else {
        return Ok(None);
    };
    let resolve = |revision: &str| {
        git.resolve_commit(revision)?.ok_or_else(|| {
            Error::config(format!(
                "{revision:?} names no commit; give a commit, branch or tag"
            ))
        })
    };
    let head = resolve(revision)?;
    let base = match changes {
        ChangeRequest::Since(base) => Some(resolve(base)?),
        _ => git.resolve_commit(&format!("{head}^"))?,
    };
    Ok(Some(gate_sdk::CommitRange {
        base,
        head,
        target_ref: None,
    }))
}

/// What a `gate/budget` finding advises.
const BUDGET_HELP: &str =
    "find the slow step with --timings, or raise --warn-after or --fail-after";

/// The one `gate/budget` finding of a run that took `elapsed`: an error over `--fail-after`, else a
/// warning over `--warn-after`, else none. An error over both thresholds names both. The time of
/// the loads that compiled a plugin (`loads`) is not counted: a first run after a clone, a core
/// upgrade or a plugin bump compiles, and the next runs do not.
fn over_budget(elapsed: Duration, loads: &Loads, options: &RunOptions) -> Option<Finding> {
    let compiling = if loads.compiled > 0 {
        loads.compiling.min(elapsed)
    } else {
        Duration::ZERO
    };
    let counted = elapsed.saturating_sub(compiling);
    let over = |threshold: Option<Duration>| threshold.filter(|threshold| counted > *threshold);
    let took = if compiling.is_zero() {
        format!("the run took {} ms", counted.as_millis())
    } else {
        format!(
            "the run took {} ms, not counting {} ms compiling {} {}",
            counted.as_millis(),
            compiling.as_millis(),
            loads.compiled,
            if loads.compiled == 1 {
                "plugin"
            } else {
                "plugins"
            }
        )
    };
    let warned = over(options.warn_after);
    let (level, message) = match (over(options.fail_after), warned) {
        (Some(fail), Some(warn)) => (
            Level::Error,
            format!(
                "{took}, over --fail-after {} ms (and --warn-after {} ms)",
                fail.as_millis(),
                warn.as_millis()
            ),
        ),
        (Some(fail), None) => (
            Level::Error,
            format!("{took}, over --fail-after {} ms", fail.as_millis()),
        ),
        (None, Some(warn)) => (
            Level::Warning,
            format!("{took}, over --warn-after {} ms", warn.as_millis()),
        ),
        (None, None) => return None,
    };
    Some(Finding {
        rule: findings::BUDGET_RULE.to_owned(),
        aspect: String::new(),
        level,
        message,
        location: None,
        target: None,
        help: Some(BUDGET_HELP.to_owned()),
        norm: None,
        project: None,
        key: None,
    })
}

/// What the run checked, for the summary: the tree, the change or the revision; or `commit
/// message` when the run checked only the message being written (`--commit-message`).
fn mode(
    source: &Source,
    changes: Option<&ChangeSet>,
    request: &ChangeRequest,
    message: bool,
) -> String {
    if message {
        return "commit message".to_owned();
    }
    match (request, changes) {
        (ChangeRequest::Push { .. } | ChangeRequest::Staged, Some(set)) => set.description.clone(),
        (ChangeRequest::Since(_), Some(set)) => match source {
            Source::WorkTree => set.description.clone(),
            other => format!("{other}, {}", set.description),
        },
        _ => match source {
            Source::Commit(revision) => format!("rev {}", short(revision)),
            other => other.to_string(),
        },
    }
}

fn short(revision: &str) -> &str {
    if revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        &revision[..12]
    } else {
        revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(warn: u64, fail: u64) -> RunOptions {
        RunOptions {
            timings: false,
            warn_after: Some(Duration::from_millis(warn)),
            fail_after: Some(Duration::from_millis(fail)),
            fail_fast: false,
        }
    }

    fn compiled(plugins: usize, millis: u64) -> Loads {
        Loads {
            compiled: plugins,
            compiling: Duration::from_millis(millis),
            ..Loads::default()
        }
    }

    #[test]
    fn a_run_that_compiled_is_measured_without_its_compile_time() {
        let elapsed = Duration::from_millis(5840);
        assert_eq!(
            over_budget(elapsed, &compiled(9, 5560), &options(3000, 10_000)),
            None
        );

        let finding = over_budget(elapsed, &compiled(9, 4000), &options(1000, 10_000))
            .expect("over --warn-after without the compile time");
        assert_eq!(finding.level, Level::Warning);
        assert_eq!(
            finding.message,
            "the run took 1840 ms, not counting 4000 ms compiling 9 plugins, over --warn-after 1000 ms"
        );

        let finding = over_budget(elapsed, &compiled(1, 840), &options(1000, 3000))
            .expect("over both without the compile time");
        assert_eq!(finding.level, Level::Error);
        assert_eq!(
            finding.message,
            "the run took 5000 ms, not counting 840 ms compiling 1 plugin, over --fail-after 3000 ms (and --warn-after 1000 ms)"
        );
    }

    #[test]
    fn a_run_that_compiled_nothing_is_measured_whole() {
        let elapsed = Duration::from_millis(5840);
        // Loads that compiled nothing leave nothing out, whatever time they record.
        let finding = over_budget(elapsed, &compiled(0, 5560), &options(3000, 10_000))
            .expect("over --warn-after");
        assert_eq!(finding.level, Level::Warning);
        assert_eq!(
            finding.message,
            "the run took 5840 ms, over --warn-after 3000 ms"
        );
        assert_eq!(
            over_budget(
                Duration::from_millis(900),
                &Loads::default(),
                &options(1000, 2000)
            ),
            None
        );
    }
}

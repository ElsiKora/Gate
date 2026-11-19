//! `gate run <suite or step>`: runs a repository's steps, in each scope the change selects, and
//! reports each step's exit code and wall time.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
#[cfg(unix)]
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use globset::GlobBuilder;

use crate::ChangeRequest;
use crate::changes::{ChangeSet, Selection};
use crate::config::{Name, Settings, Sourced, Step, StepFiles};
use crate::error::{Error, Result};
use crate::workspace::Workspace;

/// What `gate run` was asked to do.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RunRequest {
    /// The suite or step.
    pub name: String,
    /// Which paths count as changed: the scopes that run, and `files = "changed"`.
    pub changes: ChangeRequest,
    /// `$GATE_HEAVY_WRAPPER`, used when `[run] heavy-wrapper` is unset.
    pub heavy_wrapper_env: Option<String>,
    /// Given to every step as `$1`, `$2`, ...: `gate run <name> -- <argument>...`, which a hook
    /// passes its own arguments to (the message file of `commit-msg`).
    pub arguments: Vec<String>,
    /// Given to every step on its standard input: what `gate run --stdin` read (the refs of
    /// `pre-push`). Without it, a step's standard input is empty.
    pub input: Option<Vec<u8>>,
}

/// Installs a set of the root's `[tools]` and gives the directory that holds one entry per tool,
/// or `None` for an empty set.
pub type InstallTools<'a> = dyn FnMut(&BTreeSet<Name>) -> Result<Option<PathBuf>> + 'a;

/// The tools of a run: each set installed once, when the first step that names it is about to run.
struct Tools<'a, 'b> {
    install: &'a mut InstallTools<'b>,
    installed: BTreeMap<BTreeSet<Name>, Option<PathBuf>>,
}

impl Tools<'_, '_> {
    /// The directory of `names`, installed now unless an earlier step installed the same set.
    fn bin(&mut self, names: &[Name]) -> Result<Option<PathBuf>> {
        let set: BTreeSet<Name> = names.iter().cloned().collect();
        if set.is_empty() {
            return Ok(None);
        }
        if let Some(bin) = self.installed.get(&set) {
            return Ok(bin.clone());
        }
        let bin = (self.install)(&set)?;
        self.installed.insert(set, bin.clone());
        Ok(bin)
    }
}

/// How one step ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// It exited with this code.
    Exit(i32),
    /// A signal ended it.
    Signal,
    /// It ran past its `timeout` (seconds) and was stopped.
    TimedOut(u64),
    /// It could not start: the reason.
    Failed(String),
    /// It did not need to run: the reason (no matching files).
    Skipped(String),
    /// An earlier failure stopped the run (`fail-fast`).
    NotRun,
}

impl Outcome {
    /// Whether the step failed.
    #[must_use]
    pub fn failed(&self) -> bool {
        match self {
            Self::Exit(code) => *code != 0,
            Self::Signal | Self::TimedOut(_) | Self::Failed(_) => true,
            Self::Skipped(_) | Self::NotRun => false,
        }
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exit(code) => write!(formatter, "exit {code}"),
            Self::Signal => formatter.write_str("killed by a signal"),
            Self::TimedOut(seconds) => write!(formatter, "timed out after {seconds} s"),
            Self::Failed(reason) => write!(formatter, "failed to start: {reason}"),
            Self::Skipped(reason) => write!(formatter, "skipped: {reason}"),
            Self::NotRun => formatter.write_str("not run (fail-fast)"),
        }
    }
}

/// One step of a run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepRun {
    /// `repository`, or `project <name>`.
    pub scope: String,
    /// The step.
    pub step: String,
    /// How it ended.
    pub outcome: Outcome,
    /// Its wall time.
    pub duration: Duration,
}

/// The outcome of `gate run`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunReport {
    /// The suite or step.
    pub name: String,
    /// `tree`, `staged`, `changed since <base>`, `push <range>`.
    pub mode: String,
    /// Every step, in the order they ran.
    pub steps: Vec<StepRun>,
    /// The projects that ran and that were skipped.
    pub selection: Selection,
}

impl RunReport {
    /// How many steps failed.
    #[must_use]
    pub fn failures(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| step.outcome.failed())
            .count()
    }

    /// 1 when a step failed, else 0.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        i32::from(self.failures() > 0)
    }

    /// `gate run <name> (<mode>): N steps, F failed; projects: ran a; skipped b`.
    #[must_use]
    pub fn summary(&self) -> String {
        let names = |decisions: &[crate::changes::Decision]| {
            if decisions.is_empty() {
                "none".to_owned()
            } else {
                decisions
                    .iter()
                    .map(|decision| decision.project.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        };
        let count = self.steps.len();
        format!(
            "gate run {} ({}): {count} {}, {} failed; projects: ran {}; skipped {}",
            self.name,
            self.mode,
            if count == 1 { "step" } else { "steps" },
            self.failures(),
            names(&self.selection.ran),
            names(&self.selection.skipped),
        )
    }
}

/// One step to run, resolved.
struct Planned<'a> {
    scope: String,
    root: String,
    project: Option<&'a str>,
    name: Name,
    step: &'a Sourced<Step>,
    fail_fast: bool,
}

/// One scope a run may visit.
struct Scope<'a> {
    label: String,
    root: String,
    project: Option<&'a str>,
    settings: &'a Settings,
    runs: bool,
}

/// Runs `request.name` in every selected scope that defines it. Just before a step that names tools
/// runs, `install` receives its set (once per set; a step that is skipped or not run installs
/// nothing), and the step's `PATH` starts with its own tools. `progress` receives a line before and after each step; the
/// steps' own output goes to the inherited standard output and error.
///
/// # Errors
/// Fails when no scope defines the name, a scope defines it as both a step and a suite, the
/// change cannot be taken, or a step's tools cannot be installed.
pub fn run(
    workspace: &Workspace,
    request: &RunRequest,
    install: &mut InstallTools<'_>,
    progress: &mut dyn FnMut(&str),
) -> Result<RunReport> {
    let git = workspace.snapshot().git();
    let changes = match &request.changes {
        ChangeRequest::All => None,
        ChangeRequest::Staged => Some(ChangeSet::staged(git)?),
        ChangeRequest::Since(base) => {
            Some(ChangeSet::since(git, base, workspace.snapshot().source())?)
        }
        ChangeRequest::Push { range, target_ref } => {
            Some(ChangeSet::push(git, range, target_ref.as_deref())?.0)
        }
    };
    let selection = match &changes {
        None => Selection::full(workspace.projects()),
        Some(set) => Selection::select(workspace.projects(), set, workspace.inputs()),
    };
    let mode = changes
        .as_ref()
        .map_or_else(|| "tree".to_owned(), |set| set.description.clone());

    let planned = plan(workspace, &selection, &request.name)?;

    let mut tools = Tools {
        install,
        installed: BTreeMap::new(),
    };

    let staged = match (&request.changes, &changes) {
        (ChangeRequest::Staged, Some(set)) => Some(set.clone()),
        _ if planned
            .iter()
            .any(|item| item.step.value.files == Some(StepFiles::Staged)) =>
        {
            Some(ChangeSet::staged(git)?)
        }
        _ => None,
    };
    let wrapper = workspace
        .root_config()
        .file()
        .run
        .as_ref()
        .and_then(|section| section.heavy_wrapper.clone())
        .or_else(|| request.heavy_wrapper_env.clone())
        .filter(|wrapper| !wrapper.trim().is_empty());

    let mut steps = Vec::new();
    let mut stopped = false;
    for item in &planned {
        let started = Instant::now();
        let outcome = if stopped {
            Outcome::NotRun
        } else {
            let set = match item.step.value.files {
                Some(StepFiles::Staged) => staged.as_ref(),
                Some(StepFiles::Changed) | None => changes.as_ref(),
            };
            let outcome = step(
                workspace,
                item,
                set,
                wrapper.as_deref(),
                request,
                &mut tools,
                progress,
            )?;
            progress(&format!(
                "gate run: {} {}: {outcome} in {:.2} s",
                item.scope,
                item.name,
                started.elapsed().as_secs_f64()
            ));
            stopped = outcome.failed() && item.fail_fast;
            outcome
        };
        steps.push(StepRun {
            scope: item.scope.clone(),
            step: item.name.to_string(),
            outcome,
            duration: started.elapsed(),
        });
    }

    Ok(RunReport {
        name: request.name.clone(),
        mode,
        steps,
        selection,
    })
}

/// Every step to run, in order: the repository scope's, then each selected project's.
fn plan<'a>(
    workspace: &'a Workspace,
    selection: &Selection,
    requested: &str,
) -> Result<Vec<Planned<'a>>> {
    let mut scopes = Vec::new();
    if !workspace.root_is_project() {
        scopes.push(Scope {
            label: "repository".to_owned(),
            root: String::new(),
            project: None,
            settings: workspace.repository(),
            runs: selection.repository,
        });
    }
    for project in workspace.projects().iter() {
        if let Some(settings) = workspace.settings(&project.name) {
            scopes.push(Scope {
                label: format!("project {}", project.name),
                root: project.root.clone(),
                project: Some(project.name.as_str()),
                settings,
                runs: selection.runs(&project.name),
            });
        }
    }

    let name = Name::try_from(requested.to_owned()).map_err(Error::config)?;
    let mut defined = false;
    let mut planned = Vec::new();
    for scope in scopes {
        let suite = scope.settings.suites.get(&name);
        let steps: Vec<(Name, bool)> = match (suite, scope.settings.steps.get(&name)) {
            (Some(_), Some(_)) => {
                return Err(Error::config(format!(
                    "{}: {name} is both a step and a suite; rename one",
                    scope.label
                )));
            }
            (Some(suite), None) => suite
                .value
                .steps
                .iter()
                .map(|step| (step.clone(), suite.value.fail_fast))
                .collect(),
            (None, Some(_)) => vec![(name.clone(), false)],
            (None, None) => continue,
        };
        defined = true;
        if !scope.runs {
            continue;
        }
        for (step_name, fail_fast) in steps {
            let Some(step) = scope.settings.steps.get(&step_name) else {
                return Err(Error::config(format!(
                    "{}: suite {name}: step {step_name} is not defined; add it to [steps]",
                    scope.label
                )));
            };
            planned.push(Planned {
                scope: scope.label.clone(),
                root: scope.root.clone(),
                project: scope.project,
                name: step_name,
                step,
                fail_fast,
            });
        }
    }

    if defined {
        Ok(planned)
    } else {
        Err(Error::config(format!(
            "no step or suite is named {name}; gate list prints the steps and suites of each scope"
        )))
    }
}

/// Runs one step, or skips it when it takes files and none matches.
fn step(
    workspace: &Workspace,
    item: &Planned<'_>,
    changes: Option<&ChangeSet>,
    wrapper: Option<&str>,
    request: &RunRequest,
    tools: &mut Tools<'_, '_>,
    progress: &mut dyn FnMut(&str),
) -> Result<Outcome> {
    let files = match item.step.value.files {
        None => None,
        Some(_) => Some(files(workspace, item, changes)?),
    };
    if let (Some(list), Some(kind)) = (&files, item.step.value.files)
        && list.is_empty()
    {
        let glob = item
            .step
            .value
            .glob
            .as_ref()
            .map(|glob| format!(" matching {glob}"))
            .unwrap_or_default();
        return Ok(Outcome::Skipped(format!("no {kind} files{glob}")));
    }
    let bin = tools.bin(&item.step.value.tools)?;
    let script = script(&item.step.value, wrapper, files.as_deref());
    progress(&format!("gate run: {} {}: {script}", item.scope, item.name));
    Ok(execute(workspace, item, &script, request, bin.as_deref()))
}

/// The step's working directory, relative to the repository root.
fn working_directory(item: &Planned<'_>) -> String {
    let cwd = item
        .step
        .value
        .cwd
        .as_ref()
        .map(|cwd| cwd.as_str().to_owned());
    match (item.root.as_str(), cwd) {
        ("", None) => String::new(),
        ("", Some(cwd)) => cwd,
        (root, None) => root.to_owned(),
        (root, Some(cwd)) => format!("{root}/{cwd}"),
    }
}

/// The paths a `files` step receives: the change's paths in its scope that exist in the working
/// tree, under its working directory and matching its glob, relative to that directory. With no
/// change (a full run), or one where every path counts as changed (the first push of a branch),
/// every tracked path.
fn files(
    workspace: &Workspace,
    item: &Planned<'_>,
    changes: Option<&ChangeSet>,
) -> Result<Vec<String>> {
    let snapshot = workspace.snapshot();
    let candidates: BTreeSet<&str> = match changes {
        Some(set) if !set.everything => set.paths.iter().map(String::as_str).collect(),
        Some(_) | None => snapshot.paths().collect(),
    };
    let glob = match &item.step.value.glob {
        Some(glob) => Some(
            GlobBuilder::new(glob)
                .build()
                .map_err(|error| {
                    Error::config(format!("{}: glob {glob}: {error}", item.step.origin))
                })?
                .compile_matcher(),
        ),
        None => None,
    };
    let directory = working_directory(item);
    let prefix = if directory.is_empty() {
        String::new()
    } else {
        format!("{directory}/")
    };
    let root = snapshot.git().root();
    let mut paths = Vec::new();

    for path in candidates {
        if let Some(project) = item.project
            && workspace
                .projects()
                .owner(path)
                .map(|owner| owner.name.as_str())
                != Some(project)
        {
            continue;
        }
        let Some(relative) = path.strip_prefix(&prefix) else {
            continue;
        };
        if glob.as_ref().is_some_and(|glob| !glob.is_match(relative)) {
            continue;
        }
        if root.join(path).is_file() {
            paths.push(relative.to_owned());
        }
    }

    Ok(paths)
}

/// The shell script of a step: the heavy wrapper, the command, the files, each file quoted.
fn script(step: &Step, wrapper: Option<&str>, files: Option<&[String]>) -> String {
    let mut script = String::new();
    if step.heavy
        && let Some(wrapper) = wrapper
    {
        script.push_str(wrapper.trim());
        script.push(' ');
    }
    script.push_str(step.run.trim());
    for file in files.unwrap_or_default() {
        script.push(' ');
        script.push_str(&quote(file));
    }
    script
}

/// A word quoted for `sh`.
#[must_use]
pub fn quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./=:@%+,".contains(character));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

fn execute(
    workspace: &Workspace,
    item: &Planned<'_>,
    script: &str,
    request: &RunRequest,
    tools: Option<&Path>,
) -> Outcome {
    let directory = workspace
        .snapshot()
        .git()
        .root()
        .join(working_directory(item));
    if !directory.is_dir() {
        return Outcome::Failed(format!(
            "the working directory {} does not exist",
            directory.display()
        ));
    }

    let mut command = Command::new("sh");
    // The step's name is `$0` and the run's arguments are `$1`, `$2`, .... Its standard input is
    // what `--stdin` read, or nothing: gate's own standard input may belong to the hook (the
    // pushed refs of pre-push, which the hook's later calls still need).
    command
        .arg("-c")
        .arg(script)
        .arg(item.name.as_str())
        .args(&request.arguments)
        .current_dir(&directory)
        .stdin(if request.input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    // The tokens gate reads releases with stay with gate: a step, and every tool it starts, gets
    // them only when its own `env` sets them.
    command
        .env_remove("GATE_GITHUB_TOKEN")
        .env_remove("GITHUB_TOKEN");
    for (key, value) in &item.step.value.env {
        command.env(key, value);
    }
    if let Some(tools) = tools {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut entries = vec![tools.to_path_buf()];
        entries.extend(std::env::split_paths(&path));
        if let Ok(joined) = std::env::join_paths(entries) {
            command.env("PATH", joined);
        }
    }
    new_group(&mut command);

    forward_signals();
    // The lock is held from the check of INTERRUPTED until the step's group is registered, and
    // the forwarder sets INTERRUPTED under the same lock: either it sees this group and stops it,
    // or the signal came first and the step never starts.
    let spawned = {
        let mut running = RUNNING.lock().unwrap_or_else(PoisonError::into_inner);
        if INTERRUPTED.load(Ordering::SeqCst) {
            None
        } else {
            let spawned = command.spawn();
            if let Ok(child) = &spawned {
                *running = Some(child.id());
            }
            Some(spawned)
        }
    };
    let mut child = match spawned {
        None => wait_for_exit(),
        Some(Ok(child)) => child,
        Some(Err(error)) => return Outcome::Failed(format!("sh: {error}")),
    };
    // Written from a thread, so a step that reads nothing, or stops reading, never blocks gate; a
    // step that ends first closes the pipe, and the rest is dropped.
    // Not joined: a process the step left running may hold the pipe open without reading.
    let _writer = match (child.stdin.take(), request.input.clone()) {
        (Some(mut stdin), Some(input)) => Some(std::thread::spawn(move || {
            let _ = stdin.write_all(&input);
        })),
        _ => None,
    };

    let ended = match item.step.value.timeout {
        None => match child.wait() {
            Ok(status) => outcome(status),
            Err(error) => Outcome::Failed(error.to_string()),
        },
        Some(seconds) => wait_with_timeout(&mut child, Duration::from_secs(seconds), seconds),
    };
    set_running(None);
    if INTERRUPTED.load(Ordering::SeqCst) {
        wait_for_exit();
    }
    ended
}

/// The forwarder is stopping the step's group and ends the process; nothing after it runs.
fn wait_for_exit() -> ! {
    loop {
        std::thread::park();
    }
}

/// Set by [`forward_signals`] when a signal arrived.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// The process group of the step running now, for [`forward_signals`].
static RUNNING: Mutex<Option<u32>> = Mutex::new(None);

fn set_running(group: Option<u32>) {
    *RUNNING.lock().unwrap_or_else(PoisonError::into_inner) = group;
}

/// A step runs in its own process group, which the terminal's Ctrl-C does not reach. Once per
/// process, a thread takes over SIGINT, SIGTERM and SIGHUP: it passes the signal to the running
/// step's group, stops the group as a timeout does, and ends gate with 128 + the signal.
#[cfg(unix)]
fn forward_signals() {
    use rustix::process::Signal;
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;

    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        let Ok(mut signals) = Signals::new([SIGINT, SIGTERM, SIGHUP]) else {
            return;
        };
        std::thread::spawn(move || {
            if let Some(signal) = signals.forever().next() {
                let group = {
                    let running = RUNNING.lock().unwrap_or_else(PoisonError::into_inner);
                    INTERRUPTED.store(true, Ordering::SeqCst);
                    *running
                };
                let forwarded = match signal {
                    SIGINT => Signal::INT,
                    SIGHUP => Signal::HUP,
                    _ => Signal::TERM,
                };
                if let Some(group) = group {
                    stop(group, forwarded, || false);
                }
                std::process::exit(128 + signal);
            }
        });
    });
}

#[cfg(not(unix))]
fn forward_signals() {}

fn wait_with_timeout(child: &mut Child, limit: Duration, seconds: u64) -> Outcome {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return outcome(status),
            Ok(None) => {}
            Err(error) => return Outcome::Failed(error.to_string()),
        }
        if started.elapsed() >= limit {
            stop_group(child);
            return Outcome::TimedOut(seconds);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn outcome(status: ExitStatus) -> Outcome {
    status.code().map_or(Outcome::Signal, Outcome::Exit)
}

#[cfg(unix)]
fn new_group(command: &mut Command) {
    use std::os::unix::process::CommandExt as _;
    command.process_group(0);
}

#[cfg(not(unix))]
fn new_group(_command: &mut Command) {}

/// How long a stopped step's group has between the first signal and KILL.
#[cfg(unix)]
const GRACE: Duration = Duration::from_secs(2);

/// Stops the step and everything it started: TERM to its process group, then KILL to the group
/// once the group is empty or the grace period is over, whichever comes first, since a child
/// that ignores TERM outlives `sh`.
fn stop_group(child: &mut Child) {
    #[cfg(unix)]
    {
        let mut reaped = false;
        stop(child.id(), rustix::process::Signal::TERM, || {
            reaped = reaped || matches!(child.try_wait(), Ok(Some(_)));
            reaped
        });
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Sends `signal` to the process group `group`, waits until no process of it is left (polling
/// `reap` to collect the leader) or [`GRACE`] has passed, then sends KILL to the group.
#[cfg(unix)]
fn stop(group: u32, signal: rustix::process::Signal, mut reap: impl FnMut() -> bool) {
    use rustix::process::{Pid, Signal, kill_process_group, test_kill_process_group};

    let Some(group) = i32::try_from(group).ok().and_then(Pid::from_raw) else {
        return;
    };
    let _ = kill_process_group(group, signal);
    let deadline = Instant::now() + GRACE;
    while Instant::now() < deadline {
        reap();
        if test_kill_process_group(group).is_err() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = kill_process_group(group, Signal::KILL);
}

/// The names `gate run` accepts in a scope: its suites, then its steps.
#[must_use]
pub fn names(settings: &Settings) -> Vec<&Name> {
    settings
        .suites
        .keys()
        .chain(settings.steps.keys())
        .collect()
}

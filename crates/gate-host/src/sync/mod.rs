//! `gate sync`: the files the core and the plugins' generators write (git hooks, the CI workflow,
//! generator outputs), writing them, and `gate sync --check`, which reports drift as `gate/sync`
//! findings.

pub mod github;
pub mod hooks;
pub mod yaml;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::config::{CiGithub, Name, PluginItem, check_relative};
use crate::error::{Error, Result};
use crate::findings::{Finding, Level, Location, RuleFilter};
use crate::snapshot::Source;
use crate::workspace::Workspace;

pub use github::WORKFLOW_MARKER;
pub use hooks::{HOOK_MARKER, HOOKS_DIRECTORY, LAUNCHER};

/// The pseudo-rule of drift between a generated file and what `gate sync` writes.
pub const SYNC_RULE: &str = "gate/sync";

/// One file `gate sync` writes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Output {
    /// What produces it: `hooks`, `ci`, or a generator `<plugin>/<generator>`; the aspect of its
    /// drift findings.
    pub item: String,
    /// The path, relative to the repository root.
    pub path: String,
    /// The contents.
    pub contents: Vec<u8>,
    /// Whether the file is executable (the hooks).
    pub executable: bool,
}

/// A file a plugin's generator returns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedFile {
    /// The path, relative to the repository root.
    pub path: String,
    /// The contents.
    pub contents: Vec<u8>,
}

/// Runs the plugins' generators: [`crate::PluginGenerators`] over the plugin runtime, or a test's
/// own answers.
pub trait Generators {
    /// The files `generator` writes for the project named `project` (`None`: the repository), with
    /// the options configuration gives it.
    ///
    /// # Errors
    /// Fails when the generator cannot run or fails.
    fn generate(
        &self,
        project: Option<&str>,
        generator: &PluginItem,
        options: Option<&serde_json::Value>,
    ) -> Result<Vec<GeneratedFile>>;

    /// The globs, relative to the repository root, whose tracked files `generator` owns for the
    /// project named `project`: `gate sync` removes one it no longer writes. None by default.
    ///
    /// # Errors
    /// Fails when the plugin declares no such generator, or a glob covers a file the core owns.
    fn owns(&self, _project: Option<&str>, _generator: &PluginItem) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    /// Whether a `gate check` narrowed by `filter` runs a configured rule that declares
    /// `READS_HISTORY`: a CI job that runs one checks out the whole history.
    fn reads_history(&self, filter: &RuleFilter) -> bool;
}

/// A glob of tracked files a generator owns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Owned {
    /// The generator, `<plugin>/<generator>`.
    pub item: String,
    /// The glob, relative to the repository root.
    pub glob: String,
}

/// A generated file `gate sync` no longer writes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stale {
    /// What generated it: `hooks`, `ci`, or the generator that owns it.
    pub item: String,
    /// The path, relative to the repository root.
    pub path: String,
}

/// Every file `gate sync` writes, sorted by path.
///
/// # Errors
/// Fails when a hook or job is wrong for its place, a generator fails or writes outside the
/// repository, or two outputs share a path.
pub fn plan(workspace: &Workspace, generators: &dyn Generators) -> Result<Vec<Output>> {
    let mut outputs = core_outputs(workspace, generators)?;
    for entry in generator_entries(workspace) {
        outputs.extend(generator_outputs(&entry, generators)?);
    }
    sorted(outputs)
}

/// The files the core generates itself: the hooks of `[hooks]` and the workflow of `[ci.github]`.
///
/// # Errors
/// Fails when a hook or job is wrong for its place.
pub fn core_outputs(workspace: &Workspace, generators: &dyn Generators) -> Result<Vec<Output>> {
    let root = workspace.root_config().file();
    let mut outputs = Vec::new();

    if let Some(section) = &root.hooks {
        for (path, script) in hooks::scripts(section)? {
            outputs.push(Output {
                item: "hooks".to_owned(),
                path,
                contents: script.into_bytes(),
                executable: true,
            });
        }
    }

    if let Some(github) = root.ci.as_ref().and_then(|ci| ci.github.as_ref()) {
        outputs.push(Output {
            item: "ci".to_owned(),
            path: github.workflow.as_str().to_owned(),
            contents: github::workflow(
                github,
                workspace.projects(),
                &history_jobs(workspace, github, generators),
            )?
            .into_bytes(),
            executable: false,
        });
    }

    for output in &outputs {
        refuse_protected(output)?;
    }
    Ok(outputs)
}

/// Refuses a configuration whose `[hooks]` or `[ci.github]` [`core_outputs`] would refuse, so a
/// configuration that loads always generates its own files: `git.sync-state` at a commit whose
/// configuration loads never falls back to the run's for them. Which CI jobs read history changes
/// a job's checkout only, never whether the workflow is valid.
///
/// # Errors
/// The configuration error [`core_outputs`] gives.
pub(crate) fn check_core_outputs(workspace: &Workspace) -> Result<()> {
    /// No generator, and no rule that reads history.
    struct Nothing;

    impl Generators for Nothing {
        fn generate(
            &self,
            _project: Option<&str>,
            _generator: &PluginItem,
            _options: Option<&serde_json::Value>,
        ) -> Result<Vec<GeneratedFile>> {
            Ok(Vec::new())
        }

        fn reads_history(&self, _filter: &RuleFilter) -> bool {
            false
        }
    }

    core_outputs(workspace, &Nothing).map(drop)
}

/// One `[generate]` entry that runs.
#[derive(Clone, Debug)]
pub struct GeneratorEntry<'w> {
    /// The project it runs for; `None`: the repository.
    pub project: Option<&'w str>,
    /// The generator, `<plugin>/<generator>`.
    pub generator: &'w PluginItem,
    /// Its options from configuration.
    pub options: Option<&'w serde_json::Value>,
    /// Where configuration sets it, for messages.
    pub origin: String,
}

/// Every `[generate]` entry that runs, in the order [`plan`] runs them.
#[must_use]
pub fn generator_entries(workspace: &Workspace) -> Vec<GeneratorEntry<'_>> {
    let mut entries = Vec::new();
    for (project, settings) in generator_scopes(workspace) {
        for (generator, entry) in &settings.generate {
            entries.push(GeneratorEntry {
                project,
                generator,
                options: entry.value.as_ref().map(|options| &options.value),
                origin: entry.origin.to_string(),
            });
        }
    }
    entries
}

/// The files one generator writes for `project`, each checked to be a relative path the
/// generator may write.
///
/// # Errors
/// Fails when the generator fails, or writes outside the repository or where gate sync never
/// writes.
pub fn generator_outputs(
    entry: &GeneratorEntry<'_>,
    generators: &dyn Generators,
) -> Result<Vec<Output>> {
    let generator = entry.generator;
    let mut outputs = Vec::new();
    for file in generators.generate(entry.project, generator, entry.options)? {
        let path = check_relative(&file.path).map_err(|message| {
            Error::config(format!(
                "{}: [generate] {generator} writes {message}",
                entry.origin
            ))
        })?;
        let output = Output {
            item: generator.to_string(),
            path,
            contents: file.contents,
            executable: false,
        };
        refuse_protected(&output)?;
        outputs.push(output);
    }
    Ok(outputs)
}

/// `outputs` sorted by path.
///
/// # Errors
/// Fails when two outputs share a path.
pub fn sorted(mut outputs: Vec<Output>) -> Result<Vec<Output>> {
    outputs.sort_by(|left, right| left.path.cmp(&right.path));
    if let Some(pair) = outputs.windows(2).find(|pair| pair[0].path == pair[1].path) {
        return Err(Error::config(format!(
            "{} and {} both write {}; give one of them another output path",
            pair[0].item, pair[1].item, pair[0].path
        )));
    }
    Ok(outputs)
}

fn refuse_protected(output: &Output) -> Result<()> {
    if let Some(reason) = protected(&output.path, &output.item) {
        return Err(Error::config(format!(
            "{} writes {}, which {reason}; gate sync never writes there, so give it another output path",
            output.item, output.path
        )));
    }
    Ok(())
}

/// The scopes whose `[generate]` entries run: the repository (unless its root is a project), then
/// each project with settings.
fn generator_scopes(workspace: &Workspace) -> Vec<(Option<&str>, &crate::config::Settings)> {
    let mut scopes = vec![(None, workspace.repository())];
    if workspace.root_is_project() {
        scopes.clear();
    }
    for project in workspace.projects().iter() {
        if let Some(settings) = workspace.settings(&project.name) {
            scopes.push((Some(project.name.as_str()), settings));
        }
    }
    scopes
}

/// The globs every configured generator owns.
///
/// # Errors
/// Fails when a generator's plugin does not declare it, or a glob covers a file the core owns or
/// is not a glob.
pub fn owned(workspace: &Workspace, generators: &dyn Generators) -> Result<Vec<Owned>> {
    let mut owned = Vec::new();
    for (project, settings) in generator_scopes(workspace) {
        for generator in settings.generate.keys() {
            for glob in generators.owns(project, generator)? {
                owned.push(Owned {
                    item: generator.to_string(),
                    glob,
                });
            }
        }
    }
    Ok(owned)
}

/// The ids of the jobs whose gate commands need the whole history.
fn history_jobs(
    workspace: &Workspace,
    github: &CiGithub,
    generators: &dyn Generators,
) -> BTreeSet<String> {
    github
        .jobs
        .iter()
        .filter(|(_, job)| {
            job.run.iter().any(|command| {
                let words: Vec<&str> = command.split_whitespace().collect();
                entry_reads_history(workspace, &words, generators)
            })
        })
        .map(|(id, _)| id.to_string())
        .collect()
}

/// Whether a job's entry needs the history: a `check` that does, or a `run` whose steps call one.
fn entry_reads_history(workspace: &Workspace, words: &[&str], generators: &dyn Generators) -> bool {
    match words {
        ["check", arguments @ ..] => check_reads_history(arguments, generators),
        ["run", _, flags @ ..] if flags.contains(&"--push") => true,
        ["run", name, ..] => {
            let mut scopes = vec![workspace.repository()];
            scopes.extend(
                workspace
                    .projects()
                    .iter()
                    .filter_map(|project| workspace.settings(&project.name)),
            );
            scopes.into_iter().any(|settings| {
                let Ok(name) = Name::try_from((*name).to_owned()) else {
                    return false;
                };
                let steps: Vec<&Name> = match settings.suites.get(&name) {
                    Some(suite) => suite.value.steps.iter().collect(),
                    None => vec![&name],
                };
                steps.into_iter().any(|step| {
                    settings.steps.get(step).is_some_and(|step| {
                        let words: Vec<&str> = step.value.run.split_whitespace().collect();
                        match words.as_slice() {
                            [program, "check", arguments @ ..]
                                if Path::new(program)
                                    .file_name()
                                    .is_some_and(|file| file == "gate") =>
                            {
                                check_reads_history(arguments, generators)
                            }
                            _ => false,
                        }
                    })
                })
            })
        }
        _ => false,
    }
}

/// Whether `gate check <arguments>` needs the history: it checks a pushed range, a revision or a
/// change since a base, or runs a rule that reads history.
fn check_reads_history(arguments: &[&str], generators: &dyn Generators) -> bool {
    let flag = |word: &str, name: &str| word == name || word.starts_with(&format!("{name}="));
    if arguments.iter().any(|word| {
        ["--push", "--rev", "--changed", "--base"]
            .iter()
            .any(|name| flag(word, name))
    }) {
        return true;
    }
    let mut keys = Vec::new();
    let mut words = arguments.iter();
    while let Some(word) = words.next() {
        let value = if *word == "--rule" {
            words.next().copied()
        } else {
            word.strip_prefix("--rule=")
        };
        if let Some(key) = value.and_then(|value| RuleFilter::parse(value).ok()) {
            keys.push(key);
        }
    }
    generators.reads_history(&RuleFilter::new(keys))
}

/// Generated files that are tracked and are no longer an output: a hook whose `[hooks]` entry was
/// removed, a workflow whose path changed (both carry the core's marker; searched: every tracked
/// file under the hooks directory and every tracked YAML file, wherever an earlier `workflow` put
/// it), and a file under a glob a generator owns (`owned`) that no generator wrote.
///
/// # Errors
/// Fails when a tracked file cannot be read or an owned glob is not a glob.
pub fn stale(workspace: &Workspace, outputs: &[Output], owned: &[Owned]) -> Result<Vec<Stale>> {
    let wanted: BTreeSet<&str> = outputs.iter().map(|output| output.path.as_str()).collect();
    let snapshot = workspace.snapshot();
    let mut stale = Vec::new();
    let mut matchers = Vec::new();
    for owned in owned {
        let matcher = globset::GlobBuilder::new(&owned.glob)
            .literal_separator(true)
            .build()
            .map_err(|error| {
                Error::environment(format!(
                    "{} owns {:?}, which is not a glob: {error}; report it to the plugin's authors",
                    owned.item, owned.glob
                ))
            })?
            .compile_matcher();
        matchers.push((owned.item.as_str(), matcher));
    }

    let candidates: Vec<&str> = snapshot
        .paths()
        .filter(|path| {
            path.starts_with(&format!("{HOOKS_DIRECTORY}/"))
                || Path::new(path)
                    .extension()
                    .is_some_and(|extension| extension == "yml" || extension == "yaml")
        })
        .collect();
    for path in candidates {
        if wanted.contains(path) || protected(path, "hooks").is_some() {
            continue;
        }
        if let Some(text) = snapshot.text(path)?
            && text
                .lines()
                .take(2)
                .any(|line| line == HOOK_MARKER || line == WORKFLOW_MARKER)
        {
            let item = if path.starts_with(HOOKS_DIRECTORY) {
                "hooks"
            } else {
                "ci"
            };
            stale.push(Stale {
                item: item.to_owned(),
                path: path.to_owned(),
            });
        }
    }
    if !matchers.is_empty() {
        for path in snapshot.paths() {
            if wanted.contains(path) || stale.iter().any(|known| known.path == path) {
                continue;
            }
            if let Some((item, _)) = matchers.iter().find(|(_, matcher)| matcher.is_match(path))
                && protected(path, item).is_none()
            {
                stale.push(Stale {
                    item: (*item).to_owned(),
                    path: path.to_owned(),
                });
            }
        }
    }

    Ok(stale)
}

/// `gate sync --check`: one `gate/sync#<item>` error per output that is missing, differs or lost
/// its executable bit, and per stale generated file (`owned`: the globs generators own).
///
/// # Errors
/// Fails when git cannot list file modes or a tracked file cannot be read.
pub fn check(workspace: &Workspace, outputs: &[Output], owned: &[Owned]) -> Result<Vec<Finding>> {
    let snapshot = workspace.snapshot();
    let modes = modes(workspace, outputs)?;
    let mut findings = Vec::new();

    for output in outputs {
        let message = match snapshot.read(&output.path)? {
            None => Some("is missing; run gate sync, then git add it".to_owned()),
            Some(contents) if *contents != *output.contents => Some(
                "differs from what gate sync writes (edited by hand, or gate.toml changed); run gate sync"
                    .to_owned(),
            ),
            Some(_) if output.executable && !modes.contains(output.path.as_str()) => {
                Some(if matches!(snapshot.source(), Source::WorkTree) {
                    "is not executable; run gate sync".to_owned()
                } else {
                    "is not executable in git; run gate sync, then git add it".to_owned()
                })
            }
            Some(_) => None,
        };
        if let Some(message) = message {
            findings.push(drift(&output.item, &output.path, &message));
        }
    }

    for stale in stale(workspace, outputs, owned)? {
        let message = if stale.item == "hooks" || stale.item == "ci" {
            "was generated by gate sync and gate.toml no longer names it; run gate sync to remove it"
                .to_owned()
        } else {
            format!(
                "is under a glob {} owns and no generator writes it; run gate sync to remove it",
                stale.item
            )
        };
        findings.push(drift(&stale.item, &stale.path, &message));
    }

    findings.sort();
    Ok(findings)
}

fn drift(item: &str, path: &str, message: &str) -> Finding {
    Finding {
        rule: SYNC_RULE.to_owned(),
        aspect: item.to_owned(),
        level: Level::Error,
        message: format!("{path} {message}"),
        location: Some(Location::file(path, None)),
        target: None,
        help: Some("generated files change through gate.toml and gate sync only".to_owned()),
        norm: None,
        project: None,
    }
}

/// The paths among the executable outputs that are executable in the snapshot: in the working
/// tree, its files' mode; in the index or a commit, mode 100755.
fn modes<'a>(workspace: &Workspace, outputs: &'a [Output]) -> Result<BTreeSet<&'a str>> {
    let paths: Vec<&str> = outputs
        .iter()
        .filter(|output| output.executable)
        .map(|output| output.path.as_str())
        .collect();
    if paths.is_empty() {
        return Ok(BTreeSet::new());
    }

    let snapshot = workspace.snapshot();
    if cfg!(unix) && matches!(snapshot.source(), Source::WorkTree) {
        let root = snapshot.git().root();
        return Ok(paths
            .into_iter()
            .filter(|path| is_executable(&root.join(path)))
            .collect());
    }
    let mut arguments: Vec<String> = match snapshot.source() {
        Source::Commit(_) => vec![
            "ls-tree".to_owned(),
            "-z".to_owned(),
            snapshot.commit().unwrap_or("HEAD").to_owned(),
        ],
        Source::WorkTree | Source::Index => {
            vec!["ls-files".to_owned(), "-s".to_owned(), "-z".to_owned()]
        }
    };
    arguments.push("--".to_owned());
    arguments.extend(paths.iter().map(|path| (*path).to_owned()));
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let output = snapshot.git().output(&arguments)?;

    let mut executable = BTreeSet::new();
    for record in output.split(|byte| *byte == 0) {
        let record = String::from_utf8_lossy(record);
        if let Some((meta, path)) = record.split_once('\t')
            && meta.starts_with("100755")
            && let Some(found) = paths.iter().find(|wanted| **wanted == path)
        {
            executable.insert(*found);
        }
    }
    Ok(executable)
}

/// What `gate sync` changed in the working tree.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Synced {
    /// Files written (new or changed), by path.
    pub written: Vec<String>,
    /// Stale generated files removed.
    pub removed: Vec<String>,
    /// Written files git does not track yet.
    pub untracked: Vec<String>,
    /// The previous `core.hooksPath` when `gate sync` set it (`Some("")` when it was unset).
    pub hooks_path: Option<String>,
}

/// `gate sync`: writes every output that differs, removes stale generated files (`owned`: the globs
/// generators own), and points `core.hooksPath` at the hooks when `[hooks]` is configured. Every
/// write and removal goes through the same guard: never `.git`, the core's files, the launcher, the
/// hooks directory for another item, or a path behind a symbolic link.
///
/// # Errors
/// Fails when a file cannot be written or git cannot be configured.
pub fn write(workspace: &Workspace, outputs: &[Output], owned: &[Owned]) -> Result<Synced> {
    let git = workspace.snapshot().git();
    let root = git.root().to_path_buf();
    let mut synced = Synced::default();

    // Every path is checked before anything changes, so a refusal leaves the tree as it was.
    let stale = stale(workspace, outputs, owned)?;
    let mut writes = Vec::new();
    for output in outputs {
        let path = guarded(&root, &output.path, &output.item)?;
        if std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(Error::environment(format!(
                "cannot write {}: it is a symbolic link, and gate sync writes regular files only; remove the link, then run gate sync",
                output.path
            )));
        }
        writes.push((output, path));
    }
    let mut removals = Vec::new();
    for stale in stale {
        let path = guarded(&root, &stale.path, &stale.item)?;
        removals.push((stale, path));
    }

    for (output, path) in writes {
        let current = std::fs::read(&path).ok();
        if current.as_deref() != Some(output.contents.as_slice()) {
            if let Some(directory) = path.parent() {
                std::fs::create_dir_all(directory).map_err(|error| {
                    Error::environment(format!("cannot create {}: {error}", directory.display()))
                })?;
            }
            std::fs::write(&path, &output.contents).map_err(|error| {
                Error::environment(format!("cannot write {}: {error}", path.display()))
            })?;
            synced.written.push(output.path.clone());
        }
        if output.executable {
            set_executable(&path)?;
        }
    }

    for (stale, path) in removals {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|error| {
                Error::environment(format!("cannot remove {}: {error}", path.display()))
            })?;
            synced.removed.push(stale.path);
        }
    }

    let tracked: BTreeSet<String> = git.paths(&["ls-files", "-z"])?.into_iter().collect();
    synced.untracked = outputs
        .iter()
        .filter(|output| !tracked.contains(&output.path))
        .map(|output| output.path.clone())
        .collect();

    if workspace.root_config().file().hooks.is_some() {
        let current = git
            .try_text(&["config", "--get", "core.hooksPath"])?
            .unwrap_or_default();
        if current.trim() != HOOKS_DIRECTORY {
            git.output(&["config", "core.hooksPath", HOOKS_DIRECTORY])?;
            synced.hooks_path = Some(current.trim().to_owned());
        }
    }

    Ok(synced)
}

/// The core's own files, at any depth: a generator never writes them and `gate sync` never
/// removes them.
const CORE_FILES: &[&str] = &["gate.toml", "gate.lock", "gate.exceptions.toml"];

/// Why `path` is not `item`'s to write or remove, when it is not: it is inside `.git` (any case),
/// it is a core file or the launcher, or it is in the hooks directory and `item` is not the hooks.
#[must_use]
pub fn protected(path: &str, item: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.iter().any(|part| part.eq_ignore_ascii_case(".git")) {
        return Some("is inside .git, which is git's own".to_owned());
    }
    if parts.last().is_some_and(|name| {
        CORE_FILES
            .iter()
            .any(|core| name.eq_ignore_ascii_case(core))
    }) {
        return Some("is a configuration file of gate".to_owned());
    }
    if path.eq_ignore_ascii_case(LAUNCHER) {
        return Some("is the launcher, which gate init writes".to_owned());
    }
    if item != "hooks"
        && parts
            .first()
            .is_some_and(|first| first.eq_ignore_ascii_case(HOOKS_DIRECTORY))
    {
        return Some(format!(
            "is in {HOOKS_DIRECTORY}, which holds the hooks gate sync writes from [hooks]"
        ));
    }
    None
}

/// `root` joined with `path`, once `path` is `item`'s to write or remove ([`protected`]) and no
/// directory between `root` and the file is a symbolic link, which would take the write or the
/// removal elsewhere.
fn guarded(root: &Path, path: &str, item: &str) -> Result<std::path::PathBuf> {
    if let Some(reason) = protected(path, item) {
        return Err(Error::config(format!(
            "gate sync refuses to write or remove {path}: it {reason}"
        )));
    }
    let parts: Vec<&str> = path.split('/').collect();
    let mut directory = root.to_path_buf();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        directory.push(part);
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Error::environment(format!(
                    "gate sync refuses to write or remove {path}: {} is a symbolic link; make it a directory, then run gate sync",
                    directory.strip_prefix(root).unwrap_or(&directory).display()
                )));
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    Ok(root.join(path))
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let metadata = std::fs::metadata(path)
        .map_err(|error| Error::environment(format!("cannot read {}: {error}", path.display())))?;
    let mut permissions = metadata.permissions();
    if permissions.mode() & 0o111 != 0o111 {
        permissions.set_mode(permissions.mode() | 0o755);
        std::fs::set_permissions(path, permissions).map_err(|error| {
            Error::environment(format!(
                "cannot make {} executable: {error}",
                path.display()
            ))
        })?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// The outputs of each item, for a summary: item → paths.
#[must_use]
pub fn by_item(outputs: &[Output]) -> BTreeMap<&str, Vec<&str>> {
    let mut items: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for output in outputs {
        items
            .entry(output.item.as_str())
            .or_default()
            .push(output.path.as_str());
    }
    items
}

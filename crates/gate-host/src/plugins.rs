//! The plugins of a run: loading them, checking the configuration against what they declare, and
//! running their rules over the snapshot through the plugin runtime.
//!
//! A plugin's paths are relative to its grant root; the runtime hands back findings and read sets
//! with repository paths, so everything here (reporting, exceptions, the read-set record) sees
//! repository paths only.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Instant;

use gate_runtime::{
    Blob, Call, CallOutcome, CallTime, Grant, History, HostError, Invocation, Plugin, ReadSet,
    Runtime, Size, Tree,
};
use gate_sdk::{Commit, CommitRange, PluginInfo, RuleInfo, RuleRun, Scope, SyncState};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::cache::answers::{Answer, Answers, Question};
use crate::cache::{Cache, Sha256Sum, results};
use crate::changes::{ChangeSet, Selection};
use crate::config::{
    LevelSetting, PluginItem, PluginName, PluginSource, RuleId, SchemaValidator, Setting, Settings,
    Sourced,
};
use crate::error::{Error, Result};
use crate::fetch::{FetchConfig, Fetcher};
use crate::findings::{Finding, Findings, Level, Location, RuleFilter, RulesRan, Span};
use crate::git::{Git, Merges, Objects};
use crate::lock::{Lock, Wanted};
use crate::projects::Project;
use crate::snapshot::{Snapshot, Source};
use crate::sync::{self, GeneratedFile as SyncFile, Generators};
use crate::workspace::{PresetOptions, RuleDeclaration, RuleScope, Workspace};

mod declarations;

/// The local builds `--plugin` gives, by plugin name: the path and the bytes read once.
pub(crate) type Overrides = BTreeMap<PluginName, (PathBuf, Vec<u8>)>;

/// The plugins a run runs, loaded into one runtime, and what every declared plugin declares.
pub struct Loaded {
    runtime: Runtime,
    /// The plugins this command runs.
    plugins: BTreeMap<PluginName, Plugin>,
    /// What each declared plugin declares: those this command runs, and the others, from the
    /// cache or loaded for it. The configuration is checked against all of them. A plugin is
    /// missing only when it does not load under this core for good (`commit_plugins`).
    declarations: BTreeMap<PluginName, Declaration>,
    /// The run's `--plugin` builds, which `git.sync-state` loads again at other commits.
    overrides: Arc<Overrides>,
}

/// What a plugin declares when it describes itself, with its options schemas parsed: what the
/// configuration is checked against.
#[derive(Debug)]
struct Declaration {
    info: PluginInfo,
    /// The schema of the plugin's `[plugins.<name>] options`.
    options: Value,
    /// Each rule's options schema, by rule name.
    rules: BTreeMap<String, Value>,
}

impl Declaration {
    /// What plugin `name` declares, with each options schema checked to be a JSON Schema.
    ///
    /// # Errors
    /// Fails, for good, when the plugin or a rule declares an options schema that is not one.
    fn of(name: &PluginName, info: PluginInfo) -> std::result::Result<Self, LoadFailure> {
        let options = schema(name, "the plugin", &info.options_schema)?;
        let mut rules = BTreeMap::new();
        for rule in &info.rules {
            let schema = schema(name, &format!("rule {}", rule.name), &rule.options_schema)?;
            rules.insert(rule.name.clone(), schema);
        }
        Ok(Self {
            info,
            options,
            rules,
        })
    }

    fn rule(&self, rule: &str) -> Option<&RuleInfo> {
        self.info.rules.iter().find(|info| info.name == rule)
    }
}

/// Which of the declared plugins a command runs. Every other declared plugin gives what it
/// declares from the cache, or is loaded for it when the cache holds none.
#[derive(Clone, Debug)]
pub(crate) enum Needed {
    /// Every declared plugin: `gate sync`, which every clone runs before its first hook, so each
    /// plugin is compiled into the module cache, and its declaration stored, before a hook needs
    /// it.
    Every,
    /// Only these.
    Only(BTreeSet<PluginName>),
}

impl Needed {
    fn wants(&self, name: &PluginName) -> bool {
        match self {
            Self::Every => true,
            Self::Only(names) => names.contains(name),
        }
    }
}

/// The plugins a check runs: those declared in a scope `selection` runs that `filter` allows.
pub(crate) fn needed(workspace: &Workspace, selection: &Selection, filter: &RuleFilter) -> Needed {
    let mut names = BTreeSet::new();
    for (project, settings) in scopes(workspace) {
        let runs = match project {
            None => selection.repository,
            Some(name) => selection.runs(name),
        };
        if !runs {
            continue;
        }
        names.extend(
            settings
                .plugins
                .keys()
                .filter(|name| filter.allows_plugin(name.as_str()))
                .cloned(),
        );
    }
    Needed::Only(names)
}

impl std::fmt::Debug for Loaded {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Loaded")
            .field("plugins", &self.plugins.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

/// What one plugin call read, for the read-set record of the result cache.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginReads {
    /// The project the call ran for; `None` for the repository scope.
    pub project: Option<String>,
    /// The plugin.
    pub plugin: String,
    /// The paths listed and read (repository paths, with blob ids) and the git queries.
    pub read_set: ReadSet,
    /// Whether the result came from the result cache, the call not run.
    pub from_cache: bool,
    /// For a call that ran although a result of it was stored: the first input it recorded that
    /// answers differently now.
    pub stale: Option<String>,
    /// Where the call's time went; for a result from the cache, the time spent loading it and
    /// checking that every input it recorded answers as it did.
    pub time: CallTime,
    /// The fuel the call used; zero for a result from the cache.
    pub fuel_used: u64,
    /// The call's invocations, each with its rules, time and fuel; none for a result from the
    /// cache.
    pub invocations: Vec<Invocation>,
}

/// A call's outcome, and whether it came from the result cache or why it ran again.
struct Looked {
    outcome: gate_runtime::Result<CallOutcome>,
    from_cache: bool,
    stale: Option<String>,
}

/// A file at `commit` that its tree names and git cannot read, for `why`: a partial clone could
/// not fetch it. No answer is stored, so a later run with the remote reachable reads it.
fn unfetched(commit: &str, path: &str, why: &str) -> HostError {
    HostError::new(format!(
        "git cannot read {path:?} at commit {commit}, though its tree names the file ({why}): a \
         partial clone could not fetch it; run git fetch, or run gate where the remote is \
         reachable"
    ))
}

/// An answer [`GitHistory::remembered`] may keep.
fn kept(answer: Answer) -> (Answer, bool) {
    (answer, true)
}

/// What the plugins of a run reported.
#[derive(Debug, Default)]
pub(crate) struct Ran {
    pub(crate) rules: usize,
    pub(crate) per_scope: RulesRan,
    pub(crate) reads: Vec<PluginReads>,
    pub(crate) sync_states: SyncStates,
    pub(crate) git_answers: GitAnswers,
    pub(crate) generated: Generated,
    /// What the first cache write that failed said, when one did.
    pub(crate) cache_warning: Option<String>,
}

/// The first cache write of a run that failed. A write is best effort: the findings do not depend
/// on it, so its failure is one warning, not a failed check, as a bad entry read is a miss.
#[derive(Debug, Default)]
struct CacheWrites(Mutex<Option<String>>);

impl CacheWrites {
    /// Notes `written`'s failure, when it failed.
    fn note(&self, written: Result<()>) {
        if let Err(error) = written {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get_or_insert_with(|| {
                    format!(
                        "cannot write to the cache ({error}); the findings are complete, but the \
                         next run cannot reuse this run's work: free space, or set GATE_CACHE_DIR \
                         to a writable directory"
                    )
                });
        }
    }

    fn take(&self) -> Option<String> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

/// The generator runs of a command: how many ran, how many the result cache answered, and the
/// cache writes that failed.
#[derive(Debug, Default)]
struct Generations {
    ran: AtomicUsize,
    stored: AtomicUsize,
    writes: CacheWrites,
}

impl Generations {
    fn read(&self) -> Generated {
        Generated {
            ran: self.ran.load(Ordering::Relaxed),
            stored: self.stored.load(Ordering::Relaxed),
        }
    }
}

/// The generator runs of a command: how many ran, and how many the result cache answered.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Generated {
    pub ran: usize,
    pub stored: usize,
}

/// The `sync-state` answers of a run: how many it computed, and how many it read from the cache.
/// Each commit counts once, however many calls and rules ask for it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SyncStates {
    pub computed: usize,
    pub stored: usize,
}

/// The git questions of a run about full commit ids (`tree`, `changed-paths`, `merge-tree`,
/// `is-ancestor`): how many it asked git, and how many an earlier run's stored answers answered.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GitAnswers {
    pub asked: usize,
    pub stored: usize,
}

/// Loads the plugins `needed` names: the local build of each `--plugin` entry for this run
/// (`overrides`, read once), else the download `gate.lock` pins, from the cache or fetched. The
/// bytes that load are the bytes that were hashed. Every other declared plugin gives what it
/// declares from the cache, without being fetched or compiled; one the cache holds nothing for is
/// loaded for it. `None` when no configuration declares a plugin.
///
/// # Errors
/// Fails when a plugin is declared with two sources, `gate.lock` pins no download of one, a
/// pinned download cannot be fetched or does not hash to its pin, a build is not a valid plugin
/// or is another plugin than its name says, or a rule declares an options schema that is not
/// JSON.
pub(crate) fn load(
    workspace: &Workspace,
    lock: Option<&Lock>,
    overrides: &Overrides,
    needed: &Needed,
) -> Result<Option<Loaded>> {
    let Some(declared) = declared(workspace, lock, overrides)? else {
        return Ok(None);
    };
    let cache = Cache::from_env()?;
    let mut stored = BTreeMap::new();
    let mut loading = Vec::new();
    for plugin in declared {
        let from_cache = (!needed.wants(&plugin.name))
            .then(|| declarations::read(&cache, &plugin.sha256))
            .flatten()
            .filter(|info| info.name == plugin.source.plugin)
            .and_then(|info| Declaration::of(&plugin.name, info).ok());
        match from_cache {
            Some(declaration) => {
                stored.insert(plugin.name, declaration);
            }
            None => loading.push(plugin),
        }
    }
    let fetched = fetch_components(loading, lock, overrides)?;
    let (mut loaded, _) = instantiate(fetched, overrides, true)?;
    loaded.plugins.retain(|name, _| needed.wants(name));
    loaded.declarations.extend(stored);
    Ok(Some(loaded))
}

/// A declared plugin: its one source and the sha256 of the bytes that load for it.
struct Declared {
    name: PluginName,
    source: PluginSource,
    sha256: Sha256Sum,
}

/// Every plugin the configuration declares, each checked before any is left out of a run: one
/// source, and the bytes that load for it (its `--plugin` build, else a download `gate.lock`
/// pins). `None` when no configuration declares a plugin.
///
/// # Errors
/// Fails when a plugin is declared with two sources, or `gate.lock` pins no download of one.
fn declared(
    workspace: &Workspace,
    lock: Option<&Lock>,
    overrides: &Overrides,
) -> Result<Option<Vec<Declared>>> {
    let wanted = Wanted::of(workspace);
    let mut names: BTreeSet<&PluginName> = BTreeSet::new();
    for settings in scopes(workspace).map(|(_, settings)| settings) {
        names.extend(settings.plugins.keys());
    }
    if names.is_empty() {
        return Ok(None);
    }
    let mut declared = Vec::new();
    for name in names {
        let source = wanted.source_of(name)?;
        let sha256 = if let Some((_, bytes)) = overrides.get(name) {
            Sha256Sum::from_digest(&Sha256::digest(bytes))
        } else {
            lock.and_then(|lock| lock.plugins.get(source))
                .map(|pin| pin.sha256.clone())
                .ok_or_else(|| {
                    Error::config(format!(
                        "gate.lock pins no download of {source}; run gate lock and commit gate.lock"
                    ))
                })?
        };
        declared.push(Declared {
            name: name.clone(),
            source: source.clone(),
            sha256,
        });
    }
    Ok(Some(declared))
}

/// A declared plugin's component: fetched and verified against its pin, or read from `--plugin`.
struct Fetched {
    name: PluginName,
    source: PluginSource,
    /// The sha256 of `bytes`.
    sha256: Sha256Sum,
    /// Where the bytes came from, for messages.
    from: String,
    bytes: Vec<u8>,
}

/// The components of `plugins`.
///
/// # Errors
/// Fails when a pinned download cannot be fetched or does not hash to its pin.
fn fetch_components(
    plugins: Vec<Declared>,
    lock: Option<&Lock>,
    overrides: &Overrides,
) -> Result<Vec<Fetched>> {
    let mut fetcher = None;
    let mut components = Vec::new();
    for Declared {
        name,
        source,
        sha256,
    } in plugins
    {
        let (from, bytes) = if let Some((path, bytes)) = overrides.get(&name) {
            (format!("--plugin {name}={}", path.display()), bytes.clone())
        } else {
            let pin = lock
                .and_then(|lock| lock.plugins.get(&source))
                .ok_or_else(|| {
                    Error::config(format!(
                        "gate.lock pins no download of {source}; run gate lock and commit gate.lock"
                    ))
                })?;
            if fetcher.is_none() {
                fetcher = Some(Fetcher::new(FetchConfig::from_env()?, Cache::from_env()?));
            }
            let fetcher = fetcher
                .as_ref()
                .ok_or_else(|| Error::environment("the plugin fetcher is unavailable"))?;
            let unpublished = format!(
                "if the asset is not published yet, publish it, or admit the build on this machine with gate lock --set {name}={} --file <the built .wasm>",
                pin.url
            );
            let path = fetcher
                .fetch_advised(&source.asset(), pin, &unpublished)
                .map_err(|error| {
                    let message = format!("plugin {name} = {source}: {}", error.message());
                    match error.kind() {
                        crate::error::ErrorKind::Config => Error::config(message),
                        crate::error::ErrorKind::Environment => Error::environment(message),
                    }
                })?;
            let bytes = std::fs::read(&path).map_err(|error| {
                Error::environment(format!("cannot read {}: {error}", path.display()))
            })?;
            let actual = Sha256Sum::from_digest(&Sha256::digest(&bytes));
            if actual != pin.sha256 {
                return Err(Error::environment(format!(
                    "{} changed after it was verified: gate.lock pins sha256 {}, the file now has sha256 {actual}; run gate again, and check what else writes to the cache",
                    path.display(),
                    pin.sha256
                )));
            }
            (source.to_string(), bytes)
        };
        components.push(Fetched {
            name,
            source,
            sha256,
            from,
            bytes,
        });
    }
    Ok(components)
}

/// Why a fetched component did not load.
#[derive(Debug)]
struct LoadFailure {
    error: Error,
    /// Whether it fails the same way every time under this core: the component is not one this
    /// core loads (the runtime's `Load` or `Plugin` error), uses up the core's fuel or memory while
    /// it describes itself, is another plugin than its name says, or declares an options schema
    /// that is not a JSON Schema. Otherwise (the runtime's setup, a trap or the deadline while it describes
    /// itself) the failure may pass by the next run.
    lasting: bool,
}

/// Compiles and checks the fetched components, and stores what each declares. A component that
/// does not load is left out of the result and its failure returned by name; with `strict`, the
/// first such failure fails the call instead.
///
/// # Errors
/// Fails when the runtime cannot start, or with `strict` as above.
fn instantiate(
    fetched: Vec<Fetched>,
    overrides: &Overrides,
    strict: bool,
) -> Result<(Loaded, BTreeMap<PluginName, LoadFailure>)> {
    let cache = Cache::from_env()?;
    let runtime = Runtime::new(gate_runtime::Config {
        module_cache: Some(cache.modules()),
        ..gate_runtime::Config::default()
    })
    .map_err(|error| Error::environment(error.to_string()))?;
    let mut plugins = BTreeMap::new();
    let mut declarations = BTreeMap::new();
    let mut unloadable = BTreeMap::new();

    // Each component compiles (or loads its compiled module) and describes itself on a thread of
    // its own: a warm run spends most of its time here, and the slowest plugin bounds it.
    let compiled: Vec<std::result::Result<Compiled, LoadFailure>> = std::thread::scope(|scope| {
        let runtime = &runtime;
        let handles: Vec<_> = fetched
            .iter()
            .map(|component| scope.spawn(move || compile(runtime, component)))
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle.join().unwrap_or_else(|_| {
                    Err(LoadFailure {
                        error: Error::environment(
                            "a thread loading a plugin panicked; run gate again, and report it if it recurs",
                        ),
                        lasting: false,
                    })
                })
            })
            .collect()
    });
    for (component, compiled) in fetched.into_iter().zip(compiled) {
        match compiled {
            Ok((plugin, declaration)) => {
                declarations::write(&cache, &component.sha256, &declaration.info);
                declarations.insert(component.name.clone(), declaration);
                plugins.insert(component.name, plugin);
            }
            Err(failure) if strict => return Err(failure.error),
            Err(failure) => {
                unloadable.insert(component.name, failure);
            }
        }
    }

    Ok((
        Loaded {
            runtime,
            plugins,
            declarations,
            overrides: Arc::new(overrides.clone()),
        },
        unloadable,
    ))
}

/// A component compiled under this core, and what it declares.
type Compiled = (Plugin, Declaration);

/// One component compiled under this core, and what it declares.
///
/// # Errors
/// Fails when it is not a valid plugin, is another plugin than its name says, or the plugin or a
/// rule declares an options schema that is not a JSON Schema.
fn compile(runtime: &Runtime, component: &Fetched) -> std::result::Result<Compiled, LoadFailure> {
    let Fetched {
        name,
        source,
        sha256: _,
        from,
        bytes,
    } = component;
    let plugin = runtime
        .load(name.as_str(), bytes)
        .map_err(|error| LoadFailure {
            lasting: lasting_load_failure(error.kind()),
            error: Error::environment(format!("{from}: {error}")),
        })?;
    if plugin.info().name != source.plugin {
        return Err(LoadFailure {
            error: Error::config(format!(
                "{from}: the file is plugin {}, not {}; give the build of {}",
                plugin.info().name,
                source.plugin,
                source.plugin
            )),
            lasting: true,
        });
    }
    let declaration = Declaration::of(name, plugin.info().clone())?;
    Ok((plugin, declaration))
}

/// An options schema `what` of plugin `name` declares, checked to compile as JSON Schema.
///
/// # Errors
/// Fails, for good, when it is empty, not JSON, or not a JSON Schema: the plugin is broken.
fn schema(name: &PluginName, what: &str, text: &str) -> std::result::Result<Value, LoadFailure> {
    let broken = |problem: String| LoadFailure {
        error: Error::environment(format!(
            "plugin {name}: {what} declares an options schema that {problem}; the plugin is broken, report it to its authors"
        )),
        lasting: true,
    };
    let schema: Value =
        serde_json::from_str(text).map_err(|error| broken(format!("is not JSON ({error})")))?;
    jsonschema::validator_for(&schema)
        .map_err(|error| broken(format!("is not a valid JSON Schema ({error})")))?;
    Ok(schema)
}

/// Whether a runtime error while loading a component recurs every time under this core: the
/// component does not compile, link or declare itself validly (`Load`), breaks the interface
/// (`Plugin`), or uses up the fuel or memory the core gives `describe` (the limits are the core's
/// and `describe` has no input, so it does the same on every run). Anything else while loading (a
/// trap or the deadline in `describe`, the machine) may pass by the next run. A `Load` from a
/// later call is not lasting: see [`passing_call_failure`].
fn lasting_load_failure(kind: gate_runtime::ErrorKind) -> bool {
    matches!(
        kind,
        gate_runtime::ErrorKind::Load
            | gate_runtime::ErrorKind::Plugin
            | gate_runtime::ErrorKind::OutOfFuel
            | gate_runtime::ErrorKind::OutOfMemory
    )
}

impl Loaded {
    /// Checks the configuration and the `--rule` keys of `filter` against what the loaded plugins
    /// declare.
    ///
    /// # Errors
    /// Fails on the first configuration error, naming the file, line and section, or the first key
    /// that names no configured rule.
    pub(crate) fn validate(&self, workspace: &Workspace, filter: &RuleFilter) -> Result<()> {
        self.validate_except(workspace, filter, &BTreeSet::new())
    }

    /// [`Loaded::validate`] without the rules and exceptions of the plugins in `skipped`, which a
    /// commit pins and which do not load under this core: what they declare is unknown. The
    /// checks that need no plugin still cover them.
    ///
    /// # Errors
    /// As [`Loaded::validate`].
    fn validate_except(
        &self,
        workspace: &Workspace,
        filter: &RuleFilter,
        skipped: &BTreeSet<PluginName>,
    ) -> Result<()> {
        let declaration = |name: &PluginName, _: &PluginSource, rule: &str| {
            let declared = self.declarations.get(name)?;
            let info = declared.rule(rule)?;
            Some(RuleDeclaration {
                scope: match info.scope {
                    Scope::Project => RuleScope::Project,
                    Scope::Repository => RuleScope::Repository,
                },
                aspects: info.aspects.clone(),
                options_schema: declared
                    .rules
                    .get(rule)
                    .cloned()
                    .unwrap_or(Value::Bool(true)),
            })
        };
        workspace.validate_plugin_options(
            &|name: &PluginName| {
                self.declarations
                    .get(name)
                    .map(|declared| declared.options.clone())
            },
            &JsonSchemaValidator,
        )?;
        // The options the scope's presets give a rule, which the run uses when `[rules]` gives none.
        let preset_options =
            |settings: &Settings, rule: &RuleId| -> Result<Option<PresetOptions>> {
                let Some((name, declared)) = self.declarations.get_key_value(rule.plugin()) else {
                    return Ok(None);
                };
                Ok(presets(&declared.info, name, settings)?
                    .remove(rule.rule())
                    .and_then(|preset| {
                        preset.options.map(|options| PresetOptions {
                            options,
                            preset: preset.extended,
                        })
                    }))
            };
        workspace.validate_except(&declaration, &preset_options, &JsonSchemaValidator, skipped)?;
        self.check_presets(workspace)?;
        self.check_needs(workspace, filter)?;
        self.check_decisions(workspace)?;
        self.check_generators(workspace)?;
        workspace.check_rule_filter(filter, &declaration)
    }

    /// Checks that every `extends` entry of every scope names a preset its plugin declares, that
    /// each such preset reads, and that each entry turns on a rule of some scope it reaches, for
    /// every declared plugin whether or not the run runs it.
    ///
    /// # Errors
    /// Fails on the first preset the plugin does not declare, naming the entry; on the first entry
    /// whose preset names no rule of any scope the entry applies to, naming the entry and where it
    /// goes instead; or, as a broken plugin, on a preset that is not a JSON object of rules.
    fn check_presets(&self, workspace: &Workspace) -> Result<()> {
        let mut reach: Vec<Reach<'_>> = Vec::new();
        for (project, settings) in scopes(workspace) {
            let scope = if project.is_none() {
                Scope::Repository
            } else {
                Scope::Project
            };
            for (name, declared) in &self.declarations {
                if !settings.plugins.contains_key(name) {
                    continue;
                }
                presets(&declared.info, name, settings)?;
                for item in settings
                    .extends
                    .iter()
                    .filter(|item| item.value.plugin() == name.as_str())
                {
                    let turns_on = preset_turns_on(&declared.info, item.value.name(), scope);
                    if let Some(seen) = reach.iter_mut().find(|seen| {
                        seen.item.value == item.value && seen.item.origin == item.origin
                    }) {
                        seen.repository |= project.is_none();
                        seen.project |= project.is_some();
                        seen.turns_on |= turns_on;
                    } else {
                        reach.push(Reach {
                            item,
                            repository: project.is_none(),
                            project: project.is_some(),
                            turns_on,
                        });
                    }
                }
            }
        }
        if let Some(seen) = reach.iter().find(|seen| !seen.turns_on) {
            return Err(Error::config(seen.message(workspace.root_is_project())));
        }
        Ok(())
    }

    /// Checks that, where a `[decision-policy]` requires it, every level configuration sets below
    /// its default names a decision. The default is what the rule's presets give the aspect, else
    /// the rule; a rule no preset names has none, since turning it on at any level is stricter than
    /// leaving it off. A whole-rule level (an entry's `level`, or a `"<plugin>/*"` family's) is
    /// compared with the preset's rule level and with each preset aspect level it overrides.
    ///
    /// # Errors
    /// Fails on the first level below its default with no decision, naming where it is written,
    /// the default and where that comes from, and the policy that requires the decision.
    fn check_decisions(&self, workspace: &Workspace) -> Result<()> {
        for (project, settings) in scopes(workspace) {
            let scope = if project.is_none() {
                Scope::Repository
            } else {
                Scope::Project
            };
            for (name, declared) in &self.declarations {
                if !settings.plugins.contains_key(name) {
                    continue;
                }
                let presets = presets(&declared.info, name, settings)?;
                for info in declared
                    .info
                    .rules
                    .iter()
                    .filter(|info| info.scope == scope)
                {
                    let id = rule_id(name, &info.name)?;
                    let Some(preset) = presets.get(info.name.as_str()) else {
                        // No preset names the rule: it runs only because an entry or a family
                        // turns it on, which is stricter than not running, at any level.
                        continue;
                    };
                    let entry = settings.rules.get(&id);
                    let named =
                        |aspect: &str| entry.is_some_and(|entry| entry.levels.contains_key(aspect));
                    let whole = match entry.and_then(|entry| entry.level.as_ref()) {
                        Some(level) => Some((level, Written::Rule(&id))),
                        None => settings
                            .families
                            .get(name.as_str())
                            .map(|level| (level, Written::Family(name))),
                    };
                    if let Some((level, written)) = whole {
                        // A whole-rule level replaces the preset's rule level and each preset
                        // aspect level the entry's `levels` does not name.
                        let mut defaults: Vec<Floor> = Vec::new();
                        if let Some(rule_level) = preset.level {
                            defaults.push(Floor::rule(&id, None, rule_level, preset));
                        }
                        for (aspect, aspect_level) in &preset.levels {
                            if !named(aspect) {
                                defaults.push(Floor::aspect(&id, aspect, *aspect_level, preset));
                            }
                        }
                        for default in defaults {
                            decided(workspace, &written, level, &default)?;
                        }
                    }
                    for (aspect, level) in entry.into_iter().flat_map(|entry| &entry.levels) {
                        let default = match (preset.levels.get(aspect), preset.level) {
                            (Some(aspect_level), _) => {
                                Floor::aspect(&id, aspect, *aspect_level, preset)
                            }
                            (None, Some(rule_level)) => {
                                Floor::rule(&id, Some(aspect), rule_level, preset)
                            }
                            (None, None) => continue,
                        };
                        decided(workspace, &Written::Aspect(&id, aspect), level, &default)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Checks that in every scope the `[plugins.<name>] options` hold each key that an enabled rule
    /// the run allows declares it reads (`gate_sdk::Rule::NEEDS`), so a missing key fails as
    /// configuration before anything runs, naming the rule, and not when the rule runs.
    ///
    /// # Errors
    /// Fails on the first missing key, naming the plugin's entry, the key and the rule; or, as a
    /// broken plugin, on a rule whose declared needs are not a list of `{ key, when, why }`.
    fn check_needs(&self, workspace: &Workspace, filter: &RuleFilter) -> Result<()> {
        let empty = Value::Object(serde_json::Map::new());
        for (project, settings) in scopes(workspace) {
            for (name, declared) in &self.declarations {
                let Some(entry) = settings.plugins.get(name) else {
                    continue;
                };
                if !filter.allows_plugin(name.as_str()) {
                    continue;
                }
                let options = settings.plugin_options(name).unwrap_or(&empty);
                for rule in enabled_rules(&declared.info, name, settings, workspace, project)? {
                    if !filter.allows(&rule.id.to_string()) {
                        continue;
                    }
                    let Some(schema) = declared.rules.get(&rule.run.name) else {
                        continue;
                    };
                    for need in needs(name, &rule.run.name, schema)? {
                        if options.get(&need.key).is_some()
                            || need
                                .when
                                .as_ref()
                                .is_some_and(|when| !when.is_valid(options))
                        {
                            continue;
                        }
                        return Err(Error::config(format!(
                            "{}: [plugins.{name}] options has no {}, which \"{}\" reads: {}",
                            entry.origin, need.key, rule.id, need.why
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Checks that every `[generate]` key names a generator its plugin declares. A key whose
    /// plugin no gate.toml declares fails earlier, when the configuration loads.
    ///
    /// # Errors
    /// Fails on the first key that names no generator of its plugin, naming the key and the
    /// generators the plugin declares.
    fn check_generators(&self, workspace: &Workspace) -> Result<()> {
        for entry in sync::generator_entries(workspace) {
            let item = entry.generator;
            let Some(plugin) = self
                .declarations
                .iter()
                .find(|(name, _)| name.as_str() == item.plugin())
                .map(|(_, declared)| declared)
            else {
                continue;
            };
            let declared = &plugin.info.generators;
            if declared
                .iter()
                .any(|generator| generator.name == item.name())
            {
                continue;
            }
            let hint = if declared.is_empty() {
                format!(
                    "plugin {} declares no generator; remove the entry",
                    item.plugin()
                )
            } else {
                let names: Vec<String> = declared
                    .iter()
                    .map(|generator| format!("{}/{}", item.plugin(), generator.name))
                    .collect();
                format!("its generators are {}", names.join(", "))
            };
            return Err(Error::config(format!(
                "{}: \"{item}\": plugin {} has no generator {}; {hint}",
                entry.origin,
                item.plugin(),
                item.name()
            )));
        }
        Ok(())
    }

    /// These plugins with every plugin that a `[generate]` entry of `workspace` names: `self` when
    /// those are loaded already, else loaded again with them, once, the first time a `sync-state`
    /// answer falls back to the run's configuration.
    ///
    /// # Errors
    /// Fails like [`load`].
    fn with_generators(self: &Arc<Self>, workspace: &Workspace) -> Result<Arc<Self>> {
        let missing: BTreeSet<PluginName> = sync::generator_entries(workspace)
            .iter()
            .filter_map(|entry| {
                self.declarations.keys().find(|name| {
                    name.as_str() == entry.generator.plugin() && !self.plugins.contains_key(*name)
                })
            })
            .cloned()
            .collect();
        if missing.is_empty() {
            return Ok(Arc::clone(self));
        }
        let lock = crate::lock::verify_with(workspace, &self.overrides.keys().cloned().collect())?;
        // The run's plugins load again with the missing ones: a `Loaded` holds one runtime, and a
        // plugin is instantiated by the runtime that compiled it. Their modules come from the cache.
        let mut names: BTreeSet<PluginName> = self.plugins.keys().cloned().collect();
        names.extend(missing);
        let loaded = load(workspace, lock.as_ref(), &self.overrides, &Needed::Only(names))?
            .ok_or_else(|| {
                Error::environment(
                    "the run's configuration declares no plugin although a [generate] entry names one; run gate again, and report it if it recurs",
                )
            })?;
        Ok(Arc::new(loaded))
    }

    fn rule(&self, plugin: &PluginName, rule: &str) -> Option<&RuleInfo> {
        self.declarations.get(plugin)?.rule(rule)
    }

    /// Whether the rule `rule` of `plugin` declares that it reads the commit message.
    fn reads_message(&self, plugin: &PluginName, rule: &str) -> bool {
        self.rule(plugin, rule)
            .is_some_and(|info| info.reads_message)
    }

    /// Runs every enabled rule `filter` allows in every scope `selection` runs, one plugin call per
    /// scope and plugin, and pushes the findings with their levels and norms from configuration.
    ///
    /// # Errors
    /// Fails when a call fails: a trap, a limit, an environment failure the plugin reports.
    pub(crate) fn run(
        self: &Arc<Self>,
        workspace: &Workspace,
        selection: &Selection,
        changes: Option<&ChangeSet>,
        run_git: RunGit,
        filter: &RuleFilter,
        findings: &mut Findings,
    ) -> Result<Ran> {
        // A run with the commit message being written checks only that message: the rules that
        // declare reads-message. The tree is the other hooks' and CI's to check.
        let message_run = run_git.message.is_some();
        let snapshot = workspace.shared_snapshot();
        let tree_paths: Vec<String> = snapshot.paths().map(str::to_owned).collect();
        let changed_paths: Option<Vec<String>> = changes
            .filter(|set| !set.everything)
            .map(|set| set.paths.iter().cloned().collect());
        let tree: Arc<dyn Tree> = Arc::new(SnapshotTree::new(
            Arc::clone(&snapshot),
            tree_paths.clone(),
            changed_paths.clone(),
        ));
        let git_history = Arc::new(GitHistory::new(
            &snapshot,
            workspace,
            run_git,
            filter,
            Arc::clone(self),
        ));
        let history: Arc<dyn History> = Arc::clone(&git_history) as Arc<dyn History>;

        let mut planned = Vec::new();
        for (project, settings) in scopes(workspace) {
            let runs = match project {
                None => selection.repository,
                Some(name) => selection.runs(name),
            };
            if !runs {
                continue;
            }
            let root = project
                .and_then(|name| workspace.projects().get(name))
                .map(|project| project.root.clone());
            let decisions = project.map_or_else(Vec::new, |name| rule_decisions(workspace, name));
            for name in settings.plugins.keys() {
                let Some(plugin) = self.plugins.get(name) else {
                    continue;
                };
                let mut rules = enabled_rules(plugin.info(), name, settings, workspace, project)?;
                rules.retain(|rule| {
                    filter.allows(&rule.id.to_string())
                        && (!message_run || self.reads_message(name, &rule.run.name))
                });
                if rules.is_empty() {
                    continue;
                }
                let options_files: Vec<&str> = rules
                    .iter()
                    .filter_map(|rule| rule.run.options_file.as_deref())
                    .collect();
                let grant = grant(root.as_deref(), settings, name, &options_files);
                let request = check_request(root.as_deref(), settings, name, &rules, &decisions);
                planned.push((project, settings, name, rules, plugin, request, grant));
            }
        }

        let blob = |path: &str| snapshot.blob_id(path).map(|id| id.as_str().to_owned());
        let size = |path: &str| snapshot.size(path);
        let inputs = results::Inputs {
            paths: &tree_paths,
            blob: &blob,
            size: &size,
            changed: changed_paths.as_deref(),
        };
        let calls: Vec<_> = planned
            .iter()
            .map(|(_, _, _, _, plugin, request, grant)| (*plugin, request, grant))
            .collect();
        let outcomes = self.calls(&calls, &tree, &history, &inputs, &git_history.writes);

        let mut ran = Ran::default();
        for ((project, settings, name, rules, _, _, _), looked) in planned.iter().zip(outcomes) {
            let outcome = looked.outcome.map_err(|error| {
                call_failed(
                    &error,
                    project.and_then(|name| workspace.projects().get(name)),
                )
            })?;
            for finding in outcome.findings {
                if let Some(finding) = reported(name, settings, rules, *project, finding)? {
                    findings.push(finding);
                }
            }
            for rule in rules {
                ran.per_scope.insert(*project, &rule.id.to_string());
            }
            for judged in outcome.judged {
                let rule = format!("{name}/{}", judged.rule);
                ran.per_scope.narrow(*project, &rule, judged.targets);
                ran.per_scope.rejudge(*project, &rule, judged.rejudged);
            }
            ran.rules += rules.len();
            ran.reads.push(PluginReads {
                project: project.map(str::to_owned),
                plugin: name.to_string(),
                read_set: outcome.read_set,
                from_cache: looked.from_cache,
                stale: looked.stale,
                time: outcome.time,
                fuel_used: outcome.fuel_used,
                invocations: outcome.invocations,
            });
        }
        git_history.finish(&mut ran);
        Ok(ran)
    }

    /// Runs each of `calls` (plugin, request, grant): from the result cache when every input it
    /// recorded answers as it did, else on the worker pool, and stores what a run call answers.
    /// The outcomes are in the order of `calls`, each with whether it came from the cache.
    /// A result that cannot be stored is noted in `writes`.
    fn calls(
        &self,
        calls: &[(&Plugin, &gate_sdk::CheckRequest, &Grant)],
        tree: &Arc<dyn Tree>,
        history: &Arc<dyn History>,
        inputs: &results::Inputs<'_>,
        writes: &CacheWrites,
    ) -> Vec<Looked> {
        let cache = Cache::from_env().ok();
        let keys: Vec<Sha256Sum> = calls
            .iter()
            .map(|(plugin, request, grant)| results::key(plugin.sha256(), request, grant))
            .collect();
        // Why each call that had a stored result runs again.
        let mut stale: Vec<Option<String>> = vec![None; calls.len()];
        let mut outcomes: Vec<Option<Looked>> = keys
            .iter()
            .zip(&mut stale)
            .enumerate()
            .map(|(index, (key, stale))| {
                let started = Instant::now();
                let stored = results::load(cache.as_ref()?, calls[index].0.sha256(), key)?;
                if let Some(changed) = stored.changed_input(inputs, Some(history.as_ref())) {
                    *stale = Some(changed);
                    return None;
                }
                let checked = started.elapsed();
                let outcome = CallOutcome {
                    findings: stored.findings,
                    judged: stored.judged,
                    read_set: stored.read_set,
                    log: Vec::new(),
                    time: CallTime {
                        elapsed: checked,
                        ..CallTime::default()
                    },
                    fuel_used: 0,
                    invocations: Vec::new(),
                };
                Some(Looked {
                    outcome: Ok(outcome),
                    from_cache: true,
                    stale: None,
                })
            })
            .collect();
        let missed: Vec<usize> = (0..calls.len())
            .filter(|index| outcomes[*index].is_none())
            .collect();
        let run: Vec<Call<'_>> = missed
            .iter()
            .map(|index| {
                let (plugin, request, grant) = calls[*index];
                Call {
                    plugin,
                    request: request.clone(),
                    tree: Arc::clone(tree),
                    history: Some(Arc::clone(history)),
                    grant: grant.clone(),
                }
            })
            .collect();
        for (index, outcome) in missed.iter().zip(self.runtime.check_all(&run)) {
            if let (Some(cache), Ok(outcome)) = (cache.as_ref(), outcome.as_ref()) {
                let stored = results::Stored::new(
                    outcome.findings.clone(),
                    outcome.read_set.clone(),
                    inputs,
                )
                .with_judged(outcome.judged.clone());
                let plugin = calls[*index].0.sha256();
                writes.note(results::store(cache, plugin, &keys[*index], &stored));
            }
            outcomes[*index] = Some(Looked {
                outcome,
                from_cache: false,
                stale: stale[*index].take(),
            });
        }
        outcomes.into_iter().flatten().collect()
    }
}

/// A plugin's finding as the report holds it, with its level and norm from `settings`; `None` when
/// configuration turns its rule or aspect off. The runtime has already refused a finding of a rule
/// the call did not run or of an undeclared aspect.
fn reported(
    name: &PluginName,
    settings: &Settings,
    rules: &[Enabled],
    project: Option<&str>,
    finding: gate_sdk::Finding,
) -> Result<Option<Finding>> {
    let id = rule_id(name, &finding.rule)?;
    let preset = rules
        .iter()
        .find(|rule| rule.id == id)
        .map(|rule| &rule.preset);
    let level = match level(settings, &id, &finding.aspect, preset) {
        Some(Setting::Off) => return Ok(None),
        Some(Setting::On(level)) => level,
        None => Level::Error,
    };
    Ok(Some(Finding {
        norm: settings.norm(&id, &finding.aspect).map(str::to_owned),
        rule: id.to_string(),
        aspect: finding.aspect,
        level,
        message: finding.message,
        target: finding.target.or_else(|| match &finding.location {
            Some(gate_sdk::Location::Commit(commit)) => Some(commit.clone()),
            _ => None,
        }),
        location: finding.location.map(location),
        help: finding.help,
        project: project.map(str::to_owned),
        key: finding.key,
    }))
}

/// A failed call of a plugin as the user reads it: a configuration error when the plugin answered
/// that its options or settings cannot be used, otherwise an environment error. A call for a
/// project addresses every path relative to the project's root, and so does the plugin's own error
/// text, which the core cannot rewrite (it cannot tell a path in it); so the error names the
/// project and says its paths are relative to the project's root. A call for the repository, or
/// for a project at the repository's root, addresses repository paths and is named as the runtime
/// names it.
fn call_failed(error: &gate_runtime::Error, project: Option<&Project>) -> Error {
    let message = match project {
        Some(project) if !project.root.is_empty() && !error.plugin().is_empty() => {
            format!(
                "plugin {} in project {} (its paths are relative to {}): {}",
                error.plugin(),
                project.name,
                project.root,
                error.message()
            )
        }
        _ => error.to_string(),
    };
    if error.kind() == gate_runtime::ErrorKind::Configuration {
        Error::config(message)
    } else {
        Error::environment(message)
    }
}

/// What `plugin`'s calls in a scope may read: the project's `root` (`None`: the repository), the
/// extra paths the scope's declaration of the plugin reads, and `options_files`, the repository
/// paths of the option files of the rules the call runs: each file alone, not its directory, so a
/// rule can read the file its options came from wherever it lies (a root profile's, outside the
/// project).
fn grant(
    root: Option<&str>,
    settings: &Settings,
    plugin: &PluginName,
    options_files: &[&str],
) -> Grant {
    let Some(root) = root else {
        return Grant::repository();
    };
    let mut reads: Vec<String> = settings
        .plugins
        .get(plugin)
        .map(|declared| {
            declared
                .value
                .reads
                .iter()
                .map(|path| path.as_str().to_owned())
                .collect()
        })
        .unwrap_or_default();
    reads.extend(options_files.iter().map(|file| (*file).to_owned()));
    Grant::new(root, &reads)
}

/// The plugins' generators and rule declarations as `gate sync` asks for them: generators run over
/// the workspace's snapshot with their project's grant.
pub struct PluginGenerators<'w> {
    workspace: &'w Workspace,
    loaded: Option<Arc<Loaded>>,
    /// One line per `--plugin` build: name, path and sha256.
    overrides: Vec<String>,
    /// Whether a generator call failed in a way that may pass by the next call (the plugin
    /// reported an environment failure, trapped, or the runtime could not start). A call failure
    /// reaches the user as in `gate check`: a configuration error when the plugin reported its
    /// options or settings unusable, an environment error otherwise; sync-state reads this to
    /// decide whether an answer may be stored.
    passing: std::sync::atomic::AtomicBool,
    /// What the generators reported while they ran, as `gate/sync#<generator>` findings.
    findings: Mutex<Vec<Finding>>,
    /// How many generators ran and how many the result cache answered.
    generations: Arc<Generations>,
}

impl std::fmt::Debug for PluginGenerators<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginGenerators")
            .field("loaded", &self.loaded.is_some())
            .finish_non_exhaustive()
    }
}

impl<'w> PluginGenerators<'w> {
    /// Loads every plugin the configuration declares, generators or not: the local build of each
    /// `overrides` entry for this run, else its `gate.lock` pin, whose lock check skips the
    /// plugins an override covers. So every plugin is compiled into the module cache before a hook
    /// needs it. Then checks the whole configuration against what they declare.
    ///
    /// # Errors
    /// Fails like `gate check` loading its plugins.
    pub fn load(workspace: &'w Workspace, overrides: &[crate::PluginOverride]) -> Result<Self> {
        let (lines, files) = crate::resolve_overrides(workspace, overrides)?;
        let lock = crate::lock::verify_with(workspace, &files.keys().cloned().collect())?;
        let loaded = load(workspace, lock.as_ref(), &files, &Needed::Every)?;
        if let Some(loaded) = &loaded {
            loaded.validate(workspace, &RuleFilter::new(Vec::new()))?;
        }
        Ok(Self {
            workspace,
            loaded: loaded.map(Arc::new),
            overrides: lines,
            passing: std::sync::atomic::AtomicBool::new(false),
            findings: Mutex::new(Vec::new()),
            generations: Arc::default(),
        })
    }

    /// The generators of `workspace` from plugins already loaded and checked against it, counted
    /// in `generations`.
    fn with_loaded(
        workspace: &'w Workspace,
        loaded: Option<Arc<Loaded>>,
        generations: Arc<Generations>,
    ) -> Self {
        Self {
            workspace,
            loaded,
            overrides: Vec::new(),
            passing: std::sync::atomic::AtomicBool::new(false),
            findings: Mutex::new(Vec::new()),
            generations,
        }
    }

    /// How many generators ran so far, and how many the result cache answered.
    #[must_use]
    pub fn generated(&self) -> Generated {
        self.generations.read()
    }

    /// What the first cache write that failed said, when one did: the outputs are complete, but
    /// the next run cannot reuse them.
    #[must_use]
    pub fn cache_warning(&self) -> Option<String> {
        self.generations.writes.take()
    }

    /// One line per `--plugin` build this run uses: name, path and sha256.
    #[must_use]
    pub fn overrides(&self) -> &[String] {
        &self.overrides
    }

    /// A generator call's failure for `project` (`None`: the repository) as the user sees it,
    /// naming the scope, as in `gate check` (see [`call_failed`]): a configuration error when the
    /// plugin reported a configuration error, an environment error otherwise. One that may pass by
    /// the next call is noted for [`PluginGenerators::had_passing_failure`].
    fn failed(&self, project: Option<&str>, error: &gate_runtime::Error) -> Error {
        if passing_call_failure(error.kind()) {
            self.passing.store(true, Ordering::Relaxed);
        }
        call_failed(
            error,
            project.and_then(|name| self.workspace.projects().get(name)),
        )
    }

    /// What the generators that ran so far reported, as `gate/sync#<generator>` findings at level
    /// error, sorted; taken, so a second call answers only what ran since.
    #[must_use]
    pub fn take_findings(&self) -> Vec<Finding> {
        let mut findings =
            std::mem::take(&mut *self.findings.lock().unwrap_or_else(PoisonError::into_inner));
        findings.sort();
        findings
    }

    /// Whether a generator call failed in a way that may pass by the next call.
    fn had_passing_failure(&self) -> bool {
        self.passing.load(Ordering::Relaxed)
    }

    /// The loaded plugin of `item`, with its name.
    fn plugin(&self, item: &PluginItem) -> Result<(&PluginName, &Plugin, &Loaded)> {
        let loaded = self.loaded.as_deref().ok_or_else(|| {
            Error::config(format!(
                "[generate] {item}: no gate.toml declares plugin {}; declare it in [plugins]",
                item.plugin()
            ))
        })?;
        let (name, plugin) = loaded
            .plugins
            .iter()
            .find(|(name, _)| name.as_str() == item.plugin())
            .ok_or_else(|| {
                Error::config(format!(
                    "[generate] {item}: no gate.toml declares plugin {}; declare it in [plugins]",
                    item.plugin()
                ))
            })?;
        Ok((name, plugin, loaded))
    }

    /// The grant of `name`'s calls for the project named `project` (`None`: the repository).
    fn grant(&self, project: Option<&str>, name: &PluginName) -> Grant {
        let root = project
            .and_then(|project| self.workspace.projects().get(project))
            .map(|project| project.root.as_str());
        grant(root, self.settings(project), name, &[])
    }

    /// The settings of the project named `project` (`None`: the repository).
    fn settings(&self, project: Option<&str>) -> &Settings {
        project
            .and_then(|project| self.workspace.settings(project))
            .unwrap_or_else(|| self.workspace.repository())
    }
}

impl Generators for PluginGenerators<'_> {
    fn generate(
        &self,
        project: Option<&str>,
        generator: &PluginItem,
        options: Option<&Value>,
    ) -> Result<Vec<SyncFile>> {
        let (name, plugin, loaded) = self.plugin(generator)?;
        let snapshot = self.workspace.shared_snapshot();
        let tree = Arc::new(SnapshotTree::new(
            Arc::clone(&snapshot),
            snapshot.paths().map(str::to_owned).collect(),
            None,
        ));
        let options = options.map_or_else(|| "{}".to_owned(), Value::to_string);
        let plugin_options = self
            .settings(project)
            .plugin_options(name)
            .map(Value::to_string)
            .unwrap_or_default();
        let grant = self.grant(project, name);
        let blob = |path: &str| snapshot.blob_id(path).map(|id| id.as_str().to_owned());
        let size = |path: &str| snapshot.size(path);
        let inputs = results::Inputs {
            paths: &tree.paths,
            blob: &blob,
            size: &size,
            changed: None,
        };
        let cache = Cache::from_env().ok();
        let key = results::generate_key(
            plugin.sha256(),
            generator.name(),
            &options,
            &plugin_options,
            &grant,
        );
        let stored = cache
            .as_ref()
            .and_then(|cache| results::load(cache, plugin.sha256(), &key))
            .filter(|stored| stored.changed_input(&inputs, None).is_none());
        let (files, findings) = if let Some(stored) = stored {
            self.generations.stored.fetch_add(1, Ordering::Relaxed);
            (stored.files, stored.findings)
        } else {
            let (output, read_set, _) = loaded
                .runtime
                .generate(
                    plugin,
                    Arc::clone(&tree) as Arc<dyn Tree>,
                    &grant,
                    generator.name(),
                    &options,
                    &plugin_options,
                )
                .map_err(|error| self.failed(project, &error))?;
            self.generations.ran.fetch_add(1, Ordering::Relaxed);
            if let Some(cache) = &cache {
                let stored =
                    results::Stored::generated(output.files, output.findings, read_set, &inputs);
                self.generations
                    .writes
                    .note(results::store(cache, plugin.sha256(), &key, &stored));
                (stored.files, stored.findings)
            } else {
                (output.files, output.findings)
            }
        };
        let reported = findings.into_iter().map(|finding| Finding {
            rule: sync::SYNC_RULE.to_owned(),
            aspect: generator.to_string(),
            level: Level::Error,
            message: finding.message,
            target: finding.target.or_else(|| match &finding.location {
                Some(gate_sdk::Location::Commit(commit)) => Some(commit.clone()),
                _ => None,
            }),
            location: finding.location.map(location),
            help: finding.help,
            norm: None,
            project: project.map(str::to_owned),
            key: finding.key,
        });
        self.findings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(reported);
        Ok(files
            .into_iter()
            .map(|file| SyncFile {
                path: file.path,
                contents: file.contents,
                seed: file.seed,
            })
            .collect())
    }

    fn owns(&self, project: Option<&str>, generator: &PluginItem) -> Result<Vec<String>> {
        let (name, plugin, _) = self.plugin(generator)?;
        plugin
            .owned(generator.name(), &self.grant(project, name))
            .map_err(|error| self.failed(project, &error))?
            .ok_or_else(|| {
                Error::config(format!(
                    "[generate] {generator}: plugin {name} declares no generator {}",
                    generator.name()
                ))
            })
    }

    fn reads_history(&self, filter: &RuleFilter) -> bool {
        let Some(loaded) = self.loaded.as_deref() else {
            return false;
        };
        scopes(self.workspace).any(|(project, settings)| {
            settings.plugins.keys().any(|name| {
                let Some(declared) = loaded.declarations.get(name) else {
                    return false;
                };
                enabled_rules(&declared.info, name, settings, self.workspace, project)
                    .unwrap_or_default()
                    .iter()
                    .any(|rule| {
                        filter.allows(&rule.id.to_string())
                            && loaded
                                .rule(name, rule.id.rule())
                                .is_some_and(|info| info.reads_history)
                    })
            })
        })
    }
}

/// The repository scope, then each project with settings, in name order.
fn scopes(workspace: &Workspace) -> impl Iterator<Item = (Option<&str>, &Settings)> {
    std::iter::once((None, workspace.repository())).chain(workspace.projects().iter().filter_map(
        |project| {
            workspace
                .settings(&project.name)
                .map(|settings| (Some(project.name.as_str()), settings))
        },
    ))
}

fn rule_id(plugin: &PluginName, rule: &str) -> Result<RuleId> {
    RuleId::parse(&format!("{plugin}/{rule}")).map_err(|error| {
        Error::environment(format!(
            "plugin {plugin} reported a finding of rule {rule:?}: {error}; the plugin is broken, report it to its authors"
        ))
    })
}

/// A key of the plugin's options a rule reads, as its options schema declares it.
struct Need {
    key: String,
    /// The plugin's options read it when they match this; always when `None`.
    when: Option<jsonschema::Validator>,
    why: String,
}

/// The needs rule `rule` of `plugin` declares under `gate_sdk::NEEDS_KEYWORD` of its options
/// schema.
///
/// # Errors
/// Fails, as a broken plugin, when they are not a list of `{ "key", "when", "why" }` with a string
/// key and reason and a JSON Schema or `null` for `when`.
fn needs(plugin: &PluginName, rule: &str, schema: &Value) -> Result<Vec<Need>> {
    let Some(list) = schema.get(gate_sdk::NEEDS_KEYWORD) else {
        return Ok(Vec::new());
    };
    let broken = |what: String| {
        Error::environment(format!(
            "plugin {plugin} declares the options rule {rule} reads ({}) {what}; the plugin is broken, report it to its authors",
            gate_sdk::NEEDS_KEYWORD
        ))
    };
    let Some(list) = list.as_array() else {
        return Err(broken("as something other than a list".to_owned()));
    };
    list.iter()
        .map(|need| -> Result<Need> {
            let text = |field: &str| {
                need.get(field)
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| broken(format!("with an entry whose {field} is not a string")))
            };
            let when = match need.get("when") {
                None | Some(Value::Null) => None,
                Some(when) => Some(jsonschema::validator_for(when).map_err(|error| {
                    broken(format!(
                        "with a when that is not valid JSON Schema ({error})"
                    ))
                })?),
            };
            Ok(Need {
                key: text("key")?,
                when,
                why: text("why")?,
            })
        })
        .collect()
}

/// A rule that runs in a call.
struct Enabled {
    id: RuleId,
    run: RuleRun,
    /// The levels its presets give it and its aspects.
    preset: PresetLevels,
}

/// The levels presets give a rule: the rule's, and its aspects'.
#[derive(Clone, Debug, Default)]
struct PresetLevels {
    level: Option<Setting>,
    levels: BTreeMap<String, Setting>,
}

/// The level of `aspect` of rule `id`, first found: the `[rules]` entry's level of the aspect, the
/// entry's level of the whole rule, the family's, then the presets' level of the aspect and of the
/// rule. The project's own configuration wins over a preset: a level `[rules]` gives the whole rule
/// (`off` included) applies to every aspect, and a preset's aspect level applies only where
/// `[rules]` sets none. `None` leaves the default, `error`.
fn level(
    settings: &Settings,
    id: &RuleId,
    aspect: &str,
    preset: Option<&PresetLevels>,
) -> Option<Setting> {
    let own = settings
        .rules
        .get(id)
        .and_then(|entry| entry.levels.get(aspect))
        .map(|level| level.value);
    // The entry's level, else the family's: what `[rules]` says of the whole rule.
    own.or_else(|| settings.setting(id, ""))
        .or_else(|| preset.and_then(|preset| preset.levels.get(aspect).copied().or(preset.level)))
}

/// The rules of `plugin` that run in this scope, with what their call carries: those whose scope
/// matches and that configuration enables (a `[rules]` entry, a family level or a preset), unless
/// everything about them is `off`.
fn enabled_rules(
    info: &PluginInfo,
    name: &PluginName,
    settings: &Settings,
    workspace: &Workspace,
    project: Option<&str>,
) -> Result<Vec<Enabled>> {
    let scope = if project.is_none() {
        Scope::Repository
    } else {
        Scope::Project
    };
    let presets = presets(info, name, settings)?;
    let mut rules = Vec::new();

    for info in info.rules.iter().filter(|info| info.scope == scope) {
        let id = rule_id(name, &info.name)?;
        let preset = presets.get(info.name.as_str());
        let entry = settings.rules.get(&id);
        if entry.is_none() && preset.is_none() && !settings.families.contains_key(name.as_str()) {
            continue;
        }
        let whole = settings.setting(&id, "");
        let level = whole.or_else(|| preset.and_then(|preset| preset.level));
        // An aspect a preset turns on keeps the rule running, unless `[rules]` turns it off.
        let any_aspect_on = entry.is_some_and(|entry| {
            entry
                .levels
                .values()
                .any(|level| matches!(level.value, Setting::On(_)))
        }) || (whole != Some(Setting::Off)
            && preset.is_some_and(|preset| {
                preset
                    .levels
                    .values()
                    .any(|level| matches!(level, Setting::On(_)))
            }));
        if level == Some(Setting::Off) && !any_aspect_on {
            continue;
        }
        let options = settings
            .options(&id)
            .or_else(|| preset.and_then(|preset| preset.options.as_ref()));
        // The root's file also accepts repository-scope findings (as `Findings::finish` applies
        // it), so a repository-scope call gets its exceptions too when the top is a project.
        let exceptions = workspace
            .exceptions()
            .iter()
            .filter(|exception| {
                (exception.project.as_deref() == project
                    || (project.is_none() && exception.is_root()))
                    && exception.rule.rule == id
            })
            .map(|exception| exception.target.clone())
            .collect();
        rules.push(Enabled {
            id,
            run: RuleRun {
                name: info.name.clone(),
                options: options.map(Value::to_string).unwrap_or_default(),
                options_file: entry
                    .and_then(|entry| entry.options.as_ref())
                    .and_then(|options| options.value.file.clone()),
                exceptions,
            },
            preset: PresetLevels {
                level: preset.and_then(|preset| preset.level),
                levels: preset
                    .map(|preset| preset.levels.clone())
                    .unwrap_or_default(),
            },
        });
    }
    Ok(rules)
}

/// One `extends` entry, the scopes it reached and whether its preset names a rule of any of them.
struct Reach<'a> {
    item: &'a Sourced<PluginItem>,
    repository: bool,
    project: bool,
    turns_on: bool,
}

impl Reach<'_> {
    /// Why the entry turns on nothing, and where it goes instead: the root's `[project] extends`
    /// when the root is a project, else its `[workspace] extends`.
    fn message(&self, root_is_project: bool) -> String {
        let entry = format!("{}: \"{}\"", self.item.origin, self.item.value);
        let root = if root_is_project {
            "the root gate.toml's [project] extends"
        } else {
            "the root's [workspace] extends"
        };
        match (self.repository, self.project) {
            (true, false) => format!(
                "{entry} turns on no repository-scope rule, and the root runs only those; extend it in a [[profiles]] entry for the projects"
            ),
            (false, true) => {
                format!("{entry} turns on no project-scope rule; extend it in {root}")
            }
            _ => format!("{entry} turns on no rule; remove it"),
        }
    }
}

/// Whether the preset `preset` of the plugin names a rule of `scope`; a rule it sets `off` counts,
/// as the preset says something about it. A preset that is missing or does not read names none:
/// [`presets`] reports those first.
fn preset_turns_on(info: &PluginInfo, preset: &str, scope: Scope) -> bool {
    let Some(preset) = info.presets.iter().find(|known| known.name == preset) else {
        return false;
    };
    let Ok(table) = serde_json::from_str::<BTreeMap<String, Value>>(&preset.rules) else {
        return false;
    };
    table.keys().any(|rule| {
        info.rules
            .iter()
            .any(|known| known.name == *rule && known.scope == scope)
    })
}

/// One rule of a preset: `{ "level": .., "levels": { <aspect>: .. }, "options": .. }`.
#[derive(Debug)]
struct PresetRule {
    /// The preset that set the rule, `<plugin>/<preset>`.
    preset: String,
    level: Option<Setting>,
    levels: BTreeMap<String, Setting>,
    options: Option<Value>,
    /// The `extends` entry of the preset that set the rule.
    extended: Sourced<PluginItem>,
}

/// How strict a level is: `off` below `note` below `warning` below `error`.
fn strictness(setting: Setting) -> u8 {
    match setting {
        Setting::Off => 0,
        Setting::On(Level::Note) => 1,
        Setting::On(Level::Warning) => 2,
        Setting::On(Level::Error) => 3,
    }
}

/// Where a level is written, as the user wrote its key.
enum Written<'a> {
    /// The level of a rule entry: `"<plugin>/<rule>" = "<level>"` or `{ level = ... }`.
    Rule(&'a RuleId),
    /// The level of a family wildcard: `"<plugin>/*"`.
    Family(&'a PluginName),
    /// A level in a rule entry's `levels`.
    Aspect(&'a RuleId, &'a str),
}

/// The level a rule or one of its aspects has without the entry, and where a preset sets it.
struct Floor {
    /// What the level applies to: `<rule>` or `<rule>#<aspect>`.
    subject: String,
    /// The level.
    level: Setting,
    /// The preset and what it sets: `the preset p/q sets <rule>[#<aspect>] to <level>`.
    source: String,
}

impl Floor {
    /// The default of `aspect` of `id` (`None`: the whole rule) that the preset's rule level
    /// gives.
    fn rule(id: &RuleId, aspect: Option<&str>, level: Setting, preset: &PresetRule) -> Self {
        Self {
            subject: aspect.map_or_else(|| id.to_string(), |aspect| format!("{id}#{aspect}")),
            level,
            source: format!("the preset {} sets {id} to {level}", preset.preset),
        }
    }

    /// The default of `aspect` of `id` that the preset's level of that aspect gives.
    fn aspect(id: &RuleId, aspect: &str, level: Setting, preset: &PresetRule) -> Self {
        Self {
            subject: format!("{id}#{aspect}"),
            level,
            source: format!("the preset {} sets {id}#{aspect} to {level}", preset.preset),
        }
    }
}

/// Checks that `level`, written at `written`, names a decision when it is below the `default` a
/// preset gives and a `[decision-policy]` covering the file that wrote it requires one.
///
/// # Errors
/// A configuration error naming the key as written, the level, the default and the preset that
/// sets it, the policy, and the form that adds the decision.
fn decided(
    workspace: &Workspace,
    written: &Written<'_>,
    level: &LevelSetting,
    default: &Floor,
) -> Result<()> {
    if level.decision.is_some() || strictness(level.value) >= strictness(default.level) {
        return Ok(());
    }
    let Some(policy) = workspace.decision_policy(&level.origin.file) else {
        return Ok(());
    };
    let value = level.value;
    let (key, advice) = match written {
        Written::Rule(id) => (
            format!("\"{id}\" (level \"{value}\")"),
            format!(
                "write it as \"{id}\" = {{ level = \"{value}\", decision = \"<record>\" }}, or add decision = \"<record>\" to its table"
            ),
        ),
        Written::Family(plugin) => (
            format!("\"{plugin}/*\" (level \"{value}\")"),
            format!(
                "write it as \"{plugin}/*\" = {{ level = \"{value}\", decision = \"<record>\" }}"
            ),
        ),
        Written::Aspect(id, aspect) => (
            format!("\"{id}\" (levels.{aspect} \"{value}\")"),
            format!("add decision = \"<record>\" to the \"{id}\" table"),
        ),
    };
    Err(Error::config(format!(
        "{}: {key} sets {} below its default {} ({}), and {policy} [decision-policy] requires a decision for that; {advice}",
        level.origin, default.subject, default.level, default.source
    )))
}

/// The rules of the presets these settings extend from `plugin`, later presets over earlier.
fn presets(
    info: &PluginInfo,
    name: &PluginName,
    settings: &Settings,
) -> Result<BTreeMap<String, PresetRule>> {
    let mut rules = BTreeMap::new();
    for item in settings
        .extends
        .iter()
        .filter(|item| item.value.plugin() == name.as_str())
    {
        let Some(preset) = info
            .presets
            .iter()
            .find(|preset| preset.name == item.value.name())
        else {
            return Err(Error::config(format!(
                "{}: plugin {name} has no preset {}; run gate list to see its presets",
                item.origin,
                item.value.name()
            )));
        };
        let broken = |problem: String| {
            Error::environment(format!(
                "plugin {name}: preset {} {problem}; the plugin is broken, report it to its authors",
                preset.name
            ))
        };
        let table: BTreeMap<String, Value> = serde_json::from_str(&preset.rules)
            .map_err(|error| broken(format!("is not a JSON object of rules: {error}")))?;
        for (rule, value) in table {
            let level = match value.get("level") {
                None => None,
                Some(Value::String(level)) => Some(
                    Setting::parse(level)
                        .map_err(|error| broken(format!("sets rule {rule}: {error}")))?,
                ),
                Some(other) => {
                    return Err(broken(format!(
                        "sets rule {rule} to the level {other}, which is not a string"
                    )));
                }
            };
            let mut levels = BTreeMap::new();
            match value.get("levels") {
                None => {}
                Some(Value::Object(table)) => {
                    let declared = info
                        .rules
                        .iter()
                        .find(|info| info.name == rule)
                        .map(|info| info.aspects.as_slice())
                        .unwrap_or_default();
                    for (aspect, level) in table {
                        if !declared.contains(aspect) {
                            return Err(broken(format!(
                                "sets aspect {aspect} of rule {rule}, which the rule does not declare"
                            )));
                        }
                        let Value::String(level) = level else {
                            return Err(broken(format!(
                                "sets aspect {aspect} of rule {rule} to the level {level}, which is not a string"
                            )));
                        };
                        let level = Setting::parse(level).map_err(|error| {
                            broken(format!("sets aspect {aspect} of rule {rule}: {error}"))
                        })?;
                        levels.insert(aspect.clone(), level);
                    }
                }
                Some(other) => {
                    return Err(broken(format!(
                        "sets the levels of rule {rule} to {other}, which is not a table of aspects"
                    )));
                }
            }
            rules.insert(
                rule,
                PresetRule {
                    preset: format!("{name}/{}", preset.name),
                    level,
                    levels,
                    options: value.get("options").cloned(),
                    extended: item.clone(),
                },
            );
        }
    }
    Ok(rules)
}

/// The `check` call of `name` on the project at `root` (the repository when `None`), for `rules`,
/// with the rule `decisions` the scope's own `gate.toml` names.
fn check_request(
    root: Option<&str>,
    settings: &Settings,
    name: &PluginName,
    rules: &[Enabled],
    decisions: &[gate_sdk::RuleDecision],
) -> gate_sdk::CheckRequest {
    gate_sdk::CheckRequest {
        project: root.unwrap_or_default().to_owned(),
        norms: norms(settings),
        rule_decisions: decisions.to_vec(),
        rules: rules.iter().map(|rule| rule.run.clone()).collect(),
        plugin_options: settings
            .plugin_options(name)
            .map(Value::to_string)
            .unwrap_or_default(),
        languages: languages(settings),
    }
}

/// Every norm the configuration of this scope maps, without repeats: of every configured rule,
/// of every plugin and the core's, whatever its level, as `check-request.norms` promises.
fn norms(settings: &Settings) -> Vec<String> {
    let mut norms = BTreeSet::new();
    for entry in settings.rules.values() {
        norms.extend(entry.norm.iter().map(|norm| norm.value.clone()));
        norms.extend(entry.norms.values().map(|norm| norm.value.clone()));
    }
    norms.into_iter().collect()
}

/// The decisions the rule entries of project `project`'s own `gate.toml` name, as
/// `check-request.rule-decisions` promises: its own file's, which for the root project is the root
/// file. Their paths are repository paths; the runtime gives them to the plugin relative to its
/// scope's root. A project's call never carries the root file's decisions, which a path relative
/// to the project's root could not name.
fn rule_decisions(workspace: &Workspace, project: &str) -> Vec<gate_sdk::RuleDecision> {
    let Some(config) = workspace.project_config(project) else {
        return Vec::new();
    };
    config
        .rule_decisions()
        .into_iter()
        .map(|written| gate_sdk::RuleDecision {
            key: written.key,
            decision: written.decision,
            at: gate_sdk::FileLocation {
                path: config.path().to_owned(),
                span: Some(gate_sdk::Span::line(written.line)),
            },
        })
        .collect()
}

/// The `[languages.<id>]` settings as one JSON object by id; empty when there are none.
fn languages(settings: &Settings) -> String {
    if settings.languages.is_empty() {
        return String::new();
    }
    let languages: serde_json::Map<String, Value> = settings
        .languages
        .iter()
        .map(|(id, language)| (id.clone(), language.value.clone()))
        .collect();
    Value::Object(languages).to_string()
}

/// JSON Schema validation of rule options.
#[derive(Clone, Copy, Debug, Default)]
pub struct JsonSchemaValidator;

impl SchemaValidator for JsonSchemaValidator {
    fn validate(&self, schema: &Value, value: &Value) -> Vec<String> {
        match jsonschema::validator_for(schema) {
            Err(error) => vec![format!(
                "the plugin declares an options schema that is not valid JSON Schema ({error}); report it to its authors"
            )],
            Ok(validator) => validator
                .iter_errors(value)
                .map(|error| {
                    let path = error.instance_path().to_string();
                    if path.is_empty() {
                        error.to_string()
                    } else {
                        format!("{path}: {error}")
                    }
                })
                .collect(),
        }
    }
}

/// The snapshot as the plugin runtime reads it.
struct SnapshotTree {
    snapshot: Arc<Snapshot>,
    paths: Vec<String>,
    changed: Option<Vec<String>>,
}

impl SnapshotTree {
    /// The tree over `snapshot`, with its tracked `paths` in byte order, each once, as
    /// [`Tree::paths`] promises.
    fn new(snapshot: Arc<Snapshot>, paths: Vec<String>, changed: Option<Vec<String>>) -> Self {
        debug_assert!(
            paths.windows(2).all(|pair| pair[0] < pair[1]),
            "the tracked paths are sorted by byte order, each once"
        );
        Self {
            snapshot,
            paths,
            changed,
        }
    }
}

impl Tree for SnapshotTree {
    fn paths(&self) -> &[String] {
        &self.paths
    }

    fn read(&self, path: &str) -> std::result::Result<Option<Blob>, HostError> {
        let Some(bytes) = self.snapshot.read(path).map_err(host)? else {
            return Ok(None);
        };
        let id = self
            .snapshot
            .blob_id(path)
            .map(|id| id.as_str().to_owned())
            .unwrap_or_default();
        Ok(Some(Blob {
            id,
            bytes: bytes.to_vec(),
        }))
    }

    fn size(&self, path: &str) -> std::result::Result<Option<Size>, HostError> {
        let Some(bytes) = self.snapshot.size(path).map_err(host)? else {
            return Ok(None);
        };
        let id = self
            .snapshot
            .blob_id(path)
            .map(|id| id.as_str().to_owned())
            .unwrap_or_default();
        Ok(Some(Size { id, bytes }))
    }

    fn changed(&self) -> Option<&[String]> {
        self.changed.as_deref()
    }
}

/// A finding's location in a report: a repository path, or a commit's full id.
fn location(location: gate_sdk::Location) -> Location {
    match location {
        gate_sdk::Location::Commit(commit) => Location::Commit { commit },
        gate_sdk::Location::File(file) => Location::File {
            path: file.path,
            span: file.span.map(|span| Span {
                line: span.line,
                column: span.column,
                end_line: span.end_line,
                end_column: span.end_column,
            }),
        },
    }
}

/// The repository's history as the plugin runtime reads it.
/// What a run's git interface answers about the run itself, beyond its snapshot: the range of
/// `--rev` or `--push`, the message of `--commit-message` and whether a hook started it.
#[derive(Clone, Debug, Default)]
pub(crate) struct RunGit {
    /// The run's range under `--rev` or `--push`.
    pub(crate) range: Option<CommitRange>,
    /// The commit message under review.
    pub(crate) message: Option<String>,
    /// A git hook started the run.
    pub(crate) hook: bool,
}

struct GitHistory {
    git: Git,
    /// The commit of the run's snapshot: the `--rev` commit, or `HEAD`.
    head: Option<String>,
    /// The run's range under `--rev` or `--push`.
    range: Option<CommitRange>,
    /// The commit message under review (`--commit-message`).
    message: Option<String>,
    /// A git hook started the run (`--hook`).
    hook: bool,
    /// The run's `--plugin` builds, used again for the generators at another commit.
    overrides: Arc<Overrides>,
    /// The run's plugins, which also answer `sync-state` under the run's configuration.
    run_loaded: Arc<Loaded>,
    /// The run's `--rule` filter: the run's configuration is checked with it again when it
    /// answers `sync-state`, as when the run started.
    filter: RuleFilter,
    /// Each `--plugin` build's name and sha256, as every `sync-state` key holds them; hashed once.
    builds: OnceLock<String>,
    /// The run's workspace: its configuration answers `sync-state` at a commit whose own
    /// configuration this core does not read.
    run_workspace: Workspace,
    /// What `sync-state` needs of the run's configuration, computed once on first use.
    run: OnceLock<std::result::Result<RunContext, Error>>,
    /// The plugins loaded for commits' own configurations, by what they were loaded from.
    commit_plugins: Mutex<BTreeMap<Sha256Sum, CommitPlugins>>,
    /// The `sync-state` answers of this run, by commit id, each computed once.
    states: Mutex<BTreeMap<String, Arc<StateSlot>>>,
    /// How many `sync-state` answers this run computed, and how many it read from the cache.
    counted: SyncStateCount,
    /// The stored answers to questions about full commit ids, read on first use; `None` inside
    /// when the cache is not available.
    answers: OnceLock<Option<Mutex<Answers>>>,
    /// What the answers depend on beyond the commits: git's version, and whether the clone is
    /// shallow.
    facts: OnceLock<std::result::Result<Facts, HostError>>,
    /// The one `git cat-file --batch` reader that answers `tree`, `changed-paths`, `read-at` and
    /// `is-ancestor`, started on first use.
    objects: Mutex<Option<Objects>>,
    /// The one `git merge-tree --stdin` that answers `merge-tree`, started on first use.
    merges: Mutex<Batch<Merges>>,
    /// The run's cache writes that failed.
    writes: CacheWrites,
    /// What the snapshots of the commits `sync-state` reads share: listings, blob contents, a
    /// reader.
    snapshots: Arc<crate::snapshot::Shared>,
    /// The content of each blob `read-at` read from git this run, by id, stored at its end.
    blobs: Mutex<BTreeMap<String, Vec<u8>>>,
    /// The generators `sync-state` ran, and those the result cache answered.
    generations: Arc<Generations>,
}

/// A long-lived git process of a run.
#[derive(Debug, Default)]
enum Batch<T> {
    /// Not asked yet.
    #[default]
    Unstarted,
    /// Running.
    Running(T),
    /// Failed where a git process per question answered: this git cannot run it.
    Unusable,
}

/// The `sync-state` answer of one commit in a run.
type StateSlot = OnceLock<std::result::Result<SyncState, HostError>>;

/// How many `sync-state` answers a run computed and how many it read from the cache.
#[derive(Debug, Default)]
struct SyncStateCount {
    computed: AtomicUsize,
    stored: AtomicUsize,
    /// Questions about full commit ids asked of git, and answered by stored answers.
    asked: AtomicUsize,
    answered: AtomicUsize,
}

impl SyncStateCount {
    fn read(&self) -> (SyncStates, GitAnswers) {
        (
            SyncStates {
                computed: self.computed.load(Ordering::Relaxed),
                stored: self.stored.load(Ordering::Relaxed),
            },
            GitAnswers {
                asked: self.asked.load(Ordering::Relaxed),
                stored: self.answered.load(Ordering::Relaxed),
            },
        )
    }
}

/// The run's configuration as `sync-state` uses it, computed once per run.
struct RunContext {
    /// The hash of the files the run's configuration was read from, `gate.lock` among them.
    hash: Sha256Sum,
    /// The run's plugins, loaded and checked against its configuration.
    loaded: Option<Arc<Loaded>>,
    /// The paths each generator writes under the run's configuration, by project and generator,
    /// computed on first use.
    outputs: OnceLock<RunOutputs>,
    /// The hash a run-dependent answer is stored under: the configuration's and the run outputs',
    /// computed on first use.
    key: OnceLock<Sha256Sum>,
}

/// The paths each generator writes under the run's configuration, on the run's tree.
struct RunOutputs {
    by_entry: BTreeMap<EntryKey, BTreeSet<String>>,
    /// Whether a generator failed with an environment error, so these outputs may differ next run.
    transient: bool,
}

/// The variable CI sets to the branch under test: a pull request's checkout is a detached `HEAD`,
/// which names no branch.
pub const BRANCH_VARIABLE: &str = "GATE_BRANCH";

/// The branch under test: `branch_variable` (the value of [`BRANCH_VARIABLE`]) when it is set and
/// not empty, else the branch `HEAD` is on.
fn current_branch(branch_variable: Option<String>, git: &Git) -> Result<Option<String>> {
    match branch_variable
        .map(|branch| branch.trim().to_owned())
        .filter(|branch| !branch.is_empty())
    {
        Some(branch) => Ok(Some(branch)),
        None => git.branch(),
    }
}

/// The plugins a commit's configuration declares, as this core loads them.
#[derive(Clone)]
struct CommitPlugins {
    loaded: Option<Arc<Loaded>>,
    /// The plugins whose component fetched and matched its pin but does not load under this core:
    /// it does not compile, link or declare itself validly, or breaks the interface.
    unloadable: BTreeSet<PluginName>,
}

/// A `[generate]` entry: its project (`None`: the repository) and `<plugin>/<generator>`.
type EntryKey = (Option<String>, String);

/// The fields of one `git log -z` record: id, parents, raw message, author date.
const LOG_FIELDS: usize = 4;

impl History for GitHistory {
    fn head(&self) -> std::result::Result<Option<String>, HostError> {
        Ok(self.head.clone())
    }

    fn branch(&self) -> std::result::Result<Option<String>, HostError> {
        current_branch(std::env::var(BRANCH_VARIABLE).ok(), &self.git).map_err(host)
    }

    fn range(&self) -> std::result::Result<Option<CommitRange>, HostError> {
        Ok(self.range.clone())
    }

    fn message(&self) -> std::result::Result<Option<String>, HostError> {
        Ok(self.message.clone())
    }

    fn hook(&self) -> std::result::Result<bool, HostError> {
        Ok(self.hook)
    }

    fn resolve(&self, rev: &str) -> std::result::Result<Option<String>, HostError> {
        self.git.resolve_commit(rev).map_err(host)
    }

    fn tree(&self, commit: &str) -> std::result::Result<String, HostError> {
        let answer = self.remembered("tree", &[commit], true, || {
            Ok(kept(Answer::Tree(Some(
                self.objects(|objects| objects.tree(commit))?
                    .ok_or_else(|| {
                        HostError::new(format!("{commit:?} names no commit, so it has no tree"))
                    })?,
            ))))
        })?;
        match answer {
            Answer::Tree(Some(tree)) => Ok(tree),
            _ => Err(HostError::new(format!(
                "{commit:?} names no commit, so it has no tree"
            ))),
        }
    }

    fn merge_tree(
        &self,
        first: &str,
        second: &str,
    ) -> std::result::Result<Option<String>, HostError> {
        // git's merge may differ between versions: the version is part of the question. A
        // shallow clone may lack the merge base: its answers are not kept for another clone.
        let facts = self.facts()?;
        let version = facts.version.clone();
        let lasting = !facts.shallow;
        let answer = self.remembered("merge-tree", &[first, second, &version], lasting, || {
            Ok(kept(Answer::Tree(self.merge(first, second)?)))
        })?;
        match answer {
            Answer::Tree(tree) => Ok(tree),
            _ => Err(HostError::new("a stored merge-tree answer is not a tree")),
        }
    }

    fn revisions(&self, range: &str) -> std::result::Result<Vec<String>, HostError> {
        let output = self
            .git
            .output(&["rev-parse", "--revs-only", "--end-of-options", range])
            .map_err(host)?;
        Ok(String::from_utf8_lossy(&output)
            .split_whitespace()
            .map(str::to_owned)
            .collect())
    }

    fn log(&self, range: &str) -> std::result::Result<Vec<Commit>, HostError> {
        // `-z` ends each record with NUL and the format separates fields with NUL, which a commit
        // message cannot hold: any other byte of a message is safe.
        let output = self
            .git
            .output(&[
                "log",
                "-z",
                "--format=%H%x00%P%x00%B%x00%aI",
                "--end-of-options",
                range,
                "--",
            ])
            .map_err(host)?;
        let text = String::from_utf8_lossy(&output);
        let mut fields: Vec<&str> = text.split('\0').collect();
        if fields.last() == Some(&"") {
            fields.pop();
        }
        let (records, rest) = fields.as_chunks::<LOG_FIELDS>();
        if !rest.is_empty() {
            return Err(HostError::new(format!(
                "git log {range} printed {} NUL-separated fields, not records of {LOG_FIELDS}; check the git version",
                fields.len()
            )));
        }
        Ok(records
            .iter()
            .map(|[id, parents, message, author_date]| Commit {
                id: (*id).to_owned(),
                parents: parents.split_whitespace().map(str::to_owned).collect(),
                message: (*message).to_owned(),
                author_date: (*author_date).to_owned(),
            })
            .collect())
    }

    fn changed_paths(
        &self,
        parent: &str,
        commit: &str,
    ) -> std::result::Result<Vec<String>, HostError> {
        let answer = self.remembered("changed-paths", &[parent, commit], true, || {
            Ok(kept(Answer::Paths(self.objects(|objects| {
                objects.changed_paths(parent, commit)
            })?)))
        })?;
        match answer {
            Answer::Paths(paths) => Ok(paths),
            _ => Err(HostError::new(
                "a stored changed-paths answer is not a path list",
            )),
        }
    }

    fn read_at(&self, commit: &str, path: &str) -> std::result::Result<Option<Vec<u8>>, HostError> {
        let mut read = None;
        let answer = self.remembered("read-at", &[commit, path], true, || {
            let found = self
                .objects(|objects| objects.read_at(commit, path))
                .map_err(|error| self.unread(commit, path, error))?;
            if let Some((blob, content)) = found {
                read = Some(content);
                return Ok(kept(Answer::Blob(Some(blob))));
            }
            // A commit the clone lacks may come with a fetch: no file, not kept.
            if !self.objects(|objects| objects.has_commit(commit))? {
                return Ok((Answer::Blob(None), false));
            }
            // No file only when the commit's tree names none: a partial clone that cannot fetch
            // the file has not answered.
            if !self.objects(|objects| objects.names_a_file(commit, path))? {
                return Ok(kept(Answer::Blob(None)));
            }
            Err(unfetched(commit, path, "git reads it as missing"))
        })?;
        let Answer::Blob(blob) = answer else {
            return Err(HostError::new("a stored read-at answer is not a blob id"));
        };
        let Some(blob) = blob else {
            return Ok(None);
        };
        if let Some(content) = read {
            self.blobs
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(blob, content.clone());
            return Ok(Some(content));
        }
        // A stored answer: the content an earlier run stored, else git's by its id. When neither
        // has it (`gate cache prune` removed the stored content, and a partial clone lacks the
        // file), the question is asked of git again, as if no answer were stored.
        if let Some(content) = Cache::from_env()
            .ok()
            .and_then(|cache| crate::cache::answers::blob(&cache, &blob))
        {
            return Ok(Some(content));
        }
        let content = match self.objects(|objects| objects.blob(&blob)) {
            Ok(Some(content)) => content,
            _ => match self
                .objects(|objects| objects.read_at(commit, path))
                .map_err(|error| self.unread(commit, path, error))?
            {
                Some((_, content)) => content,
                None => return Err(unfetched(commit, path, "git reads it as missing")),
            },
        };
        self.blobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(blob, content.clone());
        Ok(Some(content))
    }

    fn is_ancestor(
        &self,
        ancestor: &str,
        descendant: &str,
    ) -> std::result::Result<bool, HostError> {
        // A shallow clone hides ancestors: its answers are not kept for another clone.
        let lasting = !self.facts()?.shallow;
        let answer = self.remembered("is-ancestor", &[ancestor, descendant], lasting, || {
            // A commit this clone lacks answers no, until a fetch brings it: not kept.
            Ok(
                match self.objects(|objects| objects.is_ancestor(ancestor, descendant))? {
                    Some(holds) => kept(Answer::Holds(holds)),
                    None => (Answer::Holds(false), false),
                },
            )
        })?;
        match answer {
            Answer::Holds(holds) => Ok(holds),
            _ => Err(HostError::new(
                "a stored is-ancestor answer is not a yes or no",
            )),
        }
    }

    fn sync_state(&self, commit: &str) -> std::result::Result<SyncState, HostError> {
        // One answer per commit and run, whichever call or rule asks first: a call that asks
        // while another computes the same commit waits for that answer.
        let slot = Arc::clone(
            self.states
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(commit.to_owned())
                .or_default(),
        );
        slot.get_or_init(|| {
            self.sync_state_at(commit).map_err(|error| {
                HostError::new(format!("git.sync-state at commit {commit}: {error}"))
            })
        })
        .clone()
    }
}

/// The kind of [`Cache::answer`] that holds `sync-state` answers.
const SYNC_STATE: &str = "sync-state";

/// The meaning of a stored `sync-state` answer, in its key: raised when the same commit and
/// configuration would answer differently, so answers stored before are not reused. 2: no drift
/// under the fallback. 3: only lasting load failures stored, validation per plugin, the lower
/// fuel default. 4: the fuel default back to 10^12, so a fuel failure stored under 10^11 is not
/// reused. 5: the generated checkout sets `persist-credentials: false`, so a workflow answered as
/// in sync before is drift now, under the same core version until it is bumped. 6: the generated
/// workflow has a `concurrency` group, so one written before is drift. 7: the generated workflow
/// meets the `ci/recommended` standard (named checkout and aggregate job, `permissions: {}`, a
/// timeout on every job, release comments), so one written before is drift. 8: a check's budget
/// is `--warn-after` and `--fail-after`, and a job no longer adds a failing one, so hooks and a
/// workflow written before are drift.
const SYNC_STATE_VERSION: u32 = 8;

/// The first byte of a stored answer that holds the paths.
const ANSWER: u8 = b'p';
/// The first byte of a commit's stored answer that depends on the run's configuration: the
/// paths are stored under the key that adds that configuration's hash.
const RUN_DEPENDENT: u8 = b'r';

/// The sync state at a commit, whether the run's configuration took part in it, and whether it
/// may be stored for good.
struct Computed {
    state: SyncState,
    uses_run: bool,
    /// False when a generator failed with an environment error (a passing failure) or the run
    /// outputs it used came from one: the answer holds for this run only.
    lasting: bool,
}

/// What git answers depend on beyond the commits they are about.
#[derive(Clone, Debug)]
struct Facts {
    /// `git --version`.
    version: String,
    /// Whether the clone is shallow.
    shallow: bool,
}

impl GitHistory {
    /// The history of a run on `snapshot` of `workspace`, with what the command line gave.
    fn new(
        snapshot: &Snapshot,
        workspace: &Workspace,
        run_git: RunGit,
        filter: &RuleFilter,
        run_loaded: Arc<Loaded>,
    ) -> Self {
        Self {
            git: snapshot.git().clone(),
            head: snapshot.commit().map(str::to_owned),
            range: run_git.range,
            message: run_git.message,
            hook: run_git.hook,
            overrides: Arc::clone(&run_loaded.overrides),
            run_loaded,
            filter: filter.clone(),
            builds: OnceLock::new(),
            run_workspace: workspace.clone(),
            run: OnceLock::new(),
            commit_plugins: Mutex::new(BTreeMap::new()),
            states: Mutex::new(BTreeMap::new()),
            counted: SyncStateCount::default(),
            objects: Mutex::new(None),
            merges: Mutex::new(Batch::Unstarted),
            writes: CacheWrites::default(),
            answers: OnceLock::new(),
            facts: OnceLock::new(),
            blobs: Mutex::new(BTreeMap::new()),
            snapshots: Arc::default(),
            generations: Arc::default(),
        }
    }

    fn facts(&self) -> std::result::Result<&Facts, HostError> {
        self.facts
            .get_or_init(|| {
                let version = self.git.output(&["--version"]).map_err(host)?;
                let shallow = self
                    .git
                    .try_text(&["rev-parse", "--is-shallow-repository"])
                    .map_err(host)?;
                Ok(Facts {
                    version: String::from_utf8_lossy(&version).trim().to_owned(),
                    shallow: shallow.as_deref() != Some("false"),
                })
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    /// The answer to a question about the full commit ids `of`: stored by an earlier run, or
    /// asked now and kept. A question about anything else (a ref, an abbreviated id), one that
    /// is not `lasting`, or one whose answer `ask` says not to keep, is asked and not kept.
    /// The error of a reader that failed on `path` at `commit`: when the commit's tree names the
    /// file, a partial clone could not fetch it, which [`unfetched`] says; else git's own `error`.
    fn unread(&self, commit: &str, path: &str, error: HostError) -> HostError {
        match self.objects(|objects| objects.names_a_file(commit, path)) {
            Ok(true) => unfetched(commit, path, error.message()),
            _ => error,
        }
    }

    fn remembered(
        &self,
        kind: &'static str,
        of: &[&str],
        lasting: bool,
        ask: impl FnOnce() -> std::result::Result<(Answer, bool), HostError>,
    ) -> std::result::Result<Answer, HostError> {
        let commits = match kind {
            "merge-tree" => &of[..2],
            "read-at" => &of[..1],
            _ => of,
        };
        if !lasting || !commits.iter().all(|id| crate::git::is_object_id(id)) {
            return ask().map(|(answer, _)| answer);
        }
        let question: Question = (kind, of.iter().map(|id| (*id).to_owned()).collect());
        let Some(answers) = self.answers.get_or_init(|| {
            Cache::from_env()
                .ok()
                .map(|cache| Mutex::new(Answers::load(&cache)))
        }) else {
            return ask().map(|(answer, _)| answer);
        };
        // Held while git answers: each question is asked once per run, whichever call asks first.
        let mut answers = answers.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((answer, first_use)) = answers.get(&question) {
            if first_use {
                self.counted.answered.fetch_add(1, Ordering::Relaxed);
            }
            return Ok(answer);
        }
        self.counted.asked.fetch_add(1, Ordering::Relaxed);
        let (answer, keep) = ask()?;
        if keep {
            answers.learn(question, answer.clone());
        }
        Ok(answer)
    }

    /// Stores the answers this run learned, and gives `ran` the run's counts and the cache
    /// write that failed first.
    fn finish(&self, ran: &mut Ran) {
        self.writes.note(self.store_answers());
        (ran.sync_states, ran.git_answers) = self.counted.read();
        ran.generated = self.generations.read();
        ran.cache_warning = self
            .writes
            .take()
            .or_else(|| self.generations.writes.take());
    }

    /// Stores the answers this run learned, after the blob contents they name.
    fn store_answers(&self) -> Result<()> {
        let blobs = std::mem::take(&mut *self.blobs.lock().unwrap_or_else(PoisonError::into_inner));
        if !blobs.is_empty() {
            let cache = Cache::from_env()?;
            for (blob, content) in &blobs {
                crate::cache::answers::store_blob(&cache, blob, content)?;
            }
        }
        if let Some(Some(answers)) = self.answers.get() {
            answers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .store(&Cache::from_env()?)?;
        }
        Ok(())
    }

    /// git's merge of `first` and `second`: through the run's `git merge-tree --stdin` when both
    /// are full ids, else, or when it fails, by a git process of its own, which says what is wrong.
    fn merge(&self, first: &str, second: &str) -> std::result::Result<Option<String>, HostError> {
        let ids = crate::git::is_object_id(first) && crate::git::is_object_id(second);
        let mut slot = self.merges.lock().unwrap_or_else(PoisonError::into_inner);
        if ids && matches!(*slot, Batch::Unstarted) {
            *slot = self.git.merges().map_or(Batch::Unusable, Batch::Running);
        }
        if let Batch::Running(merges) = &mut *slot {
            if let Ok(answer) = merges.merge(first, second) {
                return Ok(answer);
            }
            *slot = Batch::Unstarted;
        } else {
            return self.git.merge_tree(first, second).map_err(host);
        }
        let answer = self.git.merge_tree(first, second).map_err(host)?;
        // git answered this question by itself: the batch failed for no reason of the question.
        *slot = Batch::Unusable;
        Ok(answer)
    }

    /// Asks the run's object reader, starting it on first use; after a failure the next question
    /// starts a fresh reader, as the protocol may be out of step.
    fn objects<T>(
        &self,
        ask: impl FnOnce(&mut Objects) -> Result<T>,
    ) -> std::result::Result<T, HostError> {
        let mut slot = self.objects.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(Objects::new(&self.git).map_err(host)?);
        }
        let Some(objects) = slot.as_mut() else {
            return Err(HostError::new("git cat-file did not start"));
        };
        let answer = ask(objects);
        if answer.is_err() {
            *slot = None;
        }
        answer.map_err(host)
    }

    /// What `gate sync` produces at `commit` and which of it `gate sync --check` reports there.
    ///
    /// The generators run on the commit's tree with the commit's configuration and the plugins
    /// its `gate.lock` pins (or the run's `--plugin` builds). Only when that `gate.toml` is not a
    /// configuration of this core (it does not parse, or does not validate) do the run's own
    /// configuration and plugins run on the commit's tree instead, and then only `produced`
    /// answers: files another tool wrote cannot be judged by this core's generators, so `drift`
    /// is empty and a history rule judges by path alone. A generator that fails, or
    /// whose output `gate sync` would refuse, never fails the call: it answers the paths it could
    /// not produce. A commit without a `gate.toml` generates nothing.
    ///
    /// Answers are stored in the cache by commit, core version and `--plugin` builds; an answer
    /// the run's configuration took part in is stored under the hash of that configuration and of
    /// the run's generator outputs as well, and the commit's entry points there. An answer a
    /// generator's environment error took part in is not stored.
    ///
    /// # Errors
    /// Fails when git cannot read the commit, a plugin the commit pins cannot be fetched or does
    /// not match its pin, or the run's own configuration or plugins fail.
    fn sync_state_at(&self, commit: &str) -> Result<SyncState> {
        let cache = Cache::from_env()?;
        let own_key = self.state_key(commit, None);
        match stored(&cache, &own_key)? {
            Some((ANSWER, state)) => {
                self.counted.stored.fetch_add(1, Ordering::Relaxed);
                return Ok(state);
            }
            Some(_) => {
                let run_key = self.state_key(commit, Some(self.run_key()?));
                if let Some((_, state)) = stored(&cache, &run_key)? {
                    self.counted.stored.fetch_add(1, Ordering::Relaxed);
                    return Ok(state);
                }
            }
            None => {}
        }

        self.counted.computed.fetch_add(1, Ordering::Relaxed);
        let computed = self.compute(commit)?;
        if !computed.lasting {
            return Ok(computed.state);
        }
        if computed.uses_run {
            let run_key = self.state_key(commit, Some(self.run_key()?));
            self.writes
                .note(store(&cache, &run_key, ANSWER, &computed.state));
            self.writes.note(store(
                &cache,
                &own_key,
                RUN_DEPENDENT,
                &SyncState::default(),
            ));
        } else {
            self.writes
                .note(store(&cache, &own_key, ANSWER, &computed.state));
        }
        Ok(computed.state)
    }

    /// [`GitHistory::sync_state_at`] without the cache.
    fn compute(&self, commit: &str) -> Result<Computed> {
        let snapshot = Snapshot::open_shared(
            &self.git,
            Source::Commit(commit.to_owned()),
            &self.snapshots,
        )?;
        if snapshot.read(crate::config::FILE_NAME)?.is_none() {
            return Ok(Computed {
                state: SyncState::default(),
                uses_run: false,
                lasting: true,
            });
        }
        match Workspace::from_snapshot(snapshot) {
            Ok(workspace) => {
                if let Some(computed) = self.at_own_configuration(&workspace)? {
                    return Ok(computed);
                }
            }
            Err(error) if matches!(error.kind(), crate::error::ErrorKind::Config) => {}
            Err(error) => return Err(error),
        }

        let context = self.run_context()?;
        let workspace = self.run_workspace.with_snapshot(Snapshot::open_shared(
            &self.git,
            Source::Commit(commit.to_owned()),
            &self.snapshots,
        )?);
        let generators = PluginGenerators::with_loaded(
            &workspace,
            context.loaded.clone(),
            Arc::clone(&self.generations),
        );
        let mut computed = self.state_in(&workspace, &generators, false)?;
        computed.uses_run = true;
        // `produced` already holds every drifting path, a failing generator's run outputs too.
        computed.state.drift.clear();
        Ok(computed)
    }

    /// The answer with the commit's own configuration; `None` when that configuration does not
    /// validate against its `gate.lock`, its plugins or the core, so the run's configuration
    /// answers.
    fn at_own_configuration(&self, workspace: &Workspace) -> Result<Option<Computed>> {
        let lock =
            match crate::lock::verify_with(workspace, &self.overrides.keys().cloned().collect()) {
                Ok(lock) => lock,
                Err(error) if matches!(error.kind(), crate::error::ErrorKind::Config) => {
                    return Ok(None);
                }
                Err(error) => return Err(error),
            };
        let plugins = self.commit_plugins(workspace, lock.as_ref())?;
        // The rules of a plugin that does not load cannot be checked; the configuration is still
        // checked against every plugin that loads. The generators of those answer, and the
        // others' answer their paths.
        if let Some(loaded) = &plugins.loaded
            && loaded
                .validate_except(workspace, &RuleFilter::new(Vec::new()), &plugins.unloadable)
                .is_err()
        {
            return Ok(None);
        }
        let generators =
            PluginGenerators::with_loaded(workspace, plugins.loaded, Arc::clone(&self.generations));
        match self.state_in(workspace, &generators, !plugins.unloadable.is_empty()) {
            Ok(computed) => Ok(Some(computed)),
            Err(error) if matches!(error.kind(), crate::error::ErrorKind::Config) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// The plugins a commit's configuration declares, loaded once per run for each set of pins.
    ///
    /// # Errors
    /// Fails when a pinned component cannot be fetched or does not match its pin, or fails to load
    /// in a way that may pass by the next run (a trap or the deadline in `describe`, the machine):
    /// an environment error, not an answer. A component that does not load under this core for
    /// good does not fail: its generators cannot produce.
    fn commit_plugins(&self, workspace: &Workspace, lock: Option<&Lock>) -> Result<CommitPlugins> {
        let mut hasher = Sha256::new();
        for (source, declarations) in &Wanted::of(workspace).plugins {
            hasher.update(format!("{source}\0"));
            for declaration in declarations {
                hasher.update(format!("{}\0", declaration.name));
            }
            if let Some(pin) = lock.and_then(|lock| lock.plugins.get(source)) {
                hasher.update(format!("{}\0", pin.sha256));
            }
        }
        let key = Sha256Sum::from_digest(&hasher.finalize());
        if let Some(plugins) = self
            .commit_plugins
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return Ok(plugins.clone());
        }
        let fetched = declared(workspace, lock, &self.overrides)
            .and_then(|declared| {
                declared
                    .map(|declared| fetch_components(declared, lock, &self.overrides))
                    .transpose()
            })
            .map_err(|error| {
                Error::environment(format!(
                    "a plugin this commit's gate.lock pins cannot be fetched or does not match its pin: {error}; run again with network access, or pass --plugin <name>=<path.wasm> for it"
                ))
            })?;
        let plugins = match fetched {
            None => CommitPlugins {
                loaded: None,
                unloadable: BTreeSet::new(),
            },
            Some(fetched) => {
                let (loaded, failures) = instantiate(fetched, &self.overrides, false)?;
                if let Some((name, failure)) = failures.iter().find(|(_, failure)| !failure.lasting)
                {
                    return Err(Error::environment(format!(
                        "plugin {name}, which this commit's gate.lock pins, did not load in a way that may pass ({}); run again",
                        failure.error
                    )));
                }
                CommitPlugins {
                    loaded: Some(Arc::new(loaded)),
                    unloadable: failures.into_keys().collect(),
                }
            }
        };
        self.commit_plugins
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, plugins.clone());
        Ok(plugins)
    }

    /// The run's configuration hash and plugins, computed once. The plugins are the run's own with
    /// the generator plugins, and the configuration is checked as the run checked it, with the
    /// run's filter: a rule's needs follow `--rule` on both paths.
    fn run_context(&self) -> Result<&RunContext> {
        self.run
            .get_or_init(|| {
                let workspace = &self.run_workspace;
                let loaded = self.run_loaded.with_generators(workspace)?;
                loaded.validate(workspace, &self.filter)?;
                Ok(RunContext {
                    hash: configuration_hash(workspace)?,
                    loaded: Some(loaded),
                    outputs: OnceLock::new(),
                    key: OnceLock::new(),
                })
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    /// The paths each generator writes under the run's configuration, on the run's tree.
    fn run_outputs(&self) -> Result<&RunOutputs> {
        let context = self.run_context()?;
        Ok(context.outputs.get_or_init(|| {
            let workspace = &self.run_workspace;
            let generators = PluginGenerators::with_loaded(
                workspace,
                context.loaded.clone(),
                Arc::clone(&self.generations),
            );
            let mut by_entry: BTreeMap<EntryKey, BTreeSet<String>> = BTreeMap::new();
            for entry in sync::generator_entries(workspace) {
                if let Ok(outputs) = sync::generator_outputs(&entry, &generators) {
                    by_entry.entry(entry_key(&entry)).or_default().extend(
                        outputs
                            .into_iter()
                            .filter(|output| !output.seed)
                            .map(|output| output.path),
                    );
                }
            }
            RunOutputs {
                by_entry,
                transient: generators.had_passing_failure(),
            }
        }))
    }

    /// The hash a run-dependent answer is stored under: the run configuration's, and the paths
    /// each of its generators writes on the run's tree (they read the run's files, which the
    /// configuration's hash does not cover).
    fn run_key(&self) -> Result<&Sha256Sum> {
        let context = self.run_context()?;
        if let Some(key) = context.key.get() {
            return Ok(key);
        }
        let outputs = self.run_outputs()?;
        let mut hasher = Sha256::new();
        hasher.update(format!("{}\0", context.hash.as_str()));
        for ((project, generator), paths) in &outputs.by_entry {
            hasher.update(format!(
                "{}\0{generator}\0{}\0",
                project.as_deref().unwrap_or("/"),
                paths.len()
            ));
            for path in paths {
                hasher.update(format!("{path}\0"));
            }
        }
        Ok(context
            .key
            .get_or_init(|| Sha256Sum::from_digest(&hasher.finalize())))
    }

    /// What `gate sync` produces in `workspace` and which of it `gate sync --check` reports, both
    /// sorted. `produced` holds every output (generators', hooks', CI's) but seeds, and every path `gate sync`
    /// would remove (a tracked file under an owned glob that no generator writes, a hook or
    /// workflow gate wrote that the configuration no longer names); `drift` holds the outputs that
    /// are missing, differ or are not executable, and the paths it would remove. A `[generate]`
    /// entry whose generator fails, or whose output `gate sync` would refuse (outside the
    /// repository, a protected path, a path another output writes), writes nothing here: both
    /// lists hold the tracked files under its globs and the paths that entry writes under the
    /// run's configuration, and the answer uses the run's configuration.
    ///
    /// # Errors
    /// Fails when the core's own outputs (hooks, workflow) are wrong for the configuration, as a
    /// configuration error, or git cannot read the commit.
    /// With `unloadable` (a plugin of the configuration does not load under this core), the CI
    /// workflow, whose jobs depend on which rules read history, is answered as drifting too. When a
    /// generator call fails in a way that may pass, the answer holds for this run only.
    fn state_in(
        &self,
        workspace: &Workspace,
        generators: &PluginGenerators<'_>,
        unloadable: bool,
    ) -> Result<Computed> {
        let mut outputs = sync::core_outputs(workspace, generators)?;
        let entries = sync::generator_entries(workspace);
        let mut failed: BTreeSet<EntryKey> = BTreeSet::new();
        let mut written: Vec<(EntryKey, Vec<sync::Output>)> = Vec::new();
        // A generator's environment error may pass by the next run: the answer is not stored.
        for entry in &entries {
            match sync::generator_outputs(entry, generators) {
                Ok(files) => written.push((entry_key(entry), files)),
                Err(_) => {
                    failed.insert(entry_key(entry));
                }
            }
        }
        let mut writers: BTreeMap<&str, Vec<&EntryKey>> = BTreeMap::new();
        for (key, files) in &written {
            for file in files {
                writers.entry(file.path.as_str()).or_default().push(key);
            }
        }
        let core: BTreeSet<&str> = outputs.iter().map(|output| output.path.as_str()).collect();
        let shared: Vec<EntryKey> = writers
            .iter()
            .filter(|(path, keys)| keys.len() > 1 || core.contains(*path))
            .flat_map(|(_, keys)| keys.iter().map(|key| (*key).clone()))
            .collect();
        failed.extend(shared);
        for (key, files) in written {
            if !failed.contains(&key) {
                outputs.extend(files);
            }
        }
        let outputs = sync::sorted(outputs)?;

        let mut owned = Vec::new();
        for entry in &entries {
            match generators.owns(entry.project, entry.generator) {
                Ok(globs) => owned.extend(globs.into_iter().map(|glob| sync::Owned {
                    item: entry.generator.to_string(),
                    glob,
                })),
                Err(_) => {
                    failed.insert(entry_key(entry));
                }
            }
        }
        let mut transient = generators.had_passing_failure();

        let mut drift: BTreeSet<String> = sync::check(workspace, &outputs, &owned)?
            .iter()
            .filter_map(|finding| finding.location.as_ref()?.path().map(str::to_owned))
            .collect();
        let uses_run = !failed.is_empty();
        if uses_run {
            let run_outputs = self.run_outputs()?;
            transient |= run_outputs.transient;
            for key in &failed {
                drift.extend(run_outputs.by_entry.get(key).into_iter().flatten().cloned());
            }
        }
        if unloadable {
            drift.extend(
                outputs
                    .iter()
                    .filter(|output| output.item == "ci")
                    .map(|output| output.path.clone()),
            );
        }
        // What check reports beyond the outputs is what sync removes, which it owns too. A seed is
        // the project's file once written, so it is not produced.
        let produced: BTreeSet<String> = outputs
            .into_iter()
            .filter(|output| !output.seed)
            .map(|output| output.path)
            .chain(drift.iter().cloned())
            .collect();
        Ok(Computed {
            state: SyncState {
                produced: produced.into_iter().collect(),
                drift: drift.into_iter().collect(),
            },
            uses_run,
            lasting: !transient,
        })
    }

    /// The cache key of a `sync-state` answer: the commit, the core's version, the `--plugin`
    /// builds, and for an answer the run's configuration took part in, [`GitHistory::run_key`].
    fn state_key(&self, commit: &str, configuration: Option<&Sha256Sum>) -> Sha256Sum {
        let mut hasher = Sha256::new();
        hasher.update(format!(
            "{SYNC_STATE}\0{SYNC_STATE_VERSION}\0{}\0{commit}\0",
            env!("CARGO_PKG_VERSION")
        ));
        hasher.update(self.builds.get_or_init(|| {
            self.overrides
                .iter()
                .fold(String::new(), |mut builds, (name, (_, bytes))| {
                    let sha256 = Sha256Sum::from_digest(&Sha256::digest(bytes));
                    let _ = write!(builds, "{name}\0{}\0", sha256.as_str());
                    builds
                })
        }));
        if let Some(configuration) = configuration {
            hasher.update(format!("configuration\0{}\0", configuration.as_str()));
        }
        Sha256Sum::from_digest(&hasher.finalize())
    }
}

/// Whether a call's failure, on a component that has loaded, may pass by the next call: the plugin
/// reports an environment failure, traps, runs past the deadline (wall-clock time, which load on
/// the machine stretches), the runtime cannot start, or the component does not instantiate (`Load`:
/// a component that loaded once cannot fail to instantiate for good, so that is the machine). An
/// interface break, or a fuel or memory limit, recurs for the same component and input.
fn passing_call_failure(kind: gate_runtime::ErrorKind) -> bool {
    matches!(
        kind,
        gate_runtime::ErrorKind::Setup
            | gate_runtime::ErrorKind::Load
            | gate_runtime::ErrorKind::Trap
            | gate_runtime::ErrorKind::Deadline
            | gate_runtime::ErrorKind::Reported
    )
}

/// The key of a `[generate]` entry.
fn entry_key(entry: &sync::GeneratorEntry<'_>) -> EntryKey {
    (
        entry.project.map(str::to_owned),
        entry.generator.to_string(),
    )
}

/// The hash of the files a workspace's configuration was read from, by path, and `gate.lock`.
fn configuration_hash(workspace: &Workspace) -> Result<Sha256Sum> {
    let inputs = workspace.inputs();
    let lock = crate::lock::LOCK_FILE.to_owned();
    let paths: BTreeSet<&String> = inputs
        .everywhere
        .iter()
        .chain(inputs.projects.values().flatten())
        .chain(std::iter::once(&lock))
        .collect();
    let mut hasher = Sha256::new();
    for path in paths {
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
        match workspace.snapshot().read(path)? {
            Some(bytes) => {
                hasher.update(Sha256::digest(&bytes[..]));
            }
            None => hasher.update(b"absent"),
        }
    }
    Ok(Sha256Sum::from_digest(&hasher.finalize()))
}

/// A stored `sync-state` answer: its first byte and the state. After the first byte come the
/// produced paths, each ended by NUL, then one NUL, then the drifting paths, each ended by NUL; a
/// path is never empty.
fn stored(cache: &Cache, key: &Sha256Sum) -> Result<Option<(u8, SyncState)>> {
    let file = cache.answer(SYNC_STATE, key);
    match std::fs::read(&file) {
        Ok(bytes) => {
            let Some((&tag, rest)) = bytes.split_first() else {
                return Ok(None);
            };
            let mut state = SyncState::default();
            let mut in_drift = false;
            for path in String::from_utf8_lossy(rest).split_terminator('\0') {
                if path.is_empty() {
                    in_drift = true;
                } else if in_drift {
                    state.drift.push(path.to_owned());
                } else {
                    state.produced.push(path.to_owned());
                }
            }
            crate::cache::touch(&file);
            Ok(Some((tag, state)))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::environment(format!(
            "cannot read {}: {error}; remove it, and gate computes it again",
            file.display()
        ))),
    }
}

/// Stores a `sync-state` answer: its first byte and the state, as [`stored`] reads it.
fn store(cache: &Cache, key: &Sha256Sum, tag: u8, state: &SyncState) -> Result<()> {
    let mut temporary = cache.temporary()?;
    let mut bytes = vec![tag];
    for path in &state.produced {
        bytes.extend_from_slice(path.as_bytes());
        bytes.push(0);
    }
    bytes.push(0);
    for path in &state.drift {
        bytes.extend_from_slice(path.as_bytes());
        bytes.push(0);
    }
    temporary.file().write_all(&bytes).map_err(|error| {
        Error::environment(format!(
            "cannot write {}: {error}",
            temporary.path().display()
        ))
    })?;
    cache.insert_answer(temporary, SYNC_STATE, key)?;
    Ok(())
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "used as map_err(host), which passes the error by value"
)]
fn host(error: Error) -> HostError {
    HostError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository_on(branch: &str) -> (tempfile::TempDir, Git) {
        let directory = tempfile::tempdir().expect("directory");
        let git = |arguments: &[&str]| {
            let status = std::process::Command::new("git")
                .args(arguments)
                .current_dir(directory.path())
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
                .status()
                .expect("git");
            assert!(status.success(), "git {arguments:?}");
        };
        git(&["init", "--quiet", "--initial-branch", branch]);
        git(&["commit", "--quiet", "--allow-empty", "--message", "first"]);
        // A tag with the branch's name: `symbolic-ref --short` would answer heads/<branch>.
        git(&["tag", branch]);
        let found = Git::discover(directory.path()).expect("discover");
        (directory, found)
    }

    #[test]
    fn the_branch_is_the_ci_variable_else_the_branch_head_is_on_even_beside_a_tag_of_its_name() {
        let (_directory, git) = repository_on("feature/login");
        assert_eq!(
            current_branch(None, &git).expect("branch").as_deref(),
            Some("feature/login")
        );
        assert_eq!(
            current_branch(Some(" ".to_owned()), &git)
                .expect("branch")
                .as_deref(),
            Some("feature/login"),
            "an empty variable names no branch"
        );
        assert_eq!(
            current_branch(Some("fix/from-a-pull-request".to_owned()), &git)
                .expect("branch")
                .as_deref(),
            Some("fix/from-a-pull-request"),
            "CI's variable names the branch of a detached pull request checkout"
        );
    }

    #[test]
    fn only_a_failure_that_recurs_marks_a_component_unloadable_for_good() {
        use gate_runtime::ErrorKind as Kind;
        // The core's limits on a describe without input: the same on every run.
        for kind in [Kind::Load, Kind::Plugin, Kind::OutOfFuel, Kind::OutOfMemory] {
            assert!(lasting_load_failure(kind), "{kind:?}");
        }
        for kind in [
            Kind::Setup,
            Kind::Request,
            Kind::Trap,
            Kind::Deadline,
            Kind::Reported,
        ] {
            assert!(
                !lasting_load_failure(kind),
                "{kind:?} while loading answers for this run only"
            );
        }
        // A component that loaded once and then does not instantiate meets the machine.
        for kind in [
            Kind::Setup,
            Kind::Load,
            Kind::Trap,
            Kind::Deadline,
            Kind::Reported,
        ] {
            assert!(passing_call_failure(kind), "{kind:?}");
        }
        for kind in [
            Kind::Request,
            Kind::OutOfFuel,
            Kind::OutOfMemory,
            Kind::Plugin,
            Kind::Configuration,
        ] {
            assert!(!passing_call_failure(kind), "{kind:?}");
        }
    }

    #[test]
    fn an_options_schema_that_is_not_a_json_schema_breaks_the_plugin_for_good() {
        let name = PluginName::try_from("hello".to_owned()).expect("a plugin name");
        for (text, problem) in [
            ("", "is not JSON"),
            ("not json", "is not JSON"),
            (r#"{"type": 5}"#, "is not a valid JSON Schema"),
        ] {
            let failure = schema(&name, "the plugin", text).expect_err(text);
            assert!(failure.lasting, "{text}");
            let message = failure.error.to_string();
            assert!(
                message.contains("plugin hello: the plugin declares an options schema that")
                    && message.contains(problem)
                    && message.contains("the plugin is broken"),
                "{text}: {message}"
            );
        }
        let valid = r#"{"type": "object", "additionalProperties": false}"#;
        assert_eq!(
            schema(&name, "rule marker", valid).expect("a JSON Schema"),
            serde_json::json!({ "type": "object", "additionalProperties": false })
        );
    }

    #[test]
    fn schema_violations_name_the_key() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": { "max": { "type": "integer" } },
            "additionalProperties": false
        });
        let validator = JsonSchemaValidator;
        assert_eq!(
            validator.validate(&schema, &serde_json::json!({ "max": 3 })),
            Vec::<String>::new()
        );
        let violations = validator.validate(&schema, &serde_json::json!({ "max": "three" }));
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].starts_with("/max: "), "{violations:?}");
        let violations = validator.validate(&schema, &serde_json::json!({ "min": 1 }));
        assert!(violations[0].contains("min"), "{violations:?}");
    }

    #[test]
    fn a_sync_state_a_run_reads_counts_as_used() {
        let directory = tempfile::tempdir().expect("cache");
        let cache = Cache::at(directory.path());
        let key = Sha256Sum::from_digest(&[7; 32]);
        let mut temporary = cache.temporary().expect("temporary");
        temporary
            .file()
            .write_all(b"\x01produced\0")
            .expect("write");
        let file = cache
            .insert_answer(temporary, SYNC_STATE, &key)
            .expect("insert");
        crate::cache::used_days_ago(&file, 40);
        assert_eq!(
            crate::cache::older_than_30(&cache),
            std::slice::from_ref(&file)
        );

        let (tag, state) = stored(&cache, &key).expect("read").expect("stored");
        assert_eq!((tag, state.produced), (1, vec!["produced".to_owned()]));
        assert_eq!(
            crate::cache::older_than_30(&cache),
            Vec::<PathBuf>::new(),
            "the run read the state"
        );
    }
}

//! The engine, loaded plugins, and calls.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use gate_sdk::{
    CheckRequest, FileLocation, Finding, GenerateOutput, GeneratedFile, GeneratorInfo, Judged,
    Location, PluginInfo, PresetInfo, RuleInfo, Scope, Span,
};
use sha2::{Digest, Sha256};
use wasmtime::component::Component;
use wasmtime::{Cache, CacheConfig, Engine, Store, Trap, UpdateDeadline};

use crate::Result;
use crate::error::{Error, ErrorKind};
use crate::grant::{
    CORE_FILES, Grant, HOOKS, has_git_segment, is_core_path, is_relative_glob, owns_everything,
};
use crate::host::{History, Tree};
use crate::read_set::ReadSet;
use crate::state::{Access, Plugin as Bindings, PluginPre, State, is_full_id, linker, wit};

/// The files the core owns, as errors name them.
const CORE_OWNS: &str =
    "git's directory, gate.toml, gate.lock, gate.exceptions.toml, bin/gate, .githooks";

/// The stack of a worker thread: room for the wasm stack and the host frames under it.
const WORKER_STACK: usize = 16 << 20;

/// How often the engine's epoch advances: the resolution of [`Config::deadline`].
const TICK: Duration = Duration::from_millis(10);

/// The plugin interface this core loads, the package of `wit/gate.wit`. A component built against
/// another version fails to load.
pub const INTERFACE: &str = "gate:plugin@0.3.0";

/// How the runtime runs plugins. The limits are the core's, the same for every plugin: a plugin
/// cannot set its own. A call that hits one fails with [`ErrorKind::OutOfFuel`],
/// [`ErrorKind::Deadline`] or [`ErrorKind::OutOfMemory`], an environment failure that names the
/// plugin and the limit, never a finding.
#[derive(Clone, Debug)]
pub struct Config {
    /// The directory wasmtime keeps compiled modules in: the core passes its cache's `modules/`
    /// directory. `None` compiles every component on load; the runtime never uses wasmtime's
    /// default cache location.
    pub module_cache: Option<PathBuf>,
    /// The fuel of one call, about one unit per WebAssembly operator; 10^12 by default.
    pub fuel: u64,
    /// The wall-clock time one call may spend in the plugin, its instantiation included, to within
    /// 10 ms; time in host calls (`git`, `sync-state`, reads) does not count. 600 s by default: a
    /// backstop for a machine that stalls. On the machine the defaults were measured on,
    /// [`Config::fuel`] runs out first (10^12 units is about 355 s there); on one more than 1.7 times
    /// slower, the deadline can come first.
    pub deadline: Duration,
    /// The most linear memory one call may grow to; 4 GiB, all a 32-bit memory can address, by
    /// default.
    pub memory_bytes: usize,
    /// The most native stack WebAssembly may use in one call.
    pub wasm_stack_bytes: usize,
    /// The worker threads of [`Runtime::check_all`].
    pub workers: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            module_cache: None,
            fuel: 1_000_000_000_000,
            deadline: Duration::from_mins(10),
            memory_bytes: 4 << 30,
            wasm_stack_bytes: 8 << 20,
            workers: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
        }
    }
}

/// One engine and linker, shared by every plugin and call.
pub struct Runtime {
    engine: Engine,
    linker: wasmtime::component::Linker<State>,
    cache: Option<Cache>,
    config: Config,
    /// Cleared when the runtime drops: the thread that advances the engine's epoch then stops.
    ticking: Arc<AtomicBool>,
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.ticking.store(false, Ordering::Relaxed);
    }
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Runtime")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// A loaded component and what it declares.
pub struct Plugin {
    name: String,
    sha256: String,
    pre: PluginPre<State>,
    info: PluginInfo,
}

impl std::fmt::Debug for Plugin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Plugin")
            .field("name", &self.name)
            .field("sha256", &self.sha256)
            .finish_non_exhaustive()
    }
}

impl Plugin {
    /// The name the configuration gives the plugin.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The SHA-256 of the component, lowercase hex: part of every result-cache key.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// What the plugin's `describe` answered.
    #[must_use]
    pub fn info(&self) -> &PluginInfo {
        &self.info
    }

    /// The globs generator `generator` owns, as repository globs under `grant`'s root: what
    /// `gate sync` passes on to find stale output. `None` when the plugin has no such generator.
    ///
    /// # Errors
    /// A glob covers a file the core owns: `gate sync` would delete it.
    pub fn owned(&self, generator: &str, grant: &Grant) -> Result<Option<Vec<String>>> {
        let Some(info) = self
            .info
            .generators
            .iter()
            .find(|info| info.name == generator)
        else {
            return Ok(None);
        };
        let root = grant.root();
        let mut core: Vec<String> = CORE_FILES.iter().map(|file| (*file).to_owned()).collect();
        core.extend([format!("{HOOKS}/pre-commit"), ".git/config".to_owned()]);
        if !root.is_empty() {
            core.extend(
                [
                    "gate.toml",
                    "gate.lock",
                    "gate.exceptions.toml",
                    ".git/config",
                ]
                .map(|file| format!("{root}/{file}")),
            );
        }
        info.owns
            .iter()
            .map(|glob| {
                let glob = grant.to_repository_glob(glob);
                let matcher = globset::GlobBuilder::new(&glob)
                    .case_insensitive(true)
                    .literal_separator(true)
                    .build()
                    .map_err(|error| {
                        Error::new(
                            &self.name,
                            ErrorKind::Load,
                            format!(
                                "generator {generator} owns the glob {glob:?}, which is not a glob: {error}; report it to the plugin's authors"
                            ),
                        )
                    })?
                    .compile_matcher();
                if let Some(file) = core.iter().find(|file| matcher.is_match(file)) {
                    return Err(Error::new(
                        &self.name,
                        ErrorKind::Load,
                        format!(
                            "generator {generator} owns the glob {glob:?}, which covers {file:?}, a file the core owns ({CORE_OWNS}); report it to the plugin's authors"
                        ),
                    ));
                }
                Ok(glob)
            })
            .collect::<Result<Vec<_>>>()
            .map(Some)
    }
}

/// One `check` call: a plugin, its enabled rules for one project, and what the call may read.
pub struct Call<'p> {
    pub plugin: &'p Plugin,
    pub request: CheckRequest,
    pub tree: Arc<dyn Tree>,
    /// Git, for the rules that declare `reads-history`; `None` when the run has no history.
    pub history: Option<Arc<dyn History>>,
    pub grant: Grant,
}

impl std::fmt::Debug for Call<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Call")
            .field("plugin", &self.plugin.name)
            .field("request", &self.request)
            .field("grant", &self.grant)
            .finish_non_exhaustive()
    }
}

/// What a call answered, what it read, and what it logged.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CallOutcome {
    pub findings: Vec<Finding>,
    /// The rules of the call that judged less than their whole scope, with the targets they
    /// judged; a rule not listed judged its whole scope.
    pub judged: Vec<Judged>,
    pub read_set: ReadSet,
    /// `log` lines, then stdout and stderr lines prefixed `stdout: ` and `stderr: `.
    pub log: Vec<String>,
    /// Where the call's time went.
    pub time: CallTime,
    /// The fuel the call used: for each invocation, the fuel it was given minus the fuel left.
    pub fuel_used: u64,
    /// The call's invocations: its snapshot rules run in one, its history rules in another.
    pub invocations: Vec<Invocation>,
}

/// One invocation of a call: the rules it ran, its time and the fuel it used. The rules of one
/// invocation share an instance, so their time and fuel are not told apart.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Invocation {
    /// The rules it ran.
    pub rules: Vec<String>,
    /// From instantiation to the answer, host calls included.
    pub elapsed: Duration,
    /// The fuel it was given minus the fuel left.
    pub fuel_used: u64,
}

/// Where a call's time went: in all, and in the host by kind of host call.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CallTime {
    /// From instantiation to the answer, host calls included.
    pub elapsed: Duration,
    /// The time spent answering the plugin, by kind of host call (`git.sync-state`, `tree.read`).
    pub host: BTreeMap<String, HostTime>,
}

/// The time spent in one kind of host call, and how many calls.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HostTime {
    pub time: Duration,
    pub calls: u64,
}

impl CallTime {
    /// The time of one invocation: `elapsed` in all, `host` by kind.
    fn new(elapsed: Duration, host: BTreeMap<&'static str, HostTime>) -> Self {
        Self {
            elapsed,
            host: host
                .into_iter()
                .map(|(kind, spent)| (kind.to_owned(), spent))
                .collect(),
        }
    }

    /// Adds the time of another invocation of the same call.
    fn merge(&mut self, other: Self) {
        self.elapsed += other.elapsed;
        for (kind, spent) in other.host {
            let entry = self.host.entry(kind).or_default();
            entry.time += spent.time;
            entry.calls += spent.calls;
        }
    }
}

/// The tree of a call that reads nothing: `describe`.
struct Empty;

impl Tree for Empty {
    fn paths(&self) -> &[String] {
        &[]
    }

    fn read(&self, _: &str) -> std::result::Result<Option<crate::Blob>, crate::HostError> {
        Ok(None)
    }

    fn changed(&self) -> Option<&[String]> {
        None
    }
}

impl Runtime {
    /// A runtime with `config`.
    ///
    /// # Errors
    /// The engine or the cache directory cannot be set up.
    pub fn new(config: Config) -> Result<Self> {
        let setup = |error: wasmtime::Error| Error::new("", ErrorKind::Setup, format!("{error:#}"));
        let cache = match &config.module_cache {
            Some(directory) => {
                let unusable = |error: &dyn std::fmt::Display| {
                    Error::new(
                        "",
                        ErrorKind::Setup,
                        format!(
                            "cannot use the module cache at {}: {error}; check that the directory is writable",
                            directory.display()
                        ),
                    )
                };
                std::fs::create_dir_all(directory).map_err(|error| unusable(&error))?;
                let mut cache = CacheConfig::new();
                cache.with_directory(directory);
                Some(Cache::new(cache).map_err(|error| {
                    Error::new(
                        "",
                        ErrorKind::Setup,
                        format!(
                            "cannot use the module cache at {}: {error:#}; check that the directory is writable",
                            directory.display()
                        ),
                    )
                })?)
            }
            None => None,
        };
        let mut engine = wasmtime::Config::new();
        engine
            .wasm_component_model(true)
            .consume_fuel(true)
            .epoch_interruption(true)
            .max_wasm_stack(config.wasm_stack_bytes)
            // wasmtime-wasi turns on wasmtime's async support, and the engine then refuses a wasm
            // stack larger than the async stack (2 MiB by default). Calls here are synchronous,
            // so the async stack only has to be at least as large.
            .async_stack_size(config.wasm_stack_bytes)
            .cache(cache.clone());
        let engine = Engine::new(&engine).map_err(setup)?;
        let linker = linker(&engine).map_err(setup)?;
        let ticking = Arc::new(AtomicBool::new(true));
        let ticker = (engine.clone(), Arc::clone(&ticking));
        std::thread::Builder::new()
            .name("gate-epoch".to_owned())
            .spawn(move || {
                let (engine, ticking) = ticker;
                while ticking.load(Ordering::Relaxed) {
                    std::thread::sleep(TICK);
                    engine.increment_epoch();
                }
            })
            .map_err(|error| {
                Error::new(
                    "",
                    ErrorKind::Setup,
                    format!("cannot start the thread that times calls: {error}"),
                )
            })?;
        Ok(Self {
            engine,
            linker,
            cache,
            config,
            ticking,
        })
    }

    /// The compiled-module cache's hits and misses (compilations) so far; `(0, 0)` without a cache.
    #[must_use]
    pub fn cache_counts(&self) -> (usize, usize) {
        self.cache
            .as_ref()
            .map_or((0, 0), |cache| (cache.cache_hits(), cache.cache_misses()))
    }

    /// Loads the component `wasm` as the plugin `name` and asks for its declaration.
    ///
    /// # Errors
    /// The bytes are not a component, or one built for another version of the `gate:plugin`
    /// interface, or `describe` fails or declares a rule or generator twice.
    pub fn load(&self, name: &str, wasm: &[u8]) -> Result<Plugin> {
        let load = |error: wasmtime::Error| {
            Error::new(
                name,
                ErrorKind::Load,
                format!("cannot load the component: {error:#}"),
            )
        };
        // A component whose imports or exports do not match this core's `gate:plugin` world, such
        // as a record with another set of fields, was built against another version of the
        // interface.
        let interface = |error: wasmtime::Error| {
            Error::new(
                name,
                ErrorKind::Load,
                format!(
                    "{name} was built for another plugin interface; this gate {} loads plugins built for {INTERFACE} only: pin the plugin's release built for {INTERFACE} (each plugin's CHANGELOG.md says which interface a release is built for), or rebuild it with gate-sdk {} ({error:#})",
                    env!("CARGO_PKG_VERSION"),
                    env!("CARGO_PKG_VERSION")
                ),
            )
        };
        let component = Component::new(&self.engine, wasm).map_err(load)?;
        let pre = PluginPre::new(self.linker.instantiate_pre(&component).map_err(interface)?)
            .map_err(interface)?;
        let sha256 = Sha256::digest(wasm)
            .iter()
            .fold(String::new(), |mut text, byte| {
                let _ = write!(text, "{byte:02x}");
                text
            });
        let mut plugin = Plugin {
            name: name.to_owned(),
            sha256,
            pre,
            info: empty_info(),
        };
        let access = Access {
            tree: Arc::new(Empty),
            history: None,
            reads_history: false,
            reads_message: false,
            grant: Grant::repository(),
        };
        let (info, _, _, _, _) = self.invoke(&plugin, "describe", access, |bindings, store| {
            bindings.call_describe(store)
        })?;
        plugin.info = info_from(info);
        check_declaration(name, &plugin.info)?;
        Ok(plugin)
    }

    /// Runs one `check` call. Rules that declare `reads-history` or `reads-message` run in a call
    /// of their own that is granted git, its history for the first and the commit message for the
    /// second; the others run in a call without it.
    ///
    /// # Errors
    /// A rule the plugin does not declare, a history rule in a run without history, a trap, fuel
    /// used up, or the plugin's `err`.
    pub fn check(&self, call: &Call<'_>) -> Result<CallOutcome> {
        let plugin = call.plugin;
        let (mut history, mut snapshot) = (Vec::new(), Vec::new());
        for rule in &call.request.rules {
            let info = plugin
                .info
                .rules
                .iter()
                .find(|info| info.name == rule.name)
                .ok_or_else(|| {
                    Error::new(
                        &plugin.name,
                        ErrorKind::Request,
                        format!(
                            "has no rule {:?}; it declares: {}",
                            rule.name,
                            plugin
                                .info
                                .rules
                                .iter()
                                .map(|info| info.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    )
                })?;
            if info.reads_history || info.reads_message {
                &mut history
            } else {
                &mut snapshot
            }
            .push(rule.clone());
        }
        if !history.is_empty() && call.history.is_none() {
            return Err(Error::new(
                &plugin.name,
                ErrorKind::Request,
                format!(
                    "rule {:?} reads git history, but this run has none",
                    history[0].name
                ),
            ));
        }

        let mut outcome = CallOutcome::default();
        for (rules, git) in [(snapshot, None), (history, call.history.clone())] {
            if rules.is_empty() {
                continue;
            }
            let ran: Vec<&RuleInfo> = rules
                .iter()
                .filter_map(|rule| plugin.info.rules.iter().find(|info| info.name == rule.name))
                .collect();
            let request = CheckRequest {
                rules,
                ..call.request.clone()
            };
            let access = Access {
                tree: Arc::clone(&call.tree),
                history: git,
                reads_history: ran.iter().any(|info| info.reads_history),
                reads_message: ran.iter().any(|info| info.reads_message),
                grant: call.grant.clone(),
            };
            let label = format!(
                "check {}",
                request
                    .rules
                    .iter()
                    .map(|rule| rule.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let request = request_to_wit(&request, &call.grant);
            let names: Vec<String> = ran.iter().map(|info| info.name.clone()).collect();
            let (answer, read_set, log, time, fuel_used) =
                self.invoke(plugin, &label, access, |bindings, store| {
                    bindings.call_check(store, &request)
                })?;
            let answer = answer.map_err(|error| answered(&plugin.name, error))?;
            for finding in answer.findings {
                let finding = finding_from(&plugin.name, &call.grant, finding)?;
                ran_rule(&plugin.name, &ran, &finding)?;
                outcome.findings.push(finding);
            }
            for judged in answer.judged {
                outcome
                    .judged
                    .push(judged_from(&plugin.name, &ran, judged)?);
            }
            outcome.read_set.merge(read_set);
            outcome.log.extend(log);
            outcome.fuel_used += fuel_used;
            outcome.invocations.push(Invocation {
                rules: names,
                elapsed: time.elapsed,
                fuel_used,
            });
            outcome.time.merge(time);
        }
        Ok(outcome)
    }

    /// Runs `calls` on the worker pool; the results are in the order of `calls`.
    #[must_use]
    pub fn check_all(&self, calls: &[Call<'_>]) -> Vec<Result<CallOutcome>> {
        let next = AtomicUsize::new(0);
        let workers = self.config.workers.clamp(1, calls.len().max(1));
        let mut results: Vec<Option<Result<CallOutcome>>> =
            (0..calls.len()).map(|_| None).collect();
        let done = std::sync::Mutex::new(Vec::with_capacity(calls.len()));
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let spawned = std::thread::Builder::new()
                    .stack_size(WORKER_STACK)
                    .spawn_scoped(scope, || {
                        loop {
                            let index = next.fetch_add(1, Ordering::Relaxed);
                            let Some(call) = calls.get(index) else { break };
                            let result = self.check(call);
                            if let Ok(mut done) = done.lock() {
                                done.push((index, result));
                            }
                        }
                    });
                if let Err(error) = spawned
                    && let Ok(mut done) = done.lock()
                {
                    done.push((
                        usize::MAX,
                        Err(Error::new(
                            "",
                            ErrorKind::Setup,
                            format!("cannot start a worker: {error}"),
                        )),
                    ));
                }
            }
        });
        let finished = done
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut failure = None;
        for (index, result) in finished {
            match results.get_mut(index) {
                Some(slot) => *slot = Some(result),
                None => failure = Some(result),
            }
        }
        results
            .into_iter()
            .map(|result| {
                result.unwrap_or_else(|| {
                    failure.clone().unwrap_or_else(|| {
                        Err(Error::new(
                            "",
                            ErrorKind::Setup,
                            "a worker stopped before its call finished",
                        ))
                    })
                })
            })
            .collect()
    }

    /// Runs the generator `name` of `plugin` for the project at `grant`'s root with `options`
    /// (JSON) and the plugin's `[plugins.<name>]` options `plugin_options` (JSON). The files come
    /// back at repository paths: the generator names them relative to the project root.
    ///
    /// # Errors
    /// A trap, fuel used up, the plugin's `err`, or a file outside the project root.
    pub fn generate(
        &self,
        plugin: &Plugin,
        tree: Arc<dyn Tree>,
        grant: &Grant,
        name: &str,
        options: &str,
        plugin_options: &str,
    ) -> Result<(GenerateOutput, ReadSet, Vec<String>)> {
        let project = grant.root().to_owned();
        let access = Access {
            tree,
            history: None,
            reads_history: false,
            reads_message: false,
            grant: grant.clone(),
        };
        let label = format!("generate {name}");
        let (answer, read_set, log, _, _) =
            self.invoke(plugin, &label, access, |bindings, store| {
                bindings.call_generate(store, name, &project, options, plugin_options)
            })?;
        let output = answer.map_err(|error| answered(&plugin.name, error))?;
        let output = GenerateOutput {
            files: output
                .files
                .into_iter()
                .map(|file| {
                    let path = grant.to_output(&file.path).ok_or_else(|| {
                        Error::new(
                            &plugin.name,
                            ErrorKind::Plugin,
                            format!(
                                "generator {name} writes {:?}, which is not a path below the project root {:?}; a generator writes inside its project only",
                                file.path,
                                grant.root()
                            ),
                        )
                    })?;
                    if is_core_path(&path) {
                        return Err(Error::new(
                            &plugin.name,
                            ErrorKind::Plugin,
                            format!(
                                "generator {name} writes {path:?}, a file the core owns ({CORE_OWNS}); a generator never writes those"
                            ),
                        ));
                    }
                    Ok(GeneratedFile {
                        path,
                        contents: file.contents,
                        seed: file.seed,
                    })
                })
                .collect::<Result<_>>()?,
            findings: output
                .findings
                .into_iter()
                .map(|finding| finding_from(&plugin.name, grant, finding))
                .collect::<Result<_>>()?,
        };
        Ok((output, read_set, log))
    }

    /// Instantiates `plugin` in a fresh store with `access`, runs `call` (`label` names it in
    /// errors), and returns its answer with the read set and log.
    fn invoke<T>(
        &self,
        plugin: &Plugin,
        label: &str,
        access: Access,
        call: impl FnOnce(&Bindings, &mut Store<State>) -> wasmtime::Result<T>,
    ) -> Result<(T, ReadSet, Vec<String>, CallTime, u64)> {
        let mut store = Store::new(&self.engine, State::new(access, self.config.memory_bytes));
        store.limiter(|state| &mut state.limits);
        store
            .set_fuel(self.config.fuel)
            .map_err(|error| Error::new(&plugin.name, ErrorKind::Setup, format!("{error:#}")))?;
        // The deadline counts the plugin's own time: when the epoch reaches it, the time spent in
        // host calls (git, sync-state, reads) moves it forward by as much.
        let deadline = self.config.deadline;
        let started = Instant::now();
        store.set_epoch_deadline(ticks(deadline));
        store.epoch_deadline_callback(move |context| {
            let own = started.elapsed().saturating_sub(context.data().host_time());
            Ok(if own >= deadline {
                UpdateDeadline::Interrupt
            } else {
                UpdateDeadline::Continue(ticks(deadline.saturating_sub(own)))
            })
        });
        let answer = plugin
            .pre
            .instantiate(&mut store)
            .map_err(|error| {
                if error.downcast_ref::<Trap>() == Some(&Trap::Interrupt) {
                    return trap(&plugin.name, &error);
                }
                Error::new(
                    &plugin.name,
                    ErrorKind::Load,
                    format!("cannot instantiate: {error:#}"),
                )
            })
            .and_then(|bindings| {
                call(&bindings, &mut store).map_err(|error| trap(&plugin.name, &error))
            });
        // Out of fuel leaves none: the call used all it was given.
        let fuel_used = self
            .config
            .fuel
            .saturating_sub(store.get_fuel().unwrap_or(0));
        let limit_hit = store.data().limit_hit();
        let refused = store.data().refused().map(str::to_owned);
        let (read_set, log, host) = store.into_data().into_log();
        let time = CallTime::new(started.elapsed(), host);
        let answer = answer.map_err(|error| match (error.kind, limit_hit, refused) {
            (ErrorKind::Trap, Some(limit), _) => {
                Error::new(&plugin.name, ErrorKind::OutOfMemory, limit)
            }
            (ErrorKind::Trap, None, Some(refused)) => {
                Error::new(&plugin.name, ErrorKind::Plugin, refused)
            }
            (ErrorKind::OutOfFuel, _, _) => Error::new(
                &plugin.name,
                ErrorKind::OutOfFuel,
                format!(
                    "hit the fuel limit of {} units: the call did more work than the core allows; look for a loop that does not end",
                    self.config.fuel
                ),
            ),
            (ErrorKind::Deadline, _, _) => Error::new(
                &plugin.name,
                ErrorKind::Deadline,
                format!(
                    "the call {label} ran past the deadline of {} s; a loaded machine can cause this, so run again, and if it recurs look for a call that does too much work",
                    self.config.deadline.as_secs_f64()
                ),
            ),
            _ => error,
        });
        match answer {
            Ok(answer) => Ok((answer, read_set, log, time, fuel_used)),
            Err(mut error) => {
                let stderr: Vec<&str> = log
                    .iter()
                    .filter_map(|line| line.strip_prefix("stderr: "))
                    .collect();
                if !stderr.is_empty() {
                    error.message =
                        format!("{}; its stderr: {}", error.message, stderr.join(" | "));
                }
                Err(error)
            }
        }
    }
}

/// The epoch ticks that cover `time`, at least one.
fn ticks(time: Duration) -> u64 {
    u64::try_from(time.as_nanos().div_ceil(TICK.as_nanos()).max(1)).unwrap_or(u64::MAX)
}

/// The error a plugin answered, of its kind.
fn answered(plugin: &str, error: wit::PluginError) -> Error {
    match error {
        wit::PluginError::Configuration(message) => {
            Error::new(plugin, ErrorKind::Configuration, message)
        }
        wit::PluginError::Environment(message) => Error::new(plugin, ErrorKind::Reported, message),
    }
}

fn trap(plugin: &str, error: &wasmtime::Error) -> Error {
    match error.downcast_ref::<Trap>() {
        Some(Trap::OutOfFuel) => {
            return Error::new(plugin, ErrorKind::OutOfFuel, "hit the fuel limit");
        }
        Some(Trap::Interrupt) => {
            return Error::new(plugin, ErrorKind::Deadline, "ran past the deadline");
        }
        _ => {}
    }
    Error::new(plugin, ErrorKind::Trap, format!("trapped: {error:#}"))
}

/// Refuses a declaration the core cannot run: a rule or generator declared twice, a project-scope
/// rule that reads the commit message, a generator that owns a glob outside its project.
fn check_declaration(name: &str, info: &PluginInfo) -> Result<()> {
    for (kind, names) in [
        (
            "rule",
            info.rules
                .iter()
                .map(|rule| rule.name.as_str())
                .collect::<Vec<_>>(),
        ),
        (
            "generator",
            info.generators
                .iter()
                .map(|generator| generator.name.as_str())
                .collect(),
        ),
    ] {
        let mut seen = std::collections::BTreeSet::new();
        if let Some(twice) = names.iter().find(|name| !seen.insert(**name)) {
            return Err(Error::new(
                name,
                ErrorKind::Load,
                format!("declares the {kind} {twice:?} twice"),
            ));
        }
    }
    if let Some(rule) = info
        .rules
        .iter()
        .find(|rule| rule.reads_message && rule.scope == Scope::Project)
    {
        return Err(Error::new(
            name,
            ErrorKind::Load,
            format!(
                "rule {:?} declares reads-message but is a project-scope rule; the commit message belongs to the repository, so a rule that reads it is repository-scope; report it to the plugin's authors",
                rule.name
            ),
        ));
    }
    for generator in &info.generators {
        for glob in &generator.owns {
            let wrong = if !is_relative_glob(glob) {
                "is not relative to the project root (no leading /, no . or .. segment)"
            } else if has_git_segment(glob) {
                "reaches into git's own directory"
            } else if owns_everything(glob) {
                "matches every file of the project"
            } else {
                continue;
            };
            return Err(Error::new(
                name,
                ErrorKind::Load,
                format!(
                    "generator {} owns the glob {glob:?}, which {wrong}; report it to the plugin's authors",
                    generator.name
                ),
            ));
        }
    }
    Ok(())
}

fn empty_info() -> PluginInfo {
    PluginInfo {
        name: String::new(),
        version: String::new(),
        options_schema: String::new(),
        rules: Vec::new(),
        generators: Vec::new(),
        presets: Vec::new(),
    }
}

fn info_from(info: wit::PluginInfo) -> PluginInfo {
    PluginInfo {
        name: info.name,
        version: info.version,
        options_schema: info.options_schema,
        rules: info
            .rules
            .into_iter()
            .map(|rule| RuleInfo {
                name: rule.name,
                title: rule.title,
                aspects: rule.aspects,
                scope: match rule.scope {
                    wit::Scope::Project => Scope::Project,
                    wit::Scope::Repository => Scope::Repository,
                },
                reads_history: rule.reads_history,
                reads_message: rule.reads_message,
                languages: rule.languages,
                options_schema: rule.options_schema,
            })
            .collect(),
        generators: info
            .generators
            .into_iter()
            .map(|generator| GeneratorInfo {
                name: generator.name,
                title: generator.title,
                options_schema: generator.options_schema,
                owns: generator.owns,
            })
            .collect(),
        presets: info
            .presets
            .into_iter()
            .map(|preset| PresetInfo {
                name: preset.name,
                rules: preset.rules,
            })
            .collect(),
    }
}

/// The request as the plugin reads it. Its options files and the places of its rule decisions are
/// repository paths in `request`, as the core read them; the plugin addresses every path relative to `grant`'s root, so it gets them that
/// way, and a finding it reports at one is mapped back to the repository once.
fn request_to_wit(request: &CheckRequest, grant: &Grant) -> wit::CheckRequest {
    wit::CheckRequest {
        project: request.project.clone(),
        rules: request
            .rules
            .iter()
            .map(|rule| wit::RuleRun {
                name: rule.name.clone(),
                options: rule.options.clone(),
                options_file: rule
                    .options_file
                    .as_deref()
                    .map(|file| grant.to_plugin(file)),
                exceptions: rule.exceptions.clone(),
            })
            .collect(),
        plugin_options: request.plugin_options.clone(),
        languages: request.languages.clone(),
        norms: request.norms.clone(),
        config_file: if request.config_file.is_empty() {
            String::new()
        } else {
            grant.to_plugin(&request.config_file)
        },
        rule_decisions: request
            .rule_decisions
            .iter()
            .map(|decision| wit::RuleDecision {
                key: decision.key.clone(),
                decision: decision.decision.clone(),
                at: wit::FileLocation {
                    path: grant.to_plugin(&decision.at.path),
                    span: decision.at.span.map(|span| wit::Span {
                        line: span.line,
                        column: span.column,
                        end_line: span.end_line,
                        end_column: span.end_column,
                    }),
                },
            })
            .collect(),
    }
}

/// What a rule of the call judged, checked: the rule ran in this invocation.
fn judged_from(plugin: &str, ran: &[&RuleInfo], judged: wit::JudgedTargets) -> Result<Judged> {
    if !ran.iter().any(|rule| rule.name == judged.rule) {
        return Err(Error::new(
            plugin,
            ErrorKind::Plugin,
            format!(
                "listed what rule {:?} judged, which this call did not run; report it to the plugin's authors",
                judged.rule
            ),
        ));
    }
    Ok(Judged {
        rule: judged.rule,
        targets: judged.targets,
        rejudged: judged.rejudged,
    })
}

/// Fails unless `finding` is of a rule the call ran (`ran`), with no aspect or one the rule
/// declares.
fn ran_rule(plugin: &str, ran: &[&RuleInfo], finding: &Finding) -> Result<()> {
    let Some(rule) = ran.iter().find(|rule| rule.name == finding.rule) else {
        return Err(Error::new(
            plugin,
            ErrorKind::Plugin,
            format!(
                "reported a finding of rule {:?}, which this call did not run; report it to the plugin's authors",
                finding.rule
            ),
        ));
    };
    if !finding.aspect.is_empty() && !rule.aspects.contains(&finding.aspect) {
        return Err(Error::new(
            plugin,
            ErrorKind::Plugin,
            format!(
                "reported a finding of rule {} with aspect {:?}, which the rule does not declare (it declares: {}); report it to the plugin's authors",
                rule.name,
                finding.aspect,
                if rule.aspects.is_empty() {
                    "none".to_owned()
                } else {
                    rule.aspects.join(", ")
                }
            ),
        ));
    }
    Ok(())
}

/// A finding with a file location as a repository path; a commit location must be a full id.
fn finding_from(plugin: &str, grant: &Grant, finding: wit::Finding) -> Result<Finding> {
    let location = match finding.location {
        None => None,
        Some(wit::Location::Commit(id)) => {
            if !is_full_id(&id) {
                return Err(Error::new(
                    plugin,
                    ErrorKind::Plugin,
                    format!(
                        "reported a finding at commit {id:?}, which is not a full commit id (40 or 64 lowercase hex digits); report it to the plugin's authors"
                    ),
                ));
            }
            Some(Location::Commit(id))
        }
        Some(wit::Location::File(file)) => {
            let path = grant.resolve(&file.path).ok_or_else(|| {
                Error::new(
                    plugin,
                    ErrorKind::Plugin,
                    format!(
                        "reported a finding at {:?}, which is outside the repository",
                        file.path
                    ),
                )
            })?;
            Some(Location::File(FileLocation {
                path,
                span: file.span.map(|span| Span {
                    line: span.line,
                    column: span.column,
                    end_line: span.end_line,
                    end_column: span.end_column,
                }),
            }))
        }
    };
    Ok(Finding {
        rule: finding.rule,
        aspect: finding.aspect,
        message: finding.message,
        location,
        target: finding.target,
        help: finding.help,
        key: finding.key,
    })
}

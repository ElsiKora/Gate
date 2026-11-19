//! The spike host: runs the workload in one of three forms and prints one JSON line of host-side
//! timings with every plugin's own outcome.
//!
//! ```text
//! spike-host native  --tree DIR --tasks parse,walk [--plugins N]
//! spike-host wasm    --tree DIR --tasks parse,walk --component X.wasm [--precompiled X.cwasm] [--access host|fs] [--plugins N]
//! spike-host process --tree DIR --tasks parse,walk --plugin BIN [--access fs|rpc] [--plugins N]
//! spike-host compile --component X.wasm --out X.cwasm
//! spike-host table   native|wasm|process --tree DIR [--component ..] [--precompiled ..] [--plugin ..]
//! ```
//!
//! `--plugins N` runs N copies at once, each on its own thread, instance or process, each parsing
//! the whole tree on its own.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::time::Instant;

use serde_json::{Value, json};
use spike_workload::frame::{self, BYTES, JSON, NONE};
use spike_workload::{DirectoryTree, Outcome, Request};
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({ world: "plugin", path: "../wit" });

const STACK: usize = 8 << 20;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, Error>;

fn main() {
    if let Err(error) = run() {
        eprintln!("spike-host: {error}");
        std::process::exit(1);
    }
}

#[derive(Debug, Default)]
struct Arguments {
    command: String,
    form: String,
    tree: PathBuf,
    tasks: Vec<String>,
    access: String,
    plugins: usize,
    component: PathBuf,
    precompiled: Option<PathBuf>,
    plugin: PathBuf,
    out: PathBuf,
}

fn arguments() -> Result<Arguments> {
    let mut raw = std::env::args().skip(1);
    let mut arguments = Arguments {
        command: raw.next().ok_or("usage: spike-host <native|wasm|process|compile|table> ..")?,
        plugins: 1,
        ..Arguments::default()
    };
    if arguments.command == "table" {
        arguments.form = raw.next().ok_or("table needs a form: native, wasm or process")?;
    }
    while let Some(flag) = raw.next() {
        let value = raw.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--tree" => arguments.tree = value.into(),
            "--tasks" => arguments.tasks = value.split(',').map(str::to_owned).collect(),
            "--access" => arguments.access = value,
            "--plugins" => arguments.plugins = value.parse()?,
            "--component" => arguments.component = value.into(),
            "--precompiled" => arguments.precompiled = Some(value.into()),
            "--plugin" => arguments.plugin = value.into(),
            "--out" => arguments.out = value.into(),
            other => return Err(format!("unknown flag {other}").into()),
        }
    }
    Ok(arguments)
}

fn milliseconds(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1e3
}

fn run() -> Result<()> {
    let arguments = arguments()?;
    let started = Instant::now();

    let report = match arguments.command.as_str() {
        "compile" => compile(&arguments)?,
        "table" => table(&arguments)?,
        form => {
            let listing_started = Instant::now();
            let listing = Arc::new(spike_workload::list_directory(&arguments.tree)?);
            let listing_ms = milliseconds(listing_started);
            let (setup, outcomes) = match form {
                "native" => (json!({}), native(&arguments, &listing)?),
                "wasm" => wasm(&arguments, &listing)?,
                "process" => (json!({}), process(&arguments, &listing)?),
                other => return Err(format!("unknown form {other}").into()),
            };
            let first = outcomes.first().map(|plugin| &plugin["outcome"]["counts"]);
            let same = outcomes.iter().all(|plugin| Some(&plugin["outcome"]["counts"]) == first);
            json!({
                "form": form, "access": arguments.access, "tasks": arguments.tasks,
                "plugins": arguments.plugins, "listing_ms": listing_ms, "setup": setup,
                "same_counts": same, "outcomes": outcomes,
            })
        }
    };

    let mut report = report;
    report["wall_ms"] = json!(milliseconds(started));
    println!("{report}");
    Ok(())
}

/// Runs `body` on `count` threads at once with an 8 MiB stack each and collects the results.
fn parallel<T: Send + 'static>(
    count: usize,
    body: impl Fn(usize) -> Result<T> + Send + Sync + 'static,
) -> Result<Vec<T>> {
    let body = Arc::new(body);
    let handles: Vec<_> = (0..count)
        .map(|index| {
            let body = Arc::clone(&body);
            std::thread::Builder::new()
                .stack_size(STACK)
                .spawn(move || body(index).map_err(|error| error.to_string()))
        })
        .collect::<std::io::Result<_>>()?;
    handles
        .into_iter()
        .map(|handle| {
            handle
                .join()
                .map_err(|_| Error::from("a plugin thread panicked"))?
                .map_err(Error::from)
        })
        .collect()
}

// --- native -------------------------------------------------------------------------------------

fn native(arguments: &Arguments, listing: &Arc<Vec<String>>) -> Result<Vec<Value>> {
    let root = arguments.tree.clone();
    let tasks = arguments.tasks.clone();
    let listing = Arc::clone(listing);
    parallel(arguments.plugins, move |_| {
        let started = Instant::now();
        let mut tree = DirectoryTree { root: root.clone(), listing: (*listing).clone() };
        let outcome = spike_workload::run(&mut tree, &Request { tasks: tasks.clone() });
        Ok(json!({"call_ms": milliseconds(started), "outcome": outcome}))
    })
}

// --- wasm ---------------------------------------------------------------------------------------

/// The store state of one instance: WASI, and the tree the host calls serve.
struct State {
    wasi: WasiCtx,
    table: ResourceTable,
    root: PathBuf,
    listing: Arc<Vec<String>>,
    tracked: Arc<HashSet<String>>,
    reads: u64,
    read_bytes: u64,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

impl gate::spike::tree::Host for State {
    fn list_files(&mut self) -> Vec<String> {
        (*self.listing).clone()
    }

    fn read_file(&mut self, path: String) -> Option<Vec<u8>> {
        // Only tracked files are readable, so a guest cannot leave the tree.
        if !self.tracked.contains(&path) {
            return None;
        }
        let bytes = std::fs::read(self.root.join(&path)).ok()?;
        self.reads += 1;
        self.read_bytes += bytes.len() as u64;
        Some(bytes)
    }
}

fn engine() -> Result<Engine> {
    Ok(Engine::new(&Config::new())?)
}

/// The component, compiled from the `.wasm` file or loaded from a precompiled artifact.
fn load(engine: &Engine, arguments: &Arguments) -> Result<(Component, Value)> {
    let started = Instant::now();
    let component = match &arguments.precompiled {
        // SAFETY: the artifact was written by `spike-host compile` with the same wasmtime and
        // engine configuration; deserializing an untrusted artifact is unsound, which is why the
        // product would use the checksum-verified cache only.
        Some(path) => unsafe { Component::deserialize_file(engine, path)? },
        None => Component::from_file(engine, &arguments.component)?,
    };
    let how = if arguments.precompiled.is_some() { "deserialize_ms" } else { "compile_ms" };
    Ok((component, json!({ how: milliseconds(started) })))
}

fn linker(engine: &Engine) -> Result<Linker<State>> {
    let mut linker = Linker::<State>::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    Plugin::add_to_linker::<State, HasSelf<State>>(&mut linker, |state| state)?;
    Ok(linker)
}

fn store(engine: &Engine, root: &Path, listing: &Arc<Vec<String>>, access: &str) -> Result<Store<State>> {
    let mut wasi = WasiCtxBuilder::new();
    wasi.inherit_stderr();
    if access == "fs" {
        wasi.preopened_dir(root, "/tree", FsPerms::ReadOnly)?;
    }
    Ok(Store::new(
        engine,
        State {
            wasi: wasi.build(),
            table: ResourceTable::new(),
            root: root.to_path_buf(),
            listing: Arc::clone(listing),
            tracked: Arc::new(listing.iter().cloned().collect()),
            reads: 0,
            read_bytes: 0,
        },
    ))
}

fn wasm(arguments: &Arguments, listing: &Arc<Vec<String>>) -> Result<(Value, Vec<Value>)> {
    let engine = engine()?;
    let (component, setup) = load(&engine, arguments)?;
    let linker = Arc::new(linker(&engine)?);
    let root = arguments.tree.clone();
    let listing = Arc::clone(listing);
    let access = if arguments.access.is_empty() { "host".to_owned() } else { arguments.access.clone() };
    let request = json!({"tasks": arguments.tasks, "access": access}).to_string();

    let outcomes = parallel(arguments.plugins, move |_| {
        let started = Instant::now();
        let mut store = store(&engine, &root, &listing, &access)?;
        let plugin = Plugin::instantiate(&mut store, &component, &linker)?;
        let instantiate_ms = milliseconds(started);
        let call = Instant::now();
        let answer = plugin.call_run(&mut store, &request)?;
        let outcome: Outcome = serde_json::from_str(&answer)?;
        Ok(json!({
            "instantiate_ms": instantiate_ms, "call_ms": milliseconds(call),
            "host_reads": store.data().reads, "host_read_bytes": store.data().read_bytes,
            "outcome": outcome,
        }))
    })?;
    Ok((setup, outcomes))
}

fn compile(arguments: &Arguments) -> Result<Value> {
    let engine = engine()?;
    let started = Instant::now();
    let component = Component::from_file(&engine, &arguments.component)?;
    let compile_ms = milliseconds(started);
    let started = Instant::now();
    let bytes = component.serialize()?;
    std::fs::write(&arguments.out, &bytes)?;
    let serialize_ms = milliseconds(started);
    let started = Instant::now();
    // SAFETY: the artifact was written just above by this engine.
    let loaded = unsafe { Component::deserialize_file(&engine, &arguments.out)? };
    let deserialize_ms = milliseconds(started);
    let started = Instant::now();
    let linker = linker(&engine)?;
    let mut store = store(&engine, Path::new("."), &Arc::new(Vec::new()), "host")?;
    let _plugin = Plugin::instantiate(&mut store, &loaded, &linker)?;
    let instantiate_ms = milliseconds(started);
    Ok(json!({
        "command": "compile", "wasm_bytes": std::fs::metadata(&arguments.component)?.len(),
        "compile_ms": compile_ms, "serialize_ms": serialize_ms, "artifact_bytes": bytes.len(),
        "deserialize_ms": deserialize_ms, "linker_and_instantiate_ms": instantiate_ms,
    }))
}

// --- process ------------------------------------------------------------------------------------

/// One plugin process under `/usr/bin/time -l`, so its own peak memory is known.
struct Process {
    child: Child,
    input: std::io::BufWriter<ChildStdin>,
    output: std::io::BufReader<ChildStdout>,
    root: PathBuf,
    tracked: Arc<HashSet<String>>,
    listing: Arc<Vec<String>>,
    reads: u64,
}

impl Process {
    fn spawn(plugin: &Path, root: &Path, listing: &Arc<Vec<String>>) -> Result<Self> {
        let mut child = Command::new("/usr/bin/time")
            .arg("-l")
            .arg(plugin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let input = std::io::BufWriter::with_capacity(1 << 20, child.stdin.take().ok_or("no stdin")?);
        let output = std::io::BufReader::with_capacity(1 << 20, child.stdout.take().ok_or("no stdout")?);
        Ok(Self {
            child,
            input,
            output,
            root: root.to_path_buf(),
            tracked: Arc::new(listing.iter().cloned().collect()),
            listing: Arc::clone(listing),
            reads: 0,
        })
    }

    fn send(&mut self, message: &Value) -> Result<()> {
        Ok(frame::write(&mut self.input, JSON, message.to_string().as_bytes())?)
    }

    /// Serves the plugin's `list` and `read` requests until it answers with something else.
    fn serve(&mut self) -> Result<(u8, Vec<u8>)> {
        loop {
            let (kind, payload) = frame::read(&mut self.output)?.ok_or("the plugin closed its pipe")?;
            if kind != JSON {
                return Ok((kind, payload));
            }
            let message: Value = serde_json::from_slice(&payload)?;
            match message["op"].as_str() {
                Some("list") => {
                    let listing = serde_json::to_vec(&*self.listing)?;
                    frame::write(&mut self.input, JSON, &listing)?;
                }
                Some("read") => {
                    let path = message["path"].as_str().unwrap_or_default();
                    let bytes = self
                        .tracked
                        .contains(path)
                        .then(|| std::fs::read(self.root.join(path)).ok())
                        .flatten();
                    self.reads += 1;
                    match bytes {
                        Some(bytes) => frame::write(&mut self.input, BYTES, &bytes)?,
                        None => frame::write(&mut self.input, NONE, &[])?,
                    }
                }
                _ => return Ok((kind, payload)),
            }
        }
    }

    /// Closes the pipe and returns the plugin's peak resident set in bytes.
    fn finish(mut self) -> Result<u64> {
        drop(self.input);
        let output = std::mem::replace(&mut self.child, Command::new("true").spawn()?).wait_with_output()?;
        let text = String::from_utf8_lossy(&output.stderr);
        Ok(text
            .lines()
            .find(|line| line.contains("maximum resident set size"))
            .and_then(|line| line.split_whitespace().next())
            .and_then(|number| number.parse().ok())
            .unwrap_or(0))
    }
}

fn process(arguments: &Arguments, listing: &Arc<Vec<String>>) -> Result<Vec<Value>> {
    let plugin = arguments.plugin.clone();
    let root = arguments.tree.clone();
    let listing = Arc::clone(listing);
    let access = if arguments.access.is_empty() { "fs".to_owned() } else { arguments.access.clone() };
    let tasks = arguments.tasks.clone();
    parallel(arguments.plugins, move |_| {
        let started = Instant::now();
        let mut process = Process::spawn(&plugin, &root, &listing)?;
        let spawn_ms = milliseconds(started);
        let call = Instant::now();
        process.send(&json!({
            "op": "run", "tasks": tasks, "access": access, "root": root.to_string_lossy(),
        }))?;
        let (_, answer) = process.serve()?;
        let call_ms = milliseconds(call);
        let answer: Value = serde_json::from_slice(&answer)?;
        let reads = process.reads;
        let rss = process.finish()?;
        Ok(json!({
            "spawn_ms": spawn_ms, "call_ms": call_ms, "pipe_reads": reads,
            "plugin_max_rss_bytes": rss, "outcome": answer["outcome"],
        }))
    })
}

// --- cross-boundary -----------------------------------------------------------------------------

/// Plugin A parses and builds the flat node table; the host carries it to plugin B, which runs the
/// decorator rule on it. Then B parses on its own and runs the same rule on its syntax trees.
fn table(arguments: &Arguments) -> Result<Value> {
    let listing = Arc::new(spike_workload::list_directory(&arguments.tree)?);
    let root = arguments.tree.clone();
    let walk = vec!["walk".to_owned()];
    let mut out = HashMap::new();

    match arguments.form.as_str() {
        "native" => {
            let listing = Arc::clone(&listing);
            let result = parallel(1, move |_| {
                let mut tree = DirectoryTree { root: root.clone(), listing: (*listing).clone() };
                let started = Instant::now();
                let bytes = spike_workload::table(&mut tree, &mut Outcome::default());
                let build_ms = milliseconds(started);
                let started = Instant::now();
                let mut rule = Outcome::default();
                spike_workload::table_rule(&bytes, &mut rule);
                let rule_ms = milliseconds(started);
                let started = Instant::now();
                let parsed = spike_workload::run(&mut tree, &Request { tasks: walk.clone() });
                let parse_ms = milliseconds(started);
                Ok(json!({
                    "a_parse_and_table_ms": build_ms, "table_bytes": bytes.len(),
                    "b_table_rule_ms": rule_ms, "b_table_counts": rule.counts,
                    "b_parse_and_walk_ms": parse_ms, "b_walk_counts": parsed.counts,
                }))
            })?;
            out.insert("result", result.into_iter().next().unwrap_or_default());
        }
        "wasm" => {
            let engine = engine()?;
            let (component, setup) = load(&engine, arguments)?;
            let linker = linker(&engine)?;
            let request = json!({"tasks": ["walk"], "access": "host"}).to_string();
            let mut a = store(&engine, &root, &listing, "host")?;
            let plugin_a = Plugin::instantiate(&mut a, &component, &linker)?;
            let mut b = store(&engine, &root, &listing, "host")?;
            let plugin_b = Plugin::instantiate(&mut b, &component, &linker)?;
            let started = Instant::now();
            let bytes = plugin_a.call_table(&mut a, &request)?;
            let build_ms = milliseconds(started);
            let started = Instant::now();
            let rule: Outcome = serde_json::from_str(&plugin_b.call_table_rule(&mut b, &bytes)?)?;
            let rule_ms = milliseconds(started);
            let started = Instant::now();
            let parsed: Outcome = serde_json::from_str(&plugin_b.call_run(&mut b, &request)?)?;
            let parse_ms = milliseconds(started);
            out.insert(
                "result",
                json!({
                    "setup": setup,
                    "a_parse_and_table_ms": build_ms, "table_bytes": bytes.len(),
                    "b_table_rule_ms": rule_ms, "b_table_counts": rule.counts,
                    "b_table_rule_inside_ms": rule.phases.get("table.rule"),
                    "b_parse_and_walk_ms": parse_ms, "b_walk_counts": parsed.counts,
                }),
            );
        }
        "process" => {
            let mut a = Process::spawn(&arguments.plugin, &root, &listing)?;
            let mut b = Process::spawn(&arguments.plugin, &root, &listing)?;
            let started = Instant::now();
            a.send(&json!({"op": "table", "access": "fs", "root": root.to_string_lossy()}))?;
            let (_, bytes) = a.serve()?;
            let build_ms = milliseconds(started);
            let started = Instant::now();
            b.send(&json!({"op": "table-rule"}))?;
            frame::write(&mut b.input, BYTES, &bytes)?;
            let (_, rule) = b.serve()?;
            let rule_ms = milliseconds(started);
            let rule: Value = serde_json::from_slice(&rule)?;
            let started = Instant::now();
            b.send(&json!({"op": "run", "tasks": walk, "access": "fs", "root": root.to_string_lossy()}))?;
            let (_, parsed) = b.serve()?;
            let parse_ms = milliseconds(started);
            let parsed: Value = serde_json::from_slice(&parsed)?;
            a.finish()?;
            b.finish()?;
            out.insert(
                "result",
                json!({
                    "a_parse_and_table_ms": build_ms, "table_bytes": bytes.len(),
                    "b_table_rule_ms": rule_ms, "b_table_counts": rule["outcome"]["counts"],
                    "b_parse_and_walk_ms": parse_ms, "b_walk_counts": parsed["outcome"]["counts"],
                }),
            );
        }
        other => return Err(format!("unknown form {other}").into()),
    }

    Ok(json!({"command": "table", "form": arguments.form, "result": out.remove("result")}))
}

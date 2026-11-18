//! The data of one store (one call) and the host side of the `tree`, `git` and `log` imports.

use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use wasmtime::ResourceLimiter;
use wasmtime::component::{HasSelf, Linker, ResourceTable};
use wasmtime_wasi::cli::{IsTerminal, StdoutStream};
use wasmtime_wasi::p2::{OutputStream, Pollable, StreamResult};
use wasmtime_wasi::{
    Deterministic, HostMonotonicClock, HostWallClock, WasiCtx, WasiCtxBuilder, WasiCtxView,
    WasiView,
};

use crate::grant::Grant;
use crate::host::{History, HostError, Tree};
use crate::read_set::{GitQuery, ReadSet, range_ids, sync_state_digest};
use crate::runtime::HostTime;

wasmtime::component::bindgen!({
    world: "plugin",
    path: "../../wit",
    imports: { default: trappable },
});

pub(crate) use self::gate::plugin::{git as wit_git, types as wit};

/// The most one call keeps of what a plugin writes to stdout, and to stderr: the call goes on, the
/// rest is dropped, and the log says so once.
const OUTPUT_BYTES: usize = 1 << 20;
/// The most one call keeps of its `log` messages, counting one byte more per message: the rest is
/// dropped, and the log says so once.
const LOG_BYTES: usize = 1 << 20;
/// The most entries (listed prefixes, read paths, git questions) one call's read set holds.
const READ_SET_ENTRIES: usize = 1 << 20;
/// The most bytes of strings one call's read set holds.
const READ_SET_BYTES: usize = 64 << 20;
/// The most core instances, linear memories and tables one call may create; a component the SDK
/// builds creates a handful of each.
const INSTANCES: usize = 100;
const MEMORIES: usize = 16;
const TABLES: usize = 100;
/// What one table element counts against the memory limit: a pointer.
const TABLE_ELEMENT_BYTES: usize = 8;

/// What a call reaches.
pub(crate) struct Access {
    pub(crate) tree: Arc<dyn Tree>,
    pub(crate) history: Option<Arc<dyn History>>,
    pub(crate) grant: Grant,
}

pub(crate) struct State {
    wasi: WasiCtx,
    table: ResourceTable,
    pub(crate) limits: MemoryLimit,
    access: Access,
    read_set: ReadSet,
    /// The bytes of strings in the read set.
    read_set_bytes: usize,
    /// Whether the call tried to grow its read set past the limit.
    read_set_full: bool,
    /// Why the call failed through its own fault, when a host function refused it.
    refused: Option<String>,
    log: Vec<String>,
    /// What the log holds, counting one byte more per message.
    log_bytes: usize,
    /// Whether the log dropped a message.
    log_full: bool,
    stdout: Output,
    stderr: Output,
    /// The time spent in host calls (`git`, `tree.read`), which the call's deadline does not count.
    host_time: Duration,
    /// The time spent in the host, by kind of host call (`git.sync-state`, `tree.read`).
    host_kinds: BTreeMap<&'static str, HostTime>,
}

impl State {
    pub(crate) fn new(access: Access, memory_bytes: usize) -> Self {
        let stdout = Output::default();
        let stderr = Output::default();
        // The standard library's WASI imports are linked to fixed answers, so a plugin's result
        // depends only on what it reads through `tree` and `git`: no environment, arguments or
        // preopened directories, closed stdin, a constant clock, constant randomness (also the
        // hash-map seed), no network.
        let wasi = WasiCtxBuilder::new()
            .stdout(stdout.clone())
            .stderr(stderr.clone())
            .wall_clock(FixedClock)
            .monotonic_clock(FixedClock)
            .secure_random(Deterministic::new(vec![0x5a; 32]))
            .insecure_random(Deterministic::new(vec![0xa5; 32]))
            .insecure_random_seed(0)
            .allow_tcp(false)
            .allow_udp(false)
            .allow_ip_name_lookup(false)
            .build();
        Self {
            wasi,
            table: ResourceTable::new(),
            limits: MemoryLimit {
                bytes: memory_bytes,
                used: 0,
                hit: false,
            },
            access,
            read_set: ReadSet::default(),
            read_set_bytes: 0,
            read_set_full: false,
            refused: None,
            log: Vec::new(),
            log_bytes: 0,
            log_full: false,
            stdout,
            stderr,
            host_time: Duration::ZERO,
            host_kinds: BTreeMap::new(),
        }
    }

    /// The limit the call hit, said for the error that names it; `None` when it hit none of the
    /// limits counted here.
    pub(crate) fn limit_hit(&self) -> Option<String> {
        if self.limits.hit {
            return Some(format!(
                "hit the memory limit of {} bytes (linear memories and tables together); the call needs more memory than the core allows",
                self.limits.bytes
            ));
        }
        self.read_set_full.then(|| {
            format!(
                "hit the read-set limit of {READ_SET_ENTRIES} entries or {READ_SET_BYTES} bytes: the call listed, read or asked git more than the core records for one call"
            )
        })
    }

    /// Why a host function refused the call, when one did: the plugin's fault.
    pub(crate) fn refused(&self) -> Option<&str> {
        self.refused.as_deref()
    }

    /// The read set, and the log lines followed by what the plugin wrote to stdout and stderr,
    /// one entry per line.
    pub(crate) fn into_log(self) -> (ReadSet, Vec<String>, BTreeMap<&'static str, HostTime>) {
        let mut log = self.log;
        for (stream, output) in [("stdout", &self.stdout), ("stderr", &self.stderr)] {
            let captured = output.take();
            log.extend(
                String::from_utf8_lossy(&captured.bytes)
                    .lines()
                    .map(|line| format!("{stream}: {line}")),
            );
            if captured.dropped {
                log.push(format!(
                    "gate: the plugin wrote more than {OUTPUT_BYTES} bytes to {stream} in this call; the rest is dropped"
                ));
            }
        }
        (self.read_set, log, self.host_kinds)
    }

    /// The time this call has spent in host calls so far.
    pub(crate) fn host_time(&self) -> Duration {
        self.host_time
    }

    /// Runs the host side of a call, its time kept apart from the plugin's own.
    fn host_call<T>(&mut self, kind: &'static str, call: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let answer = call();
        let spent = started.elapsed();
        self.host_time += spent;
        let entry = self.host_kinds.entry(kind).or_default();
        entry.time += spent;
        entry.calls += 1;
        answer
    }

    fn history(&self, function: &str) -> wasmtime::Result<Arc<dyn History>> {
        self.access.history.clone().ok_or_else(|| {
            wasmtime::format_err!(
                "the plugin called git.{function}, but no rule of this call declares reads-history"
            )
        })
    }

    fn record(&mut self, query: GitQuery, depends_on: Vec<String>) -> wasmtime::Result<()> {
        if self.read_set.git.contains_key(&query) {
            return Ok(());
        }
        let bytes = query.bytes() + depends_on.iter().map(String::len).sum::<usize>();
        self.admit(bytes)?;
        self.read_set.git.insert(query, depends_on);
        Ok(())
    }

    fn record_listed(&mut self, prefix: String) -> wasmtime::Result<()> {
        if self.read_set.listed.contains(&prefix) {
            return Ok(());
        }
        self.admit(prefix.len())?;
        self.read_set.listed.insert(prefix);
        Ok(())
    }

    fn record_read(&mut self, path: String, blob: Option<String>) -> wasmtime::Result<()> {
        if self.read_set.read.contains_key(&path) {
            return Ok(());
        }
        self.admit(path.len() + blob.as_ref().map_or(0, String::len))?;
        self.read_set.read.insert(path, blob);
        Ok(())
    }

    /// Counts one more read-set entry of `bytes`; past the limit the call fails.
    fn admit(&mut self, bytes: usize) -> wasmtime::Result<()> {
        self.read_set_bytes = self.read_set_bytes.saturating_add(bytes);
        if self.read_set.entries() >= READ_SET_ENTRIES || self.read_set_bytes > READ_SET_BYTES {
            self.read_set_full = true;
            return Err(wasmtime::format_err!(
                "the call's read set passed the limit"
            ));
        }
        Ok(())
    }

    /// `id` when it is a full commit id; otherwise the call fails as the plugin's fault.
    fn full_id(&mut self, function: &str, id: &str) -> wasmtime::Result<()> {
        if is_full_id(id) {
            return Ok(());
        }
        let message = format!(
            "called git.{function} with {id:?}, which is not a full commit id (40 or 64 lowercase hex digits); resolve a revision with git.resolve first"
        );
        self.refused = Some(message.clone());
        Err(wasmtime::format_err!("{message}"))
    }
}

/// Whether `id` is a full git object id: 40 (SHA-1) or 64 (SHA-256) lowercase hex digits.
pub(crate) fn is_full_id(id: &str) -> bool {
    matches!(id.len(), 40 | 64)
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

/// The memory one call may grow to: every linear memory and table of the call together, counted as
/// they grow (a table element as [`TABLE_ELEMENT_BYTES`]); `hit` records that the call tried to grow
/// past it. The call's instances, memories and tables are counted too.
pub(crate) struct MemoryLimit {
    pub(crate) bytes: usize,
    used: usize,
    hit: bool,
}

impl MemoryLimit {
    fn grow(&mut self, bytes: usize) -> wasmtime::Result<bool> {
        let used = self.used.saturating_add(bytes);
        if used > self.bytes {
            self.hit = true;
            return Err(wasmtime::format_err!(
                "the call tried to grow its memory past the limit"
            ));
        }
        self.used = used;
        Ok(true)
    }
}

impl ResourceLimiter for MemoryLimit {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.grow(desired.saturating_sub(current))
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.grow(
            desired
                .saturating_sub(current)
                .saturating_mul(TABLE_ELEMENT_BYTES),
        )
    }

    fn instances(&self) -> usize {
        INSTANCES
    }

    fn memories(&self) -> usize {
        MEMORIES
    }

    fn tables(&self) -> usize {
        TABLES
    }
}

/// What a call wrote to stdout or stderr: the first [`OUTPUT_BYTES`], and whether more was dropped.
/// Writes past the limit succeed, so the plugin goes on.
#[derive(Clone, Default)]
struct Output(Arc<Mutex<Captured>>);

#[derive(Default)]
struct Captured {
    bytes: Vec<u8>,
    dropped: bool,
}

impl Output {
    fn keep(&self, bytes: &[u8]) {
        let mut captured = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let room = OUTPUT_BYTES.saturating_sub(captured.bytes.len());
        if bytes.len() > room {
            captured.dropped = true;
        }
        captured
            .bytes
            .extend_from_slice(&bytes[..bytes.len().min(room)]);
    }

    fn take(&self) -> Captured {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl IsTerminal for Output {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl StdoutStream for Output {
    fn p2_stream(&self) -> Box<dyn OutputStream> {
        Box::new(self.clone())
    }

    fn async_stream(&self) -> Box<dyn tokio::io::AsyncWrite + Send + Sync> {
        Box::new(self.clone())
    }
}

impl OutputStream for Output {
    fn write(&mut self, bytes: bytes::Bytes) -> StreamResult<()> {
        self.keep(&bytes);
        Ok(())
    }

    fn flush(&mut self) -> StreamResult<()> {
        Ok(())
    }

    fn check_write(&mut self) -> StreamResult<usize> {
        Ok(OUTPUT_BYTES)
    }
}

#[wasmtime_wasi::async_trait]
impl Pollable for Output {
    async fn ready(&mut self) {}
}

impl tokio::io::AsyncWrite for Output {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.keep(bytes);
        Poll::Ready(Ok(bytes.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// The clock every plugin sees: the Unix epoch, standing still.
struct FixedClock;

impl HostWallClock for FixedClock {
    fn resolution(&self) -> Duration {
        Duration::from_nanos(1)
    }

    fn now(&self) -> Duration {
        Duration::ZERO
    }
}

impl HostMonotonicClock for FixedClock {
    fn resolution(&self) -> u64 {
        1
    }

    fn now(&self) -> u64 {
        0
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "used as map_err(host), which passes the error by value"
)]
fn host(error: HostError) -> wasmtime::Error {
    wasmtime::format_err!("the host failed: {error}")
}

impl gate::plugin::types::Host for State {}

impl gate::plugin::tree::Host for State {
    fn paths(&mut self, prefix: String) -> wasmtime::Result<Vec<String>> {
        let grant = &self.access.grant;
        let paths = self
            .access
            .tree
            .paths()
            .iter()
            .filter(|path| grant.allows(path))
            .map(|path| grant.to_plugin(path))
            .filter(|path| path.starts_with(&prefix));
        let mut paths: Vec<String> = paths.collect();
        paths.sort();
        for covered in grant.listed(&prefix) {
            self.record_listed(covered)?;
        }
        Ok(paths)
    }

    fn read(&mut self, path: String) -> wasmtime::Result<Option<Vec<u8>>> {
        let Some(path) = self.access.grant.to_repository(&path) else {
            return Ok(None);
        };
        let tree = Arc::clone(&self.access.tree);
        let blob = self
            .host_call("tree.read", || tree.read(&path))
            .map_err(host)?;
        self.record_read(path, blob.as_ref().map(|blob| blob.id.clone()))?;
        Ok(blob.map(|blob| blob.bytes))
    }

    fn changed(&mut self) -> wasmtime::Result<Option<Vec<String>>> {
        self.read_set.changed = true;
        let grant = &self.access.grant;
        Ok(self.access.tree.changed().map(|paths| {
            paths
                .iter()
                .filter(|path| grant.allows(path))
                .map(|path| grant.to_plugin(path))
                .collect()
        }))
    }
}

impl wit_git::Host for State {
    fn head(&mut self) -> wasmtime::Result<Option<String>> {
        let history = self.history("head")?;
        let head = self
            .host_call("git.head", || history.head())
            .map_err(host)?;
        self.record(GitQuery::Head, head.iter().cloned().collect())?;
        Ok(head)
    }

    fn branch(&mut self) -> wasmtime::Result<Option<String>> {
        let history = self.history("branch")?;
        let branch = self
            .host_call("git.branch", || history.branch())
            .map_err(host)?;
        self.record(GitQuery::Branch, branch.iter().cloned().collect())?;
        Ok(branch)
    }

    fn range(&mut self) -> wasmtime::Result<Option<wit_git::CommitRange>> {
        let history = self.history("range")?;
        let range = self
            .host_call("git.range", || history.range())
            .map_err(host)?;
        self.record(GitQuery::Range, range_ids(range.as_ref()))?;
        Ok(range.map(|range| wit_git::CommitRange {
            base: range.base,
            head: range.head,
            target_ref: range.target_ref,
        }))
    }

    fn resolve(&mut self, rev: String) -> wasmtime::Result<Option<String>> {
        let history = self.history("resolve")?;
        let id = self
            .host_call("git.resolve", || history.resolve(&rev))
            .map_err(host)?;
        self.record(GitQuery::Resolve { rev }, id.iter().cloned().collect())?;
        Ok(id)
    }

    fn tree(&mut self, commit: String) -> wasmtime::Result<String> {
        let history = self.history("tree")?;
        self.full_id("tree", &commit)?;
        let tree = self
            .host_call("git.tree", || history.tree(&commit))
            .map_err(host)?;
        self.record(GitQuery::Tree { commit }, Vec::new())?;
        Ok(tree)
    }

    fn merge_tree(&mut self, first: String, second: String) -> wasmtime::Result<Option<String>> {
        let history = self.history("merge-tree")?;
        self.full_id("merge-tree", &first)?;
        self.full_id("merge-tree", &second)?;
        let tree = self
            .host_call("git.merge-tree", || history.merge_tree(&first, &second))
            .map_err(host)?;
        self.record(GitQuery::MergeTree { first, second }, Vec::new())?;
        Ok(tree)
    }

    fn log(&mut self, range: String) -> wasmtime::Result<Vec<wit_git::Commit>> {
        let history = self.history("log")?;
        let (commits, revisions) = self
            .host_call("git.log", || {
                Ok::<_, HostError>((history.log(&range)?, history.revisions(&range)?))
            })
            .map_err(host)?;
        self.record(GitQuery::Log { range }, revisions)?;
        Ok(commits
            .into_iter()
            .map(|commit| wit_git::Commit {
                id: commit.id,
                parents: commit.parents,
                message: commit.message,
                author_date: commit.author_date,
            })
            .collect())
    }

    fn changed_paths(&mut self, parent: String, commit: String) -> wasmtime::Result<Vec<String>> {
        let history = self.history("changed-paths")?;
        self.full_id("changed-paths", &parent)?;
        self.full_id("changed-paths", &commit)?;
        let paths = self
            .host_call("git.changed-paths", || {
                history.changed_paths(&parent, &commit)
            })
            .map_err(host)?;
        self.record(GitQuery::ChangedPaths { parent, commit }, Vec::new())?;
        let grant = &self.access.grant;
        Ok(paths
            .iter()
            .filter(|path| grant.allows(path))
            .map(|path| grant.to_plugin(path))
            .collect())
    }

    fn read_at(&mut self, commit: String, path: String) -> wasmtime::Result<Option<Vec<u8>>> {
        let history = self.history("read-at")?;
        self.full_id("read-at", &commit)?;
        let Some(path) = self.access.grant.to_repository(&path) else {
            return Ok(None);
        };
        let bytes = self
            .host_call("git.read-at", || history.read_at(&commit, &path))
            .map_err(host)?;
        self.record(GitQuery::ReadAt { commit, path }, Vec::new())?;
        Ok(bytes)
    }

    fn is_ancestor(&mut self, ancestor: String, descendant: String) -> wasmtime::Result<bool> {
        let history = self.history("is-ancestor")?;
        self.full_id("is-ancestor", &ancestor)?;
        self.full_id("is-ancestor", &descendant)?;
        let answer = self
            .host_call("git.is-ancestor", || {
                history.is_ancestor(&ancestor, &descendant)
            })
            .map_err(host)?;
        self.record(
            GitQuery::IsAncestor {
                ancestor,
                descendant,
            },
            Vec::new(),
        )?;
        Ok(answer)
    }

    fn sync_state(&mut self, commit: String) -> wasmtime::Result<wit_git::SyncPaths> {
        let history = self.history("sync-state")?;
        self.full_id("sync-state", &commit)?;
        let state = self
            .host_call("git.sync-state", || history.sync_state(&commit))
            .map_err(host)?;
        self.record(
            GitQuery::SyncState { commit },
            vec![sync_state_digest(&state)],
        )?;
        let grant = &self.access.grant;
        let granted = |paths: Vec<String>| -> Vec<String> {
            paths
                .iter()
                .filter(|path| grant.allows(path))
                .map(|path| grant.to_plugin(path))
                .collect()
        };
        Ok(wit_git::SyncPaths {
            produced: granted(state.produced),
            drift: granted(state.drift),
        })
    }
}

impl PluginImports for State {
    fn log(&mut self, mut message: String) -> wasmtime::Result<()> {
        if self.log_full {
            return Ok(());
        }
        let room = LOG_BYTES.saturating_sub(self.log_bytes);
        if message.len() < room {
            self.log_bytes += message.len() + 1;
            self.log.push(message);
            return Ok(());
        }
        let mut end = room.saturating_sub(1);
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        if !message.is_empty() {
            self.log.push(message);
        }
        self.log.push(format!(
            "gate: the plugin logged more than {LOG_BYTES} bytes in this call; the rest is dropped"
        ));
        self.log_full = true;
        Ok(())
    }
}

/// The linker every call uses: WASI 0.2 with the fixed answers of [`State::new`], and the plugin
/// world's imports.
pub(crate) fn linker(engine: &wasmtime::Engine) -> wasmtime::Result<Linker<State>> {
    let mut linker = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    Plugin::add_to_linker::<State, HasSelf<State>>(&mut linker, |state| state)?;
    Ok(linker)
}

//! The shared cache: verified downloads by sha256, outside the repository.
//!
//! One directory (`$GATE_CACHE_DIR`, by default the user cache directory) is shared by every
//! repository and worktree on the machine:
//!
//! ```text
//! blobs/sha256/<sha256>                       a verified download: a plugin, a core archive
//! core/<sha256>/gate                          the core unpacked by bin/gate from that archive
//! modules/                                    compiled plugins: wasmtime's own cache (the plugin runtime)
//! results/<plugin sha256>/                    the result cache, by plugin (the plugin runtime)
//! git/                                        answers about commits and file contents (the plugin runtime)
//! tmp/                                        downloads in progress
//! CACHEDIR.TAG                                marks the directory as a cache (bford.info/cachedir)
//! ```
//!
//! An entry is written to `tmp/` and renamed into place, so a reader never sees a partial file and
//! two processes filling the same entry both succeed with one entry. A filled entry needs no
//! network, and is hashed again each time it is read: an entry that is not a regular file, or no
//! longer hashes to its name, is removed and downloaded again.
//!
//! Downloads (`blobs/sha256/`, `core/`, `tools/`) are pinned: `gate cache prune` keeps one exactly
//! while a lock names it. Everything under `results/` and `git/` is derived: a run that finds it
//! missing computes it again, so `gate cache prune` removes it when nothing can use it, and by its
//! last use, the file's modification time, which a run sets each time it uses the file.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest as _, Sha256};

use crate::error::{Error, Result};

pub(crate) mod answers;
pub(crate) mod results;

/// The variable that names the cache directory.
pub const CACHE_DIR_VARIABLE: &str = "GATE_CACHE_DIR";

/// The file that marks a directory as a cache, so backup tools skip it and `gate cache prune`
/// knows it is gate's.
pub const CACHE_TAG: &str = "CACHEDIR.TAG";

const CACHE_TAG_CONTENTS: &str = "Signature: 8a477f597d28d172789f06886806bc55\n# This file is a cache directory tag created by gate.\n# For information about cache directory tags see https://bford.info/cachedir/\n";

/// How old a file in `tmp/` must be before `gate cache prune` removes it: older than any download.
const STALE_TEMPORARY: Duration = Duration::from_hours(24);

/// How old a tool's temporary directory (`tools/.<name>.<pid>.<attempt>`, being unpacked or set
/// aside by a run) must be before `gate cache prune` removes it: older than any unpacking.
const STALE_TOOL_TEMPORARY: Duration = Duration::from_hours(1);

/// How long a stored file content that no stored answer names is kept: a run stores the contents
/// of the files it reads before the answer file that names them.
const UNNAMED_GRACE: Duration = Duration::from_hours(24);

/// A sha256 checksum: 64 lowercase hexadecimal digits.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Sha256Sum(String);

impl Sha256Sum {
    /// Parses a checksum.
    ///
    /// # Errors
    /// Fails, saying what is expected, when `value` is not 64 lowercase hexadecimal digits.
    pub fn parse(value: &str) -> std::result::Result<Self, String> {
        if value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Ok(Self(value.to_owned()))
        } else {
            Err(format!(
                "{value:?} is not a sha256 checksum; expected 64 lowercase hexadecimal digits"
            ))
        }
    }

    /// The checksum of `bytes` as hexadecimal digits.
    #[must_use]
    pub fn from_digest(bytes: &[u8]) -> Self {
        Self(hex(bytes))
    }

    /// The hexadecimal digits.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256Sum {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Lowercase hexadecimal digits of `bytes`.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

/// The cache directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    /// The cache at `root`.
    #[must_use]
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The cache `$GATE_CACHE_DIR` names, else the user cache directory: `~/Library/Caches/gate`
    /// on macOS, `$XDG_CACHE_HOME/gate` or `~/.cache/gate` elsewhere. `bin/gate` finds the same.
    ///
    /// # Errors
    /// Fails when `$GATE_CACHE_DIR` is not an absolute path, or neither it nor `$HOME` is set.
    pub fn from_env() -> Result<Self> {
        let variable = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
        Self::from_variables(
            variable(CACHE_DIR_VARIABLE).map(PathBuf::from),
            variable("XDG_CACHE_HOME").map(PathBuf::from),
            variable("HOME").map(PathBuf::from),
        )
    }

    fn from_variables(
        explicit: Option<PathBuf>,
        xdg: Option<PathBuf>,
        home: Option<PathBuf>,
    ) -> Result<Self> {
        if let Some(root) = explicit {
            if root.is_absolute() {
                return Ok(Self::at(root));
            }
            return Err(Error::environment(format!(
                "{CACHE_DIR_VARIABLE}={} is not an absolute path; the cache is shared by every worktree, so name it from the root",
                root.display()
            )));
        }
        let missing = || {
            Error::environment(format!(
                "no cache directory: set {CACHE_DIR_VARIABLE} to an absolute path, or HOME"
            ))
        };
        if cfg!(target_os = "macos") {
            let home = home.ok_or_else(missing)?;
            return Ok(Self::at(home.join("Library").join("Caches").join("gate")));
        }
        match xdg.filter(|path| path.is_absolute()) {
            Some(xdg) => Ok(Self::at(xdg.join("gate"))),
            None => Ok(Self::at(
                home.ok_or_else(missing)?.join(".cache").join("gate"),
            )),
        }
    }

    /// The cache directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the verified download with this checksum lives.
    #[must_use]
    pub fn blob(&self, sha256: &Sha256Sum) -> PathBuf {
        self.root.join("blobs").join("sha256").join(sha256.as_str())
    }

    /// The verified download with this checksum, when the cache has it: a regular file (not a
    /// symbolic link) that still hashes to `sha256`. An entry that is anything else is removed, so
    /// the caller downloads it again.
    ///
    /// # Errors
    /// Fails when the entry cannot be read or a bad entry cannot be removed.
    pub fn get(&self, sha256: &Sha256Sum) -> Result<Option<PathBuf>> {
        let path = self.blob(sha256);
        let metadata = match self.held(sha256)? {
            Held::Missing => return Ok(None),
            Held::Verified => return Ok(Some(path)),
            Held::Changed(metadata) => metadata,
        };
        let removed = if metadata.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        match removed {
            Ok(()) => Ok(None),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Error::environment(format!(
                "{} is not the verified download it is named for, and cannot be removed: {error}; remove it by hand",
                path.display()
            ))),
        }
    }

    /// What the cache holds under this checksum, without changing anything: nothing, the verified
    /// download, or an entry that is not it (another file, a link, a directory).
    ///
    /// # Errors
    /// Fails when the entry cannot be read.
    pub fn held(&self, sha256: &Sha256Sum) -> Result<Held> {
        let path = self.blob(sha256);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Held::Missing),
            Err(error) => {
                return Err(Error::environment(format!(
                    "cannot read {}: {error}",
                    path.display()
                )));
            }
        };
        if metadata.file_type().is_file() && &hash_file(&path)? == sha256 {
            Ok(Held::Verified)
        } else {
            Ok(Held::Changed(metadata))
        }
    }

    /// Whether the directory carries [`CACHE_TAG`], which gate writes on first use.
    #[must_use]
    pub fn tagged(&self) -> bool {
        fs::symlink_metadata(self.root.join(CACHE_TAG))
            .is_ok_and(|metadata| metadata.file_type().is_file())
    }

    fn tag(&self) -> Result<()> {
        if self.tagged() {
            return Ok(());
        }
        let path = self.root.join(CACHE_TAG);
        fs::write(&path, CACHE_TAG_CONTENTS).map_err(|error| {
            Error::environment(format!(
                "cannot write {}: {error}; check that the cache directory is writable or set {CACHE_DIR_VARIABLE}",
                path.display()
            ))
        })
    }

    /// Where `bin/gate` unpacks the core archive with this checksum.
    #[must_use]
    pub fn core(&self, sha256: &Sha256Sum) -> PathBuf {
        self.root.join("core").join(sha256.as_str())
    }

    /// The directory the plugin runtime gives wasmtime's own cache (`Config::cache`) for compiled
    /// modules. Its contents belong to wasmtime; `gate cache prune` keeps or removes it as one unit.
    #[must_use]
    pub fn modules(&self) -> PathBuf {
        self.root.join("modules")
    }

    /// The directory of the result cache.
    #[must_use]
    pub fn results(&self) -> PathBuf {
        self.root.join("results")
    }

    /// The directory of what plugins declare, one directory per plugin sha256.
    #[must_use]
    pub fn declarations(&self) -> PathBuf {
        self.root.join("declarations")
    }

    /// The file of what the plugin with this sha256 declares, as the core whose entry name is
    /// `key` stores it, in [`Cache::declarations`].
    #[must_use]
    pub fn declaration(&self, plugin: &Sha256Sum, key: &str) -> PathBuf {
        self.declarations().join(plugin.as_str()).join(key)
    }

    /// Moves a written temporary file into place as the declaration of [`Cache::declaration`].
    /// When another process filled it first, this one replaces it with the same declaration.
    ///
    /// # Errors
    /// Fails when the file cannot be synced or renamed.
    pub fn insert_declaration(
        &self,
        temporary: Temporary,
        plugin: &Sha256Sum,
        key: &str,
    ) -> Result<PathBuf> {
        Self::place(temporary, self.declaration(plugin, key))
    }

    /// The directory of answers about commits.
    #[must_use]
    pub fn git(&self) -> PathBuf {
        self.root.join("git")
    }

    /// A new, empty file in `tmp/`, removed when dropped unless [`Cache::insert`] moves it.
    ///
    /// # Errors
    /// Fails when the directory or the file cannot be created.
    pub fn temporary(&self) -> Result<Temporary> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let directory = self.root.join("tmp");
        create_directory(&directory)?;
        self.tag()?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        loop {
            let name = format!(
                "{}-{nanos}-{}.part",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            let path = directory.join(name);
            match File::options().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok(Temporary { path, file }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    return Err(Error::environment(format!(
                        "cannot create {}: {error}; check that the cache directory is writable or set {CACHE_DIR_VARIABLE}",
                        path.display()
                    )));
                }
            }
        }
    }

    /// Moves a verified temporary file into place as the entry with this checksum. When another
    /// process filled the entry first, its file stays and this one is replaced by the same bytes.
    ///
    /// # Errors
    /// Fails when the file cannot be synced or renamed.
    pub fn insert(&self, temporary: Temporary, sha256: &Sha256Sum) -> Result<PathBuf> {
        let target = self.blob(sha256);
        Self::place(temporary, target)
    }

    /// The file of the answer of kind `kind` (such as `sync-state`) under `key`, in [`Cache::git`].
    #[must_use]
    pub fn answer(&self, kind: &str, key: &Sha256Sum) -> PathBuf {
        let digits = key.as_str();
        self.git().join(kind).join(&digits[..2]).join(digits)
    }

    /// The file of a stored call result of the plugin with sha256 `plugin` under `key`, in
    /// [`Cache::results`]: one directory per plugin, so the results go with the plugin.
    #[must_use]
    pub fn result(&self, plugin: &str, key: &Sha256Sum) -> PathBuf {
        let digits = key.as_str();
        self.results().join(plugin).join(&digits[..2]).join(digits)
    }

    /// Moves a written temporary file into place as the result of [`Cache::result`] for `plugin`
    /// and `key`. When another process filled it first, this one replaces it with an equally valid
    /// result.
    ///
    /// # Errors
    /// Fails when the file cannot be synced or renamed.
    pub fn insert_result(
        &self,
        temporary: Temporary,
        plugin: &str,
        key: &Sha256Sum,
    ) -> Result<PathBuf> {
        Self::place(temporary, self.result(plugin, key))
    }

    /// Moves a written temporary file into place as the answer of [`Cache::answer`] for `kind`
    /// and `key`. When another process filled it first, this one replaces it with the same answer.
    ///
    /// # Errors
    /// Fails when the file cannot be synced or renamed.
    pub fn insert_answer(
        &self,
        temporary: Temporary,
        kind: &str,
        key: &Sha256Sum,
    ) -> Result<PathBuf> {
        Self::place(temporary, self.answer(kind, key))
    }

    fn place(mut temporary: Temporary, target: PathBuf) -> Result<PathBuf> {
        if let Some(directory) = target.parent() {
            create_directory(directory)?;
        }
        temporary.file.sync_all().map_err(|error| {
            Error::environment(format!(
                "cannot write {}: {error}",
                temporary.path.display()
            ))
        })?;
        let mut tries = 0;
        while let Err(error) = fs::rename(&temporary.path, &target) {
            // `gate cache prune` removed the emptied directory meanwhile: create it again.
            if error.kind() == io::ErrorKind::NotFound && tries < 3 && temporary.path.exists() {
                tries += 1;
                if let Some(directory) = target.parent() {
                    create_directory(directory)?;
                }
                continue;
            }
            return Err(Error::environment(format!(
                "cannot move {} to {}: {error}",
                temporary.path.display(),
                target.display()
            )));
        }
        // Renamed: nothing is left for the drop to remove.
        temporary.path = PathBuf::new();
        Ok(target)
    }

    /// The entries no checksum in `referenced` names (downloads, cores, tools and plugin
    /// declarations), `modules/` as a whole when no plugin is
    /// pinned (`plugins_pinned` false: wasmtime's file names do not say which plugin a module
    /// came from), files in `tmp/` older than a day, and tool temporaries (`tools/.<name>.*`)
    /// older than an hour. No symbolic link is followed.
    ///
    /// # Errors
    /// Fails when the directory exists without [`CACHE_TAG`] (it may not be gate's cache), when a
    /// directory of the layout is a symbolic link, and when a directory cannot be read.
    pub fn unreferenced(
        &self,
        referenced: &BTreeSet<Sha256Sum>,
        plugins_pinned: bool,
    ) -> Result<Vec<Entry>> {
        let mut entries = Vec::new();
        if !self.directory(&self.root)? {
            return Ok(entries);
        }
        if !self.tagged() {
            return Err(Error::environment(format!(
                "{} has no {CACHE_TAG}, so it may not be gate's cache; nothing was pruned. Check {CACHE_DIR_VARIABLE}; gate writes the tag the first time it fills a cache",
                self.root.display()
            )));
        }
        let modules = self.modules();
        if !plugins_pinned && self.directory(&modules)? {
            entries.push(Entry::at(modules, Reason::Unpinned)?);
        }
        let blobs = self.root.join("blobs");
        for (parent, directory) in [
            (&blobs, blobs.join("sha256")),
            (&blobs, self.root.join("core")),
            (&self.root, self.declarations()),
        ] {
            if !self.directory(parent)? || !self.directory(&directory)? {
                continue;
            }
            for (path, name) in children(&directory)? {
                let kept = Sha256Sum::parse(&name).is_ok_and(|sum| referenced.contains(&sum));
                if !kept {
                    entries.push(Entry::at(path, Reason::Unpinned)?);
                }
            }
        }
        // tools/<sha256>/ (what was unpacked, and its record) goes with its download; tools/bin/
        // holds links only, checked and rebuilt on use, so it goes whenever a tool does.
        let now = SystemTime::now();
        let tools = self.root.join("tools");
        if self.directory(&tools)? {
            let mut removed = false;
            let mut bin = None;
            for (path, name) in children(&tools)? {
                if name == "bin" {
                    bin = Some(path);
                    continue;
                }
                if name.starts_with('.') {
                    // A run may still be unpacking it or setting it aside: only a stale one goes.
                    if age(&path, now) > STALE_TOOL_TEMPORARY {
                        entries.push(Entry::at(path, Reason::Stale)?);
                    }
                    continue;
                }
                if !Sha256Sum::parse(&name).is_ok_and(|sum| referenced.contains(&sum)) {
                    entries.push(Entry::at(path, Reason::Unpinned)?);
                    removed = true;
                }
            }
            if removed && let Some(bin) = bin {
                entries.push(Entry::at(bin, Reason::Unpinned)?);
            }
        }
        let temporary = self.root.join("tmp");
        let stale = if self.directory(&temporary)? {
            children(&temporary)?
        } else {
            Vec::new()
        };
        for (path, _) in stale {
            if age(&path, now) > STALE_TEMPORARY {
                entries.push(Entry::at(path, Reason::Stale)?);
            }
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(entries)
    }

    /// What `gate cache prune` removes: the entries of [`Cache::unreferenced`]; the results of a
    /// plugin `plugins` does not name, and any other entry of `results/` that is no plugin's; the
    /// stored file contents no stored answer names, stored more than a day ago (a run stores a
    /// content before the answer that names it); then, of the derived files left (under
    /// `results/` and `git/`), those last used longer ago than `bounds.older_than`, and, while the
    /// whole cache is over `bounds.max_size`, the least recently used, by path on a tie. A pinned
    /// download and `modules/` are never removed by a bound.
    ///
    /// # Errors
    /// As [`Cache::unreferenced`], and when a stored answer cannot be read: the contents it names
    /// are then unknown, and nothing is planned.
    pub fn plan(
        &self,
        referenced: &BTreeSet<Sha256Sum>,
        plugins: &BTreeSet<Sha256Sum>,
        bounds: &Bounds,
    ) -> Result<Plan> {
        let mut entries = self.unreferenced(referenced, !plugins.is_empty())?;
        if !self.directory(&self.root)? {
            return Ok(Plan::default());
        }
        let now = SystemTime::now();
        // The derived files a bound may remove, with their last use.
        let mut derived: Vec<Derived> = Vec::new();
        let results = self.results();
        if self.directory(&results)? {
            for (path, name) in children(&results)? {
                let reason = match Sha256Sum::parse(&name) {
                    Ok(plugin) if !plugins.contains(&plugin) => Reason::Unpinned,
                    Ok(_) if self.directory(&path)? => {
                        derived.extend(derived_files(&path)?);
                        continue;
                    }
                    _ => Reason::OldFormat,
                };
                entries.push(Entry::at(path, reason)?);
            }
        }
        let git = self.git();
        if self.directory(&git)? {
            let answered = answers::stored_blob_files(self)?;
            let blobs = git.join("blobs");
            for file in derived_files(&git)? {
                let unnamed = file.path.starts_with(&blobs) && !answered.contains(&file.path);
                if unnamed && now.duration_since(file.used).unwrap_or_default() > UNNAMED_GRACE {
                    entries.push(file.entry(Reason::Unnamed));
                } else {
                    derived.push(file);
                }
            }
        }
        if let Some(days) = bounds.older_than {
            let limit = Duration::from_secs(days.saturating_mul(24 * 60 * 60));
            derived.retain(|file| {
                let old = now.duration_since(file.used).unwrap_or_default() > limit;
                if old {
                    entries.push(file.entry(Reason::Older(days)));
                }
                !old
            });
        }
        let removed: u64 = entries.iter().map(|entry| entry.bytes).sum();
        let mut left = size(&self.root).saturating_sub(removed);
        let mut over = None;
        if let Some(max) = bounds.max_size {
            derived.sort_by(|first, second| {
                first
                    .used
                    .cmp(&second.used)
                    .then_with(|| first.path.cmp(&second.path))
            });
            let mut candidates = derived.into_iter();
            while left > max {
                let Some(file) = candidates.next() else {
                    over = Some(max);
                    break;
                };
                left = left.saturating_sub(file.bytes);
                entries.push(file.entry(Reason::OverSize(max)));
            }
        }
        entries.sort_by(|first, second| first.path.cmp(&second.path));
        Ok(Plan {
            entries,
            left,
            over,
        })
    }

    /// Whether `path` is a directory: `false` when it does not exist.
    fn directory(&self, path: &Path) -> Result<bool> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => Err(Error::environment(format!(
                "{} is a symbolic link; gate cache prune follows none, so nothing was pruned. Point {CACHE_DIR_VARIABLE} at the real directory, or remove the link",
                path.display()
            ))),
            Ok(metadata) if metadata.is_dir() => Ok(true),
            Ok(_) => Err(Error::environment(format!(
                "{} is not a directory, so {} is not laid out as gate's cache; nothing was pruned",
                path.display(),
                self.root.display()
            ))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(Error::environment(format!(
                "cannot read {}: {error}",
                path.display()
            ))),
        }
    }

    /// Removes the entries [`Cache::unreferenced`] returned.
    ///
    /// # Errors
    /// Fails on the first entry that cannot be removed, naming it.
    pub fn remove(&self, entries: &[Entry]) -> Result<()> {
        for entry in entries {
            if !entry.path.starts_with(&self.root) {
                return Err(Error::environment(format!(
                    "{} is outside the cache {}; nothing was removed past it",
                    entry.path.display(),
                    self.root.display()
                )));
            }
            let removed = if entry.directory {
                fs::remove_dir_all(&entry.path)
            } else {
                fs::remove_file(&entry.path)
            };
            match removed {
                Ok(()) if !entry.directory => remove_emptied(&entry.path),
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(Error::environment(format!(
                        "cannot remove {}: {error}",
                        entry.path.display()
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Removes the directories that removing `file` emptied: its `<2 hex>/` shard, and the results
/// directory of a plugin, `<plugin sha256>/`, above it. A directory a run filled meanwhile stays,
/// and a run that finds its shard gone creates it again ([`Cache::place`]).
fn remove_emptied(file: &Path) {
    let mut directory = file.parent();
    while let Some(path) = directory {
        let name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default();
        let shard = (name.len() == 2 && name.bytes().all(|byte| byte.is_ascii_hexdigit()))
            || Sha256Sum::parse(name).is_ok();
        if !shard || fs::remove_dir(path).is_err() {
            return;
        }
        directory = path.parent();
    }
}

/// Every file below `directory`, with its last use; no symbolic link is followed, and a link
/// counts as a file.
fn derived_files(directory: &Path) -> Result<Vec<Derived>> {
    let mut files = Vec::new();
    let mut directories = vec![directory.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for (path, _) in children(&directory)? {
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                // A run removed it meanwhile (an answer file it merged).
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(Error::environment(format!(
                        "cannot read {}: {error}",
                        path.display()
                    )));
                }
            };
            if metadata.is_dir() {
                directories.push(path);
                continue;
            }
            // Unreadable times count as now: the file is kept as recent.
            let used = metadata.modified().unwrap_or_else(|_| SystemTime::now());
            files.push(Derived {
                path,
                bytes: metadata.len(),
                used,
            });
        }
    }
    Ok(files)
}

/// How long ago `path` was last modified; zero when that cannot be read, so it counts as recent.
fn age(path: &Path, now: SystemTime) -> Duration {
    fs::symlink_metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_or(Duration::ZERO, |modified| {
            now.duration_since(modified).unwrap_or_default()
        })
}

/// What the cache holds under a checksum.
#[derive(Debug)]
pub enum Held {
    /// Nothing.
    Missing,
    /// A regular file that hashes to the checksum.
    Verified,
    /// Something else: another file, a link or a directory.
    Changed(fs::Metadata),
}

/// A cache entry `gate cache prune` would remove.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    /// The file or directory.
    pub path: PathBuf,
    /// Whether it is a directory.
    pub directory: bool,
    /// Its size in bytes, with everything below it.
    pub bytes: u64,
    /// Why it goes.
    pub reason: Reason,
}

/// Why `gate cache prune` removes an entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    /// No lock names the download, or the plugin of the results; `modules/` when no plugin is
    /// pinned.
    Unpinned,
    /// A temporary file or directory older than any download or unpacking.
    Stale,
    /// A stored file content that no stored answer names, stored more than a day ago.
    Unnamed,
    /// An entry under `results/` that no plugin's results are laid out as: the format-2 layout.
    OldFormat,
    /// A derived file last used more than this many days ago (`--older-than`).
    Older(u64),
    /// A derived file removed, least recently used first, to bring the cache within this many
    /// bytes (`--max-size`).
    OverSize(u64),
}

/// A derived file: under `results/` or `git/`, with its last use.
struct Derived {
    path: PathBuf,
    bytes: u64,
    used: SystemTime,
}

impl Derived {
    fn entry(&self, reason: Reason) -> Entry {
        Entry {
            path: self.path.clone(),
            directory: false,
            bytes: self.bytes,
            reason,
        }
    }
}

/// The bounds of `gate cache prune`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Bounds {
    /// `--older-than <days>`: derived files last used longer ago go.
    pub older_than: Option<u64>,
    /// `--max-size <bytes>`: derived files go, least recently used first, while the cache is
    /// larger.
    pub max_size: Option<u64>,
}

/// What [`Cache::plan`] removes, and what is left.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Plan {
    /// The entries to remove, sorted by path.
    pub entries: Vec<Entry>,
    /// The bytes the cache holds once they are removed.
    pub left: u64,
    /// `--max-size` when the cache stays over it with no derived file left: what is left is
    /// pinned downloads, `modules/` and files in use, which no bound removes.
    pub over: Option<u64>,
}

impl Entry {
    fn at(path: PathBuf, reason: Reason) -> Result<Self> {
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            Error::environment(format!("cannot read {}: {error}", path.display()))
        })?;
        let directory = metadata.is_dir();
        let bytes = if directory {
            size(&path)
        } else {
            metadata.len()
        };
        Ok(Self {
            path,
            directory,
            bytes,
            reason,
        })
    }
}

/// Sets the modification time of `path` to now: the file's last use, which `gate cache prune`
/// bounds. A file that cannot be touched keeps its time; the run goes on.
pub(crate) fn touch(path: &Path) {
    let _ignored = File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(SystemTime::now()));
}

/// For tests: sets the last use of `path` to `days` days ago.
#[cfg(test)]
pub(crate) fn used_days_ago(path: &Path, days: u64) {
    File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(SystemTime::now() - Duration::from_hours(24 * days)))
        .expect("set the last use");
}

/// For tests: what `gate cache prune --older-than 30` removes from `cache`, with no lock.
#[cfg(test)]
pub(crate) fn older_than_30(cache: &Cache) -> Vec<PathBuf> {
    let bounds = Bounds {
        older_than: Some(30),
        max_size: None,
    };
    cache
        .plan(&BTreeSet::new(), &BTreeSet::new(), &bounds)
        .expect("plan")
        .entries
        .into_iter()
        .map(|entry| entry.path)
        .collect()
}

/// A file being written in `tmp/`, removed when dropped unless it was inserted.
#[derive(Debug)]
pub struct Temporary {
    path: PathBuf,
    file: File,
}

impl Temporary {
    /// The file, for writing.
    pub fn file(&mut self) -> &mut File {
        &mut self.file
    }

    /// Where it is.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        // Only this process wrote the file; removing it leaves the cache as it was.
        if !self.path.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// The sha256 of the file at `path`.
///
/// # Errors
/// Fails when the file cannot be read.
pub fn hash_file(path: &Path) -> Result<Sha256Sum> {
    let failed =
        |error: io::Error| Error::environment(format!("cannot read {}: {error}", path.display()));
    let mut file = File::open(path).map_err(failed)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(failed)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(Sha256Sum::from_digest(&hasher.finalize()))
}

fn create_directory(directory: &Path) -> Result<()> {
    fs::create_dir_all(directory).map_err(|error| {
        Error::environment(format!(
            "cannot create {}: {error}; check that the cache directory is writable or set {CACHE_DIR_VARIABLE}",
            directory.display()
        ))
    })
}

fn children(directory: &Path) -> Result<Vec<(PathBuf, String)>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(Error::environment(format!(
                "cannot read {}: {error}",
                directory.display()
            )));
        }
    };
    let mut children = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            Error::environment(format!("cannot read {}: {error}", directory.display()))
        })?;
        children.push((
            entry.path(),
            entry.file_name().to_string_lossy().into_owned(),
        ));
    }
    Ok(children)
}

fn size(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !metadata.is_dir() {
        return metadata.len();
    }
    fs::read_dir(path).map_or(0, |entries| {
        entries
            .filter_map(std::result::Result::ok)
            .map(|entry| size(&entry.path()))
            .sum()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_directory_follows_the_platform() {
        let home = Some(PathBuf::from("/home/test"));
        let explicit = Cache::from_variables(Some(PathBuf::from("/srv/cache")), None, home.clone());
        assert_eq!(explicit.ok(), Some(Cache::at("/srv/cache")));

        let relative = Cache::from_variables(Some(PathBuf::from("cache")), None, home.clone());
        assert!(relative.is_err());

        let default = Cache::from_variables(None, Some(PathBuf::from("/xdg")), home);
        let expected = if cfg!(target_os = "macos") {
            "/home/test/Library/Caches/gate"
        } else {
            "/xdg/gate"
        };
        assert_eq!(default.ok(), Some(Cache::at(expected)));
        assert!(Cache::from_variables(None, None, None).is_err());
    }

    #[test]
    fn prune_keeps_the_declarations_of_pinned_plugins_only() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let cache = Cache::at(directory.path());
        std::fs::write(directory.path().join(CACHE_TAG), CACHE_TAG_CONTENTS).expect("tag");
        let pinned = Sha256Sum::parse(&"b".repeat(64)).expect("checksum");
        let unpinned = Sha256Sum::parse(&"d".repeat(64)).expect("checksum");
        for plugin in [&pinned, &unpinned] {
            let path = cache.declaration(plugin, "entry");
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directories");
            std::fs::write(path, "{}").expect("entry");
        }

        let entries = cache
            .unreferenced(&BTreeSet::from([pinned]), true)
            .expect("listed");
        let paths: Vec<&Path> = entries.iter().map(|entry| entry.path.as_path()).collect();
        assert_eq!(
            paths,
            [cache.declarations().join(unpinned.as_str()).as_path()]
        );
    }

    #[test]
    fn checksums_are_64_lowercase_hex_digits() {
        assert!(Sha256Sum::parse(&"a".repeat(64)).is_ok());
        assert!(Sha256Sum::parse(&"A".repeat(64)).is_err());
        assert!(Sha256Sum::parse(&"a".repeat(63)).is_err());
        assert_eq!(hex(&[0x00, 0xab, 0xff]), "00abff");
    }
}

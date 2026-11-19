//! The shared cache: verified downloads by sha256, outside the repository.
//!
//! One directory (`$GATE_CACHE_DIR`, by default the user cache directory) is shared by every
//! repository and worktree on the machine:
//!
//! ```text
//! blobs/sha256/<sha256>                       a verified download: a plugin, a core archive
//! core/<sha256>/gate                          the core unpacked by bin/gate from that archive
//! modules/                                    compiled plugins: wasmtime's own cache (the plugin runtime)
//! results/                                    the result cache (the plugin runtime)
//! git/                                        answers about commits (the plugin runtime)
//! tmp/                                        downloads in progress
//! CACHEDIR.TAG                                marks the directory as a cache (bford.info/cachedir)
//! ```
//!
//! An entry is written to `tmp/` and renamed into place, so a reader never sees a partial file and
//! two processes filling the same entry both succeed with one entry. A filled entry needs no
//! network, and is hashed again each time it is read: an entry that is not a regular file, or no
//! longer hashes to its name, is removed and downloaded again.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest as _, Sha256};

use crate::error::{Error, Result};

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
        fs::rename(&temporary.path, &target).map_err(|error| {
            Error::environment(format!(
                "cannot move {} to {}: {error}",
                temporary.path.display(),
                target.display()
            ))
        })?;
        // Renamed: nothing is left for the drop to remove.
        temporary.path = PathBuf::new();
        Ok(target)
    }

    /// The entries no checksum in `referenced` names, `modules/` as a whole when no plugin is
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
            entries.push(Entry::at(modules)?);
        }
        let blobs = self.root.join("blobs");
        for directory in [blobs.join("sha256"), self.root.join("core")] {
            if !self.directory(&blobs)? || !self.directory(&directory)? {
                continue;
            }
            for (path, name) in children(&directory)? {
                let kept = Sha256Sum::parse(&name).is_ok_and(|sum| referenced.contains(&sum));
                if !kept {
                    entries.push(Entry::at(path)?);
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
                        entries.push(Entry::at(path)?);
                    }
                    continue;
                }
                if !Sha256Sum::parse(&name).is_ok_and(|sum| referenced.contains(&sum)) {
                    entries.push(Entry::at(path)?);
                    removed = true;
                }
            }
            if removed && let Some(bin) = bin {
                entries.push(Entry::at(bin)?);
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
                entries.push(Entry::at(path)?);
            }
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(entries)
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
}

impl Entry {
    fn at(path: PathBuf) -> Result<Self> {
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
        })
    }
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
    fn checksums_are_64_lowercase_hex_digits() {
        assert!(Sha256Sum::parse(&"a".repeat(64)).is_ok());
        assert!(Sha256Sum::parse(&"A".repeat(64)).is_err());
        assert!(Sha256Sum::parse(&"a".repeat(63)).is_err());
        assert_eq!(hex(&[0x00, 0xab, 0xff]), "00abff");
    }
}

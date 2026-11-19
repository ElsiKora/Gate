//! `[tools]`: external commands pinned by sha256 for each platform, downloaded once into the
//! shared cache through the verified fetch (the same path as plugins and the core), unpacked,
//! and put first on the `PATH` of the steps that name them in `tools`.
//!
//! Layout in the cache: the download is `blobs/sha256/<sha256>`; it is unpacked into
//! `tools/<sha256>/files/`, and `tools/<sha256>/executable` records the executable's path inside
//! `files/` and its sha256. Before each run the executable must still be a regular file with that
//! sha256, as `bin/gate` checks the cached core. `tools/bin/<key>/` holds one link per tool,
//! checked the same way.
//!
//! Every directory is built complete, record included, under a temporary name beside its place
//! and renamed into it, so a concurrent run sees either nothing or a whole entry. A directory in
//! place is never removed there: one that does not hold is first renamed aside, then the new one
//! takes its place, and only the one set aside is removed.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::cache::{Cache, Sha256Sum, hash_file};
use crate::config::{Name, Tool, ToolTarget, is_archive};
use crate::error::{Error, Result};
use crate::fetch::Fetcher;
use crate::lock::Pin;

/// The target triples this machine runs, in the order a tool's targets are tried.
#[must_use]
pub fn platforms() -> Vec<String> {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "macos" => vec![format!("{arch}-apple-darwin")],
        "linux" => vec![
            format!("{arch}-unknown-linux-gnu"),
            format!("{arch}-unknown-linux-musl"),
        ],
        "windows" => vec![format!("{arch}-pc-windows-msvc")],
        os => vec![format!("{arch}-{os}")],
    }
}

/// Where the tools live in `cache`.
#[must_use]
pub fn directory(cache: &Cache) -> PathBuf {
    cache.root().join("tools")
}

/// The tools of a run, installed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Installed {
    /// The directory that holds one entry per tool, named after the tool: the step's `PATH` starts
    /// with it.
    pub bin: PathBuf,
    /// Each tool and its executable.
    pub tools: BTreeMap<String, PathBuf>,
}

/// Fetches, verifies and unpacks every tool for this machine, reusing what the cache holds.
/// `None` when there is no tool.
///
/// # Errors
/// Fails when a tool has no target for this machine, a download fails or does not match its
/// sha256 (nothing enters the cache then), an archive cannot be unpacked or does not hold the
/// named executable.
pub fn install(tools: &BTreeMap<Name, Tool>, fetcher: &Fetcher) -> Result<Option<Installed>> {
    install_for(tools, fetcher, &platforms())
}

/// [`install`] for the given target triples.
///
/// # Errors
/// As [`install`].
pub fn install_for(
    tools: &BTreeMap<Name, Tool>,
    fetcher: &Fetcher,
    platforms: &[String],
) -> Result<Option<Installed>> {
    if tools.is_empty() {
        return Ok(None);
    }
    let cache = fetcher.cache();

    let mut installed = BTreeMap::new();
    let mut key = String::new();
    for (name, tool) in tools {
        let Some((platform, target)) = platforms
            .iter()
            .find_map(|platform| tool.targets.get(platform).map(|target| (platform, target)))
        else {
            return Err(Error::environment(format!(
                "gate.toml: [tools.{name}] has no target for this machine ({}); add [tools.{name}.targets.\"{}\"]",
                platforms.join(" or "),
                platforms.first().map_or("", String::as_str)
            )));
        };
        let context = format!("[tools.{name}] {platform}");
        let (executable, sha256) = unpack(name.as_str(), tool, target, fetcher)
            .map_err(|error| prefix(&error, &context))?;
        let _ = writeln!(key, "{name}={}={sha256}", executable.display());
        installed.insert(name.to_string(), executable);
    }

    let bin = directory(cache)
        .join("bin")
        .join(Sha256Sum::from_digest(&Sha256::digest(key.as_bytes())).as_str());
    if !links_hold(&bin, &installed) {
        let temporary = temporary_sibling(&bin)?;
        for (name, executable) in &installed {
            link(executable, &temporary.join(name))?;
        }
        settle(&temporary, &bin, |bin| links_hold(bin, &installed))?;
    }

    Ok(Some(Installed {
        bin,
        tools: installed,
    }))
}

/// Whether `bin` holds exactly one link per tool, each to its executable.
fn links_hold(bin: &Path, installed: &BTreeMap<String, PathBuf>) -> bool {
    let Ok(entries) = fs::read_dir(bin) else {
        return false;
    };
    let count = entries.count();
    count == installed.len()
        && installed
            .iter()
            .all(|(name, executable)| link_holds(&bin.join(name), executable))
}

#[cfg(unix)]
fn link_holds(entry: &Path, executable: &Path) -> bool {
    fs::read_link(entry).is_ok_and(|target| target == executable)
}

#[cfg(not(unix))]
fn link_holds(entry: &Path, executable: &Path) -> bool {
    match (hash_file(entry), hash_file(executable)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn prefix(error: &Error, context: &str) -> Error {
    let message = format!("gate.toml: {context}: {}", error.message());
    match error.kind() {
        crate::error::ErrorKind::Config => Error::config(message),
        crate::error::ErrorKind::Environment => Error::environment(message),
    }
}

/// The record inside a tool's directory: the executable's path inside `files/`, and its sha256.
const RECORD: &str = "executable";

/// The directory inside a tool's directory that holds what was unpacked.
const FILES: &str = "files";

/// The tool's executable in `tools/<sha256>/files/` and its own sha256: checked against the
/// record when it is unpacked already, else unpacked from the verified download, recorded, and
/// put in place.
fn unpack(
    name: &str,
    tool: &Tool,
    target: &ToolTarget,
    fetcher: &Fetcher,
) -> Result<(PathBuf, Sha256Sum)> {
    let pin = Pin {
        url: target.url.clone(),
        sha256: Sha256Sum::parse(&target.sha256).map_err(Error::config)?,
    };
    let directory = directory(fetcher.cache()).join(pin.sha256.as_str());
    let relative = target.bin.clone().unwrap_or_else(|| name.to_owned());
    let executable = directory.join(FILES).join(&relative);

    if let Some(sha256) = recorded(&directory, &relative) {
        return Ok((executable, sha256));
    }

    let blob = fetcher.fetch_pinned_by(
        &format!("the tool {name} {}", tool.version),
        &pin,
        &format!("gate.toml [tools.{name}]"),
    )?;
    let temporary = temporary_sibling(&directory)?;
    let built = build(&blob, tool, target, &relative, &temporary);
    let sha256 = match built {
        Ok(sha256) => sha256,
        Err(error) => {
            let _ = fs::remove_dir_all(&temporary);
            return Err(error);
        }
    };
    settle(&temporary, &directory, |directory| {
        recorded(directory, &relative).is_some()
    })?;
    Ok((executable, sha256))
}

/// Unpacks or copies the verified download into `temporary/files/`, checks that it holds the
/// executable, and writes the record beside it. Returns the executable's sha256.
fn build(
    blob: &Path,
    tool: &Tool,
    target: &ToolTarget,
    relative: &str,
    temporary: &Path,
) -> Result<Sha256Sum> {
    let files = temporary.join(FILES);
    fs::create_dir(&files)
        .map_err(|error| Error::environment(format!("{}: {error}", files.display())))?;
    let inside = files.join(relative);
    if is_archive(&target.url) {
        extract(blob, &target.url, &files)?;
    } else {
        fs::copy(blob, &inside)
            .map_err(|error| Error::environment(format!("{}: {error}", inside.display())))?;
        make_executable(&inside)?;
    }
    if !fs::symlink_metadata(&inside).is_ok_and(|metadata| metadata.file_type().is_file()) {
        return Err(Error::config(format!(
            "version {}: {} holds no regular file {relative}; set bin to the executable's path inside the archive",
            tool.version, target.url
        )));
    }
    let sha256 = hash_file(&inside)?;
    let record = temporary.join(RECORD);
    fs::write(&record, format!("{relative}\n{sha256}\n"))
        .map_err(|error| Error::environment(format!("{}: {error}", record.display())))?;
    Ok(sha256)
}

/// The sha256 the record in `directory` gives, when it names `relative` and the executable there
/// is still a regular file that has it.
fn recorded(directory: &Path, relative: &str) -> Option<Sha256Sum> {
    let text = fs::read_to_string(directory.join(RECORD)).ok()?;
    let mut lines = text.lines();
    if lines.next()? != relative {
        return None;
    }
    let sha256 = Sha256Sum::parse(lines.next()?).ok()?;
    let executable = directory.join(FILES).join(relative);
    let metadata = fs::symlink_metadata(&executable).ok()?;
    (metadata.file_type().is_file() && hash_file(&executable).ok()? == sha256).then_some(sha256)
}

/// Unpacks the archive `blob` (downloaded from `url`) into `into`.
fn extract(blob: &Path, url: &str, into: &Path) -> Result<()> {
    let zip = Path::new(url)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"));
    let mut command = if zip {
        let mut command = Command::new("unzip");
        command.arg("-q").arg(blob).arg("-d").arg(into);
        command
    } else {
        let mut command = Command::new("tar");
        command.arg("-xf").arg(blob).arg("-C").arg(into);
        command
    };
    let output = command.output().map_err(|error| {
        Error::environment(format!("{url}: cannot start the unpacker: {error}"))
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(Error::environment(format!(
            "{url}: cannot unpack: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// A new, empty directory beside `path`, for a result that is then renamed into place.
fn temporary_sibling(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().unwrap_or(path);
    fs::create_dir_all(parent)
        .map_err(|error| Error::environment(format!("{}: {error}", parent.display())))?;
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    for attempt in 0..1000_u32 {
        let candidate = parent.join(format!(".{file_name}.{}.{attempt}", std::process::id()));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(Error::environment(format!(
                    "{}: {error}",
                    candidate.display()
                )));
            }
        }
    }
    Err(Error::environment(format!(
        "{}: cannot create a temporary directory beside it",
        path.display()
    )))
}

/// Puts the complete directory `temporary` at `path`. When `path` is taken already, a directory
/// that `holds` (another run got there first) is kept and `temporary` dropped; one that does not
/// is renamed aside, `temporary` takes its place, and the one set aside is removed. A directory is
/// never removed while it is at `path`.
fn settle(temporary: &Path, path: &Path, holds: impl Fn(&Path) -> bool) -> Result<()> {
    let failed = |error: std::io::Error| {
        let _ = fs::remove_dir_all(temporary);
        Error::environment(format!("{}: {error}", path.display()))
    };
    for _ in 0..ATTEMPTS {
        match fs::rename(temporary, path) {
            Ok(()) => return Ok(()),
            Err(_) if path.is_dir() => {}
            Err(error) => return Err(failed(error)),
        }
        if holds(path) {
            let _ = fs::remove_dir_all(temporary);
            return Ok(());
        }
        let aside = temporary_sibling(path)?;
        match fs::rename(path, &aside) {
            Ok(()) => {}
            // Another run set it aside first: try the rename again.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                let _ = fs::remove_dir_all(&aside);
                return Err(failed(error));
            }
        }
        let _ = fs::remove_dir_all(&aside);
    }
    let _ = fs::remove_dir_all(temporary);
    Err(Error::environment(format!(
        "{}: other runs kept replacing it; run again",
        path.display()
    )))
}

/// How often [`settle`] tries to put a directory in place while other runs replace it.
const ATTEMPTS: usize = 16;

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|error| Error::environment(format!("{}: {error}", path.display())))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn link(executable: &Path, entry: &Path) -> Result<()> {
    std::os::unix::fs::symlink(executable, entry)
        .map_err(|error| Error::environment(format!("{}: {error}", entry.display())))
}

#[cfg(not(unix))]
fn link(executable: &Path, entry: &Path) -> Result<()> {
    fs::copy(executable, entry)
        .map(|_| ())
        .map_err(|error| Error::environment(format!("{}: {error}", entry.display())))
}

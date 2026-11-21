//! `gate.lock`: the URL and sha256 of the core per target and of every plugin.
//!
//! Written by `gate lock`, committed, never edited by hand. The format is TOML with one fixed
//! layout, so `bin/gate` reads its core pins with `awk`:
//!
//! ```toml
//! [core]
//! version = "0.2.1"
//!
//! [core.targets.aarch64-apple-darwin]
//! url = "https://github.com/ElsiKora/Gate/releases/download/v0.2.1/gate-aarch64-apple-darwin.tar.gz"
//! sha256 = "<64 hexadecimal digits>"
//!
//! [plugins."ElsiKora/Gate:git@0.2.1"]
//! url = "https://github.com/ElsiKora/Gate/releases/download/plugin-git-v0.2.1/git.wasm"
//! sha256 = "<64 hexadecimal digits>"
//! ```
//!
//! Plugins are keyed by their source, so two projects may pin two versions of one plugin.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::cache::Sha256Sum;
use crate::config::{PluginName, PluginSource, SettingOrigin};
use crate::error::{Error, ErrorKind, Result};
use crate::fetch::{self, Fetcher};
use crate::git::Git;
use crate::workspace::Workspace;

/// The lock file, at the repository root.
pub const LOCK_FILE: &str = "gate.lock";

/// The targets the core is released for, as Rust target triples.
pub const TARGETS: [&str; 2] = ["aarch64-apple-darwin", "x86_64-unknown-linux-musl"];

/// The repository whose releases carry the core.
pub const CORE_REPOSITORY: (&str, &str) = ("ElsiKora", "Gate");

const HEADER: &str = "# Written by `gate lock`. Commit it; never edit it by hand.\n";

/// The file in the worktree's git directory whose advisory lock a run holds while it reads,
/// changes and writes `gate.lock`.
pub const GUARD_FILE: &str = "gate.lock.guard";

/// The file in the worktree's git directory whose advisory lock a run holds while it reads,
/// changes and writes `gate/baseline.json`.
pub const BASELINE_GUARD_FILE: &str = "gate.baseline.guard";

/// Held by a run from reading `gate.lock` to writing it: an advisory lock on [`GUARD_FILE`] in the
/// worktree's git directory, so that a second run waits until the first has written, and reads
/// what it wrote; [`Guard::take_baseline`] does the same for `gate/baseline.json` on
/// [`BASELINE_GUARD_FILE`]. The operating system releases the lock when the run ends, however it
/// ends.
#[derive(Debug)]
pub struct Guard {
    _file: File,
}

impl Guard {
    /// Takes the guard of the worktree `git` is in. When another run holds it, says so on stderr
    /// and waits for it.
    ///
    /// # Errors
    /// Fails when the git directory cannot be found or the lock cannot be taken, saying what to do.
    pub fn take(git: &Git) -> Result<Self> {
        Self::take_for(git, GUARD_FILE, LOCK_FILE)
    }

    /// Takes the guard of `gate/baseline.json` in the worktree `git` is in, which a run holds from
    /// reading the baseline to writing it. When another run holds it, says so on stderr and waits
    /// for it.
    ///
    /// # Errors
    /// As [`Guard::take`].
    pub fn take_baseline(git: &Git) -> Result<Self> {
        Self::take_for(git, BASELINE_GUARD_FILE, crate::baseline::BASELINE_FILE)
    }

    fn take_for(git: &Git, file: &str, guarded: &str) -> Result<Self> {
        let directory = git.git_dir()?;
        Self::take_at(&directory.join(file), &directory, guarded)
    }

    fn take_at(path: &Path, directory: &Path, guarded: &str) -> Result<Self> {
        let refused = |error: std::io::Error| {
            Error::environment(format!(
                "cannot lock {} to update {guarded}: {error}; check that {} is writable and on a local file system",
                path.display(),
                directory.display()
            ))
        };
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(refused)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                eprintln!(
                    "gate: waiting for another gate run to finish writing {guarded} ({})",
                    path.display()
                );
                file.lock().map_err(refused)?;
            }
            Err(TryLockError::Error(error)) => return Err(refused(error)),
        }
        Ok(Self { _file: file })
    }
}

/// One pinned download.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pin {
    /// Where it is downloaded from: an `https` URL (plain `http` only to this machine).
    pub url: String,
    /// What it must hash to.
    pub sha256: Sha256Sum,
}

/// The pinned core: a version and one archive per target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorePin {
    /// The version, without `v`.
    pub version: String,
    /// The archive `gate-<target>.tar.gz` of each target.
    pub targets: BTreeMap<String, Pin>,
}

/// The contents of `gate.lock`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Lock {
    /// The core, when pinned.
    pub core: Option<CorePin>,
    /// Every plugin, by source.
    pub plugins: BTreeMap<PluginSource, Pin>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LockFile {
    core: Option<CoreFile>,
    #[serde(default)]
    plugins: BTreeMap<String, PinFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CoreFile {
    version: String,
    #[serde(default)]
    targets: BTreeMap<String, PinFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinFile {
    url: String,
    sha256: String,
}

impl Lock {
    /// Parses the text of a lock file. `path` names it in errors.
    ///
    /// # Errors
    /// Fails, naming the entry and what is expected, when the text is not a lock file.
    pub fn parse(path: &str, text: &str) -> Result<Self> {
        let wrong = |message: String| {
            Error::config(format!(
                "{path}: {message}; the file is written by gate lock, run it again instead of editing the file"
            ))
        };
        let file: LockFile = toml::from_str(text).map_err(|error| {
            wrong(
                error
                    .message()
                    .trim_end_matches(['\n', '.'])
                    .replace('\n', " "),
            )
        })?;
        let pin = |entry: &str, raw: PinFile| -> Result<Pin> {
            fetch::check_url(&raw.url).map_err(|message| wrong(format!("{entry}: {message}")))?;
            let sha256 = Sha256Sum::parse(&raw.sha256)
                .map_err(|message| wrong(format!("{entry}: {message}")))?;
            Ok(Pin {
                url: raw.url,
                sha256,
            })
        };

        let core = match file.core {
            None => None,
            Some(core) => {
                if !crate::config::is_version(&core.version) {
                    return Err(wrong(format!(
                        "[core] version {:?} is not a version",
                        core.version
                    )));
                }
                let mut targets = BTreeMap::new();
                for (target, raw) in core.targets {
                    if !TARGETS.contains(&target.as_str()) {
                        return Err(wrong(format!(
                            "[core.targets.{target}] is not a released target; expected one of {}",
                            TARGETS.join(", ")
                        )));
                    }
                    let entry = format!("[core.targets.{target}]");
                    targets.insert(target, pin(&entry, raw)?);
                }
                Some(CorePin {
                    version: core.version,
                    targets,
                })
            }
        };

        let mut plugins = BTreeMap::new();
        for (source, raw) in file.plugins {
            let parsed = PluginSource::parse(&source)
                .map_err(|message| wrong(format!("[plugins.{source:?}]: {message}")))?;
            let entry = format!("[plugins.{source:?}]");
            plugins.insert(parsed, pin(&entry, raw)?);
        }

        Ok(Self { core, plugins })
    }

    /// The text of the lock file, in its one layout.
    #[must_use]
    pub fn render(&self) -> String {
        let mut text = String::from(HEADER);
        if let Some(core) = &self.core {
            let _ = write!(text, "\n[core]\nversion = \"{}\"\n", core.version);
            for (target, pin) in &core.targets {
                let _ = write!(
                    text,
                    "\n[core.targets.{target}]\nurl = \"{}\"\nsha256 = \"{}\"\n",
                    pin.url, pin.sha256
                );
            }
        }
        for (source, pin) in &self.plugins {
            let _ = write!(
                text,
                "\n[plugins.\"{source}\"]\nurl = \"{}\"\nsha256 = \"{}\"\n",
                pin.url, pin.sha256
            );
        }
        text
    }

    /// Reads `gate.lock` from the working tree at `root`.
    ///
    /// # Errors
    /// Fails when the file cannot be read or is not a lock file.
    pub fn read(root: &Path) -> Result<Option<Self>> {
        let path = root.join(LOCK_FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(LOCK_FILE, &text).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Error::environment(format!(
                "cannot read {}: {error}",
                path.display()
            ))),
        }
    }

    /// Writes `gate.lock` into the working tree at `root`, through a synced temporary file and a
    /// rename, so a reader sees the old lock or the new one. A run that read the lock to change it
    /// holds the [`Guard`] from that read to this write.
    ///
    /// # Errors
    /// Fails when the file cannot be written; the old lock is then left as it was.
    pub fn write(&self, root: &Path) -> Result<()> {
        crate::files::replace(&root.join(LOCK_FILE), self.render().as_bytes())
    }

    /// Every checksum the lock pins.
    #[must_use]
    pub fn checksums(&self) -> BTreeSet<Sha256Sum> {
        self.core
            .iter()
            .flat_map(|core| core.targets.values())
            .chain(self.plugins.values())
            .map(|pin| pin.sha256.clone())
            .collect()
    }

    /// The checksums of the plugins the lock pins.
    #[must_use]
    pub fn plugin_checksums(&self) -> BTreeSet<Sha256Sum> {
        self.plugins
            .values()
            .map(|pin| pin.sha256.clone())
            .collect()
    }

    /// What differs between the lock and what the configuration wants, one line each.
    #[must_use]
    pub fn mismatches(&self, wanted: &Wanted) -> Vec<String> {
        let mut lines = Vec::new();
        match &self.core {
            None => lines.push(format!(
                "it pins no core, and gate.toml asks for {} ([gate] version)",
                wanted.core
            )),
            Some(core) if core.version != wanted.core => lines.push(format!(
                "it pins core {}, and gate.toml asks for {} ([gate] version)",
                core.version, wanted.core
            )),
            Some(core) => {
                for target in TARGETS {
                    if !core.targets.contains_key(target) {
                        lines.push(format!("it has no core archive for {target}"));
                    }
                }
            }
        }
        for (source, declarations) in &wanted.plugins {
            if !self.plugins.contains_key(source) {
                let first = &declarations[0];
                lines.push(format!(
                    "it does not pin plugin {} = {source} ({})",
                    first.name, first.origin
                ));
            }
        }
        for source in self.plugins.keys() {
            if !wanted.plugins.contains_key(source) {
                lines.push(format!("it pins {source}, which no gate.toml declares"));
            }
        }
        lines
    }
}

/// What the locks of one or more repositories pin, for `gate cache prune`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Pinned {
    /// Every checksum: core archives and plugins.
    pub checksums: BTreeSet<Sha256Sum>,
    /// The plugins' checksums alone: compiled modules are kept while any is pinned.
    pub plugins: BTreeSet<Sha256Sum>,
    /// The checksums of the `[tools]` of each worktree's root `gate.toml`: their downloads and
    /// unpacked directories are kept.
    pub tools: BTreeSet<Sha256Sum>,
    /// The lock files read.
    pub files: Vec<PathBuf>,
}

/// Adds what the `gate.lock` of each worktree of the repository at `path` pins to `pinned`. A
/// worktree without a lock pins nothing.
///
/// # Errors
/// Fails when `path` is not in a git repository, or a lock file cannot be read or parsed: with an
/// unreadable lock, nothing is safe to prune.
pub fn pinned_in(path: &Path, pinned: &mut Pinned) -> Result<()> {
    let git = Git::discover(path)?;
    let listing = git.output(&["worktree", "list", "--porcelain"])?;
    let mut roots: Vec<PathBuf> = String::from_utf8_lossy(&listing)
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect();
    if roots.is_empty() {
        roots.push(git.root().to_path_buf());
    }
    for root in roots {
        if let Some(lock) = Lock::read(&root).map_err(|error| {
            Error::environment(format!(
                "{}: {}; nothing was pruned",
                root.join(LOCK_FILE).display(),
                error.message()
            ))
        })? {
            pinned.checksums.extend(lock.checksums());
            pinned.plugins.extend(lock.plugin_checksums());
            pinned.files.push(root.join(LOCK_FILE));
        }
        let tools = tool_checksums(&root.join("gate.toml"))?;
        pinned.checksums.extend(tools.iter().cloned());
        pinned.tools.extend(tools);
    }
    Ok(())
}

/// The sha256 of every `[tools.<name>.targets.<target>]` of a `gate.toml`; none when the file
/// does not exist.
fn tool_checksums(path: &Path) -> Result<BTreeSet<Sha256Sum>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => {
            return Err(Error::environment(format!(
                "cannot read {}: {error}; nothing was pruned",
                path.display()
            )));
        }
    };
    let table: toml::Table = text.parse().map_err(|error| {
        Error::environment(format!(
            "{} is not TOML ({error}), so its [tools] are unknown; nothing was pruned",
            path.display()
        ))
    })?;
    Ok(table
        .get("tools")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|tools| tools.values())
        .filter_map(|tool| tool.get("targets").and_then(toml::Value::as_table))
        .flat_map(|targets| targets.values())
        .filter_map(|target| target.get("sha256").and_then(toml::Value::as_str))
        .filter_map(|sha256| Sha256Sum::parse(sha256).ok())
        .collect())
}

/// One place a plugin source is declared.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Declaration {
    /// The name `[plugins]` gives it.
    pub name: PluginName,
    /// Where.
    pub origin: SettingOrigin,
}

/// What the configuration needs pinned: the core version and every declared plugin source.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Wanted {
    /// `[gate] version`.
    pub core: String,
    /// Every declared source, with where it is declared.
    pub plugins: BTreeMap<PluginSource, Vec<Declaration>>,
}

impl Wanted {
    /// What the configuration of an open workspace needs pinned.
    #[must_use]
    pub fn of(workspace: &Workspace) -> Self {
        let core = workspace
            .root_config()
            .file()
            .gate
            .as_ref()
            .map(|gate| gate.version.as_str().to_owned())
            .unwrap_or_default();
        let mut plugins: BTreeMap<PluginSource, Vec<Declaration>> = BTreeMap::new();
        let scopes = std::iter::once(workspace.repository()).chain(
            workspace
                .projects()
                .iter()
                .filter_map(|project| workspace.settings(&project.name)),
        );
        for settings in scopes {
            for (name, plugin) in &settings.plugins {
                let declaration = Declaration {
                    name: name.clone(),
                    origin: plugin.origin.clone(),
                };
                let entry = plugins.entry(plugin.value.source.clone()).or_default();
                if !entry.contains(&declaration) {
                    entry.push(declaration);
                }
            }
        }
        for declarations in plugins.values_mut() {
            declarations.sort_by(|left, right| left.origin.cmp(&right.origin));
        }
        Self { core, plugins }
    }

    /// The core version alone, for a configuration with no plugins yet.
    #[must_use]
    pub fn core(version: &str) -> Self {
        Self {
            core: version.to_owned(),
            plugins: BTreeMap::new(),
        }
    }

    /// The one source declared under `name`.
    ///
    /// # Errors
    /// Fails when no file declares `name`, or files declare it with different sources.
    pub fn source_of(&self, name: &PluginName) -> Result<&PluginSource> {
        let sources: Vec<&PluginSource> = self
            .plugins
            .iter()
            .filter(|(_, declarations)| declarations.iter().any(|item| &item.name == name))
            .map(|(source, _)| source)
            .collect();
        match sources.as_slice() {
            [source] => Ok(source),
            [] => Err(Error::config(format!(
                "no gate.toml declares plugin {name}; declare it in [plugins] before pinning it"
            ))),
            several => Err(Error::config(format!(
                "plugin {name} is declared with {} sources ({}); --set pins one source, so give every declaration the same version first",
                several.len(),
                several
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }
}

/// Checks that the `gate.lock` of the workspace's snapshot matches its configuration.
///
/// A configuration that declares no plugin needs no lock to run: the core that runs is already
/// here. With a lock, the lock must match.
///
/// # Errors
/// Fails with a configuration error that says to run `gate lock` when the lock is missing, not
/// tracked, or does not match.
pub fn verify(workspace: &Workspace) -> Result<Option<Lock>> {
    verify_with(workspace, &BTreeSet::new())
}

/// [`verify`], for a run in which a `--plugin` build replaces each plugin of `overridden`: the
/// lock need not pin a source that only overridden plugins declare, and may still pin it.
///
/// # Errors
/// As [`verify`], for every other plugin and the core.
pub fn verify_with(
    workspace: &Workspace,
    overridden: &BTreeSet<PluginName>,
) -> Result<Option<Lock>> {
    let mut wanted = Wanted::of(workspace);
    let covered: BTreeSet<PluginSource> = wanted
        .plugins
        .iter()
        .filter(|(_, declarations)| {
            declarations
                .iter()
                .all(|declaration| overridden.contains(&declaration.name))
        })
        .map(|(source, _)| source.clone())
        .collect();
    wanted.plugins.retain(|source, _| !covered.contains(source));
    let snapshot = workspace.snapshot();
    let Some(text) = snapshot.text(LOCK_FILE)? else {
        if wanted.plugins.is_empty() {
            return Ok(None);
        }
        let on_disk = snapshot.git().root().join(LOCK_FILE).is_file();
        let help = if on_disk {
            "it exists but is not tracked; run git add gate.lock (gate reads tracked files only)"
        } else {
            "run gate lock to resolve and pin them, then commit gate.lock"
        };
        return Err(Error::config(format!(
            "plugins are declared and there is no gate.lock to pin them; {help}"
        )));
    };
    let lock = Lock::parse(LOCK_FILE, &text)?;
    let mut pinned = lock.clone();
    pinned.plugins.retain(|source, _| !covered.contains(source));
    let lines = pinned.mismatches(&wanted);
    if lines.is_empty() {
        Ok(Some(lock))
    } else {
        Err(Error::config(format!(
            "gate.lock does not match the configuration: {}; run gate lock and commit gate.lock",
            lines.join("; ")
        )))
    }
}

/// The version of this core.
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Checks that the running core, version `running`, is the one the configuration asks for
/// (`[gate] version`, which `gate.lock` pins). Every command but `gate init` and `gate lock`, which
/// write the pin, runs this first.
///
/// # Errors
/// Fails with an environment error that says how to run the pinned core.
pub fn check_core(workspace: &Workspace, running: &str) -> Result<()> {
    let wanted = Wanted::of(workspace).core;
    if wanted.is_empty() || wanted == running {
        return Ok(());
    }
    Err(Error::environment(format!(
        "this is gate {running}, and gate.toml asks for gate {wanted} ([gate] version), which gate.lock pins; run bin/gate, which runs the pinned core, or set [gate] version to {running} and run gate lock"
    )))
}

/// What [`resolve`] did.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Resolution {
    /// The new lock.
    pub lock: Lock,
    /// What was resolved from a release now.
    pub resolved: Vec<String>,
    /// What the old lock already pinned and was kept.
    pub kept: Vec<String>,
    /// What the old lock pinned and no configuration declares any more.
    pub removed: Vec<String>,
}

/// Resolves what the configuration wants into a lock. Pins the old lock already has are kept;
/// everything else is resolved from its GitHub release, downloaded into the cache and hashed.
///
/// # Errors
/// Fails when a release or an asset does not exist, a download fails, a credential is missing
/// or refused, or a download does not match the checksum the release publishes. Releases that do
/// not exist are a configuration error naming every one of them; any other failure stops at once.
pub fn resolve(wanted: &Wanted, existing: Option<&Lock>, fetcher: &Fetcher) -> Result<Resolution> {
    let mut resolution = Resolution::default();
    let mut missing = Missing::default();

    let kept_core = existing.and_then(|lock| lock.core.as_ref()).filter(|core| {
        core.version == wanted.core
            && TARGETS
                .iter()
                .all(|target| core.targets.contains_key(*target))
    });
    if let Some(core) = kept_core {
        resolution.lock.core = Some(core.clone());
        resolution.kept.push(format!("core {}", core.version));
    } else {
        let (owner, repo) = CORE_REPOSITORY;
        let tag = format!("v{}", wanted.core);
        let mut targets = BTreeMap::new();
        for target in TARGETS {
            let asset = format!("gate-{target}.tar.gz");
            match missing.note(fetcher.pin_release_asset(owner, repo, &tag, &asset))? {
                Some(pin) => {
                    targets.insert(target.to_owned(), pin);
                }
                // One message for the release: its other targets are missing with it.
                None => break,
            }
        }
        if targets.len() == TARGETS.len() {
            resolution.lock.core = Some(CorePin {
                version: wanted.core.clone(),
                targets,
            });
            resolution.resolved.push(format!("core {}", wanted.core));
        }
    }

    for source in wanted.plugins.keys() {
        if let Some(pin) = existing.and_then(|lock| lock.plugins.get(source)) {
            resolution.lock.plugins.insert(source.clone(), pin.clone());
            resolution.kept.push(source.to_string());
        } else {
            let pinned = fetcher.pin_release_asset(
                &source.owner,
                &source.repo,
                &source.release_tag(),
                &source.asset(),
            );
            if let Some(pin) = missing.note(pinned)? {
                resolution.lock.plugins.insert(source.clone(), pin);
                resolution.resolved.push(source.to_string());
            }
        }
    }
    missing.fail()?;

    if let Some(lock) = existing {
        resolution.removed = lock
            .plugins
            .keys()
            .filter(|source| !wanted.plugins.contains_key(*source))
            .map(ToString::to_string)
            .collect();
    }
    Ok(resolution)
}

/// The releases [`resolve`] found missing so far: a configuration error each, collected so that one
/// run names every version gate.toml asks for that is not released.
#[derive(Debug, Default)]
struct Missing(Vec<String>);

impl Missing {
    /// The pin, or `None` when its release does not exist; any other failure is returned.
    fn note(&mut self, pinned: Result<Pin>) -> Result<Option<Pin>> {
        match pinned {
            Ok(pin) => Ok(Some(pin)),
            Err(error) if error.kind() == ErrorKind::Config => {
                self.0.push(error.message().to_owned());
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    /// One configuration error for every missing release, or nothing when none is missing.
    fn fail(self) -> Result<()> {
        match self.0.as_slice() {
            [] => Ok(()),
            [one] => Err(Error::config(one.clone())),
            many => Err(Error::config(format!(
                "{} releases gate.toml asks for do not exist; nothing was written:\n  {}",
                many.len(),
                many.join("\n  ")
            ))),
        }
    }
}

/// `gate lock --set <name>=<url> --file <path>`: pins the source declared under `name` to `url`
/// and the sha256 of the build, without a download, and drops pins no configuration declares.
/// Every other pin stays as it is.
///
/// # Errors
/// Fails when there is no lock yet, `name` is not declared with one source, or the URL is not one
/// gate fetches.
pub fn set(wanted: &Wanted, existing: Option<Lock>, name: &PluginName, pin: Pin) -> Result<Lock> {
    let Some(mut lock) = existing else {
        return Err(Error::config(format!(
            "there is no {LOCK_FILE} to set a pin in; run gate lock first and commit it"
        )));
    };
    fetch::check_url(&pin.url)
        .map_err(|message| Error::config(format!("--set {name}: {message}")))?;
    let source = wanted.source_of(name)?.clone();
    lock.plugins.insert(source, pin);
    lock.plugins
        .retain(|source, _| wanted.plugins.contains_key(source));
    Ok(lock)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sum(digit: char) -> Sha256Sum {
        Sha256Sum::parse(&digit.to_string().repeat(64)).expect("checksum")
    }

    fn lock() -> Lock {
        let mut targets = BTreeMap::new();
        for target in TARGETS {
            targets.insert(
                target.to_owned(),
                Pin {
                    url: format!("https://example.invalid/v0.1.0/gate-{target}.tar.gz"),
                    sha256: sum('a'),
                },
            );
        }
        let mut plugins = BTreeMap::new();
        plugins.insert(
            PluginSource::parse("ElsiKora/Gate:git@0.1.0").expect("source"),
            Pin {
                url: "https://example.invalid/plugin-git-v0.1.0/git.wasm".to_owned(),
                sha256: sum('b'),
            },
        );
        Lock {
            core: Some(CorePin {
                version: "0.1.0".to_owned(),
                targets,
            }),
            plugins,
        }
    }

    #[test]
    fn a_lock_renders_and_parses_back() {
        let lock = lock();
        let text = lock.render();
        assert!(text.starts_with(HEADER));
        assert!(text.contains("\n[core.targets.aarch64-apple-darwin]\nurl = \"https://example.invalid/v0.1.0/gate-aarch64-apple-darwin.tar.gz\"\n"));
        assert!(text.contains("\n[plugins.\"ElsiKora/Gate:git@0.1.0\"]\n"));
        assert_eq!(Lock::parse(LOCK_FILE, &text).expect("parse"), lock);
    }

    #[test]
    fn a_wrong_lock_says_what_is_wrong() {
        let wrong = lock().render().replace(&"b".repeat(64), "short");
        let error = Lock::parse(LOCK_FILE, &wrong).expect_err("short checksum");
        assert!(
            error
                .message()
                .contains("\"short\" is not a sha256 checksum"),
            "{error}"
        );

        let plain = lock()
            .render()
            .replace("https://example.invalid", "http://example.invalid");
        let error = Lock::parse(LOCK_FILE, &plain).expect_err("plain http");
        assert!(error.message().contains("https"), "{error}");
    }
}

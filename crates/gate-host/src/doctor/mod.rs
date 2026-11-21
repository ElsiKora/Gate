//! `gate doctor`: checks that a clone is ready to run Gate, and says what to do when it is not.

use std::fmt;
use std::path::Path;
use std::process::Command;

use crate::cache::{Cache, Held};
use crate::lock::{CORE_VERSION, LOCK_FILE, Lock, verify};
use crate::sync::{HOOKS_DIRECTORY, LAUNCHER};
use crate::workspace::Workspace;

/// The oldest git release Gate is tested with.
pub const MINIMUM_GIT: (u32, u32) = (2, 25);

/// How a check ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    /// Ready: what was found.
    Ok(String),
    /// Not ready: what to do.
    Fix(String),
}

/// One check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Check {
    /// What was checked.
    pub name: &'static str,
    /// How it ended.
    pub status: Status,
}

impl fmt::Display for Check {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.status {
            Status::Ok(found) => write!(formatter, "ok   {}: {found}", self.name),
            Status::Fix(todo) => write!(formatter, "fix  {}: {todo}", self.name),
        }
    }
}

/// Whether every check is ok.
#[must_use]
pub fn healthy(checks: &[Check]) -> bool {
    checks
        .iter()
        .all(|check| matches!(check.status, Status::Ok(_)))
}

/// Runs every check: git, the hooks path, the launcher (`launcher` is the text `gate init`
/// writes), `gate.lock`, the cache, the plugins of the lock and whether they are compiled.
#[must_use]
pub fn doctor(workspace: &Workspace, cache: Option<&Cache>, launcher: &[u8]) -> Vec<Check> {
    let lock = verify(workspace);
    vec![
        Check {
            name: "git",
            status: git_version(),
        },
        Check {
            name: "hooks",
            status: hooks(workspace),
        },
        Check {
            name: "launcher",
            status: launcher_status(workspace, launcher),
        },
        Check {
            name: "lock",
            status: lock_status(&lock),
        },
        Check {
            name: "cache",
            status: cache.map_or_else(
                || {
                    Status::Fix(
                        "no cache directory; set $GATE_CACHE_DIR to an absolute path, or HOME"
                            .to_owned(),
                    )
                },
                |cache| writable(cache.root()),
            ),
        },
        Check {
            name: "plugins",
            status: plugins(lock.as_ref().ok().and_then(Option::as_ref), cache),
        },
        Check {
            name: "compiled",
            status: compiled(lock.as_ref().ok().and_then(Option::as_ref), cache),
        },
    ]
}

/// Parses `git version 2.47.1` (and vendor suffixes such as `(Apple Git-154)`).
#[must_use]
pub fn parse_git_version(text: &str) -> Option<(u32, u32)> {
    let version = text.trim().strip_prefix("git version ")?;
    let mut parts = version.split(['.', ' ']);
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

fn git_version() -> Status {
    let output = match Command::new("git").arg("--version").output() {
        Ok(output) if output.status.success() => output,
        Ok(_) | Err(_) => {
            return Status::Fix("git does not run; install git and put it on PATH".to_owned());
        }
    };
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    match parse_git_version(&text) {
        Some(version) if version >= MINIMUM_GIT => Status::Ok(text),
        Some(_) => Status::Fix(format!(
            "{text} is older than {}.{}; update git",
            MINIMUM_GIT.0, MINIMUM_GIT.1
        )),
        None => Status::Fix(format!(
            "cannot read the version from {text:?}; check the git on PATH"
        )),
    }
}

fn hooks(workspace: &Workspace) -> Status {
    if workspace.root_config().file().hooks.is_none() {
        return Status::Ok("gate.toml has no [hooks]".to_owned());
    }
    let git = workspace.snapshot().git();
    match git.try_text(&["config", "--get", "core.hooksPath"]) {
        Ok(Some(path)) if path.trim() == HOOKS_DIRECTORY => {
            Status::Ok(format!("core.hooksPath is {HOOKS_DIRECTORY}"))
        }
        Ok(Some(path)) => Status::Fix(format!(
            "core.hooksPath is {:?}, not {HOOKS_DIRECTORY}; run gate sync",
            path.trim()
        )),
        Ok(None) if !git.root().join(HOOKS_DIRECTORY).is_dir() => Status::Fix(format!(
            "gate.toml has [hooks], and gate sync has not written them yet; run gate sync to write {HOOKS_DIRECTORY} and set core.hooksPath"
        )),
        Ok(None) => Status::Fix(format!(
            "core.hooksPath is not set, so the hooks in {HOOKS_DIRECTORY} do not run; run gate sync"
        )),
        Err(error) => Status::Fix(format!("cannot read core.hooksPath: {}", error.message())),
    }
}

fn launcher_status(workspace: &Workspace, expected: &[u8]) -> Status {
    let path = workspace.snapshot().git().root().join(LAUNCHER);
    let Ok(contents) = std::fs::read(&path) else {
        return Status::Fix(format!("{LAUNCHER} is missing; run gate init"));
    };
    if !is_executable(&path) {
        return Status::Fix(format!(
            "{LAUNCHER} is not executable; run chmod +x {LAUNCHER} and commit the mode"
        ));
    }
    if contents == expected {
        Status::Ok(format!("{LAUNCHER} is the launcher of gate {CORE_VERSION}"))
    } else {
        Status::Fix(format!(
            "{LAUNCHER} differs from the launcher gate {CORE_VERSION} writes; run gate init, then commit it"
        ))
    }
}

fn lock_status(lock: &crate::error::Result<Option<Lock>>) -> Status {
    let lock = match lock {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            return Status::Fix(format!(
                "{LOCK_FILE} is not tracked, and {LAUNCHER} reads the core from it; run gate lock, then commit {LOCK_FILE}"
            ));
        }
        Err(error) => return Status::Fix(error.message().to_owned()),
    };
    let Some(core) = &lock.core else {
        return Status::Fix(format!(
            "{LOCK_FILE} pins no core, so {LAUNCHER} has nothing to run; run gate lock"
        ));
    };
    Status::Ok(format!(
        "{LOCK_FILE} pins gate {} and {} {}",
        core.version,
        lock.plugins.len(),
        if lock.plugins.len() == 1 {
            "plugin"
        } else {
            "plugins"
        }
    ))
}

fn writable(cache: &Path) -> Status {
    let probe = cache.join(format!(".doctor.{}", std::process::id()));
    let result = std::fs::create_dir_all(cache)
        .and_then(|()| std::fs::write(&probe, b"probe"))
        .and_then(|()| std::fs::remove_file(&probe));
    match result {
        Ok(()) => Status::Ok(format!("{} is writable", cache.display())),
        Err(error) => Status::Fix(format!(
            "cannot write to {}: {error}; fix its permissions or set $GATE_CACHE_DIR",
            cache.display()
        )),
    }
}

fn plugins(lock: Option<&Lock>, cache: Option<&Cache>) -> Status {
    let Some(lock) = lock else {
        return Status::Fix(format!(
            "no usable {LOCK_FILE} (see lock), so the plugins cannot be checked"
        ));
    };
    if lock.plugins.is_empty() {
        return Status::Ok(format!("{LOCK_FILE} pins no plugin"));
    }
    let Some(cache) = cache else {
        return Status::Fix("no cache directory to hold the plugins".to_owned());
    };
    // Read only: a changed entry is reported, and the next gate check replaces it.
    let mut missing = Vec::new();
    let mut changed = Vec::new();
    for (source, pin) in &lock.plugins {
        match cache.held(&pin.sha256) {
            Ok(Held::Verified) => {}
            Ok(Held::Missing) => missing.push(source.to_string()),
            Ok(Held::Changed(_)) => changed.push(source.to_string()),
            Err(error) => return Status::Fix(error.message().to_owned()),
        }
    }
    let mut problems = Vec::new();
    if !missing.is_empty() {
        problems.push(format!("not in the cache: {}", missing.join(", ")));
    }
    if !changed.is_empty() {
        problems.push(format!(
            "in the cache but not matching the sha256 gate.lock pins: {}",
            changed.join(", ")
        ));
    }
    if problems.is_empty() {
        Status::Ok(format!(
            "every plugin of {LOCK_FILE} is in the cache and matches its sha256"
        ))
    } else {
        Status::Fix(format!(
            "{}; run gate check once with network access",
            problems.join("; ")
        ))
    }
}

/// Whether each plugin of `lock` that the cache holds is compiled under this core: a check
/// compiles the others first, which the hook budget does not count but a developer waits for.
fn compiled(lock: Option<&Lock>, cache: Option<&Cache>) -> Status {
    let (Some(lock), Some(cache)) = (lock, cache) else {
        return Status::Ok(
            "nothing to compile without a usable gate.lock and cache (see above)".to_owned(),
        );
    };
    // A plugin is compiled once it has loaded under this core: its declaration is then stored.
    let mut waiting = Vec::new();
    for (source, pin) in &lock.plugins {
        if matches!(cache.held(&pin.sha256), Ok(Held::Verified))
            && !crate::plugins::declarations::stored(cache, &pin.sha256)
        {
            waiting.push(source.to_string());
        }
    }
    if waiting.is_empty() {
        return Status::Ok(format!(
            "every plugin of {LOCK_FILE} in the cache is compiled under gate {CORE_VERSION}"
        ));
    }
    Status::Fix(format!(
        "{} {} not compiled yet under gate {CORE_VERSION} ({}); run gate sync before the first commit, or the first gate check compiles {}",
        waiting.len(),
        if waiting.len() == 1 {
            "plugin is"
        } else {
            "plugins are"
        },
        waiting.join(", "),
        if waiting.len() == 1 { "it" } else { "them" }
    ))
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

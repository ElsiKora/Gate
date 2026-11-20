//! `gate init`, `gate lock` and `gate cache prune`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use gate_host::cache::{Bounds, Cache, Entry, Reason};
use gate_host::config::PluginName;
use gate_host::lock::{self, Guard, LOCK_FILE, Lock, Pin, Wanted};
use gate_host::{Error, Fetcher, Git, Source, Workspace, files};

/// The launcher `gate init` writes to `bin/gate`.
pub const LAUNCHER: &str = include_str!("launcher.sh");

/// Where the launcher goes, relative to the repository root.
pub const LAUNCHER_PATH: &str = "bin/gate";

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What `gate lock` was asked.
#[derive(Debug, Default)]
pub struct LockRequest {
    /// `--set <name>=<url>`.
    pub set: Option<String>,
    /// `--file <path>`: the built plugin `--set` pins.
    pub file: Option<PathBuf>,
    /// `--plugin` overrides, which `gate lock` refuses.
    pub overrides: Vec<String>,
}

/// `gate lock`: resolves what the configuration declares into `gate.lock`, or, with `--set` and
/// `--file`, pins one plugin to a local build under its future release URL: the build's sha256
/// goes into the pin and its bytes into the cache, where `gate check` finds them.
///
/// # Errors
/// Fails with an override, on a configuration error, and when a release cannot be resolved.
pub fn lock(request: &LockRequest) -> Result<String, Error> {
    if let Some(first) = request.overrides.first() {
        return Err(Error::config(format!(
            "gate lock refuses --plugin {first}: an override replaces a plugin for one gate check run and never enters gate.lock; pin the build under its release URL with gate lock --set <name>=<url> --file <path>"
        )));
    }
    let workspace = Workspace::open(Path::new("."), Source::WorkTree)?;
    let root = workspace.snapshot().git().root().to_path_buf();
    let wanted = Wanted::of(&workspace);

    if let Some(set) = &request.set {
        let (name, url) = set.split_once('=').ok_or_else(|| {
            Error::config(format!(
                "--set {set:?} is not <name>=<url>; for example house=https://github.com/acme/rules/releases/download/plugin-house-v1.3.0/house.wasm"
            ))
        })?;
        let name = PluginName::try_from(name.to_owned()).map_err(Error::config)?;
        let file = request.file.as_deref().ok_or_else(|| {
            Error::config(format!(
                "--set {set} needs --file <path>, the built plugin to pin"
            ))
        })?;
        let sha256 = gate_host::cache::hash_file(file)
            .map_err(|error| Error::environment(format!("--file: {}", error.message())))?;
        let pin = Pin {
            url: url.to_owned(),
            sha256: sha256.clone(),
        };
        let line = format!(
            "pinned {name} to {} sha256 {} from {}, now in the cache",
            pin.url,
            pin.sha256,
            file.display()
        );
        // The file is hashed before the guard is taken, so a build that changes while this run
        // waits fails the check below. Every check first: nothing enters the cache or gate.lock for
        // a pin that is refused.
        let _guard = Guard::take(workspace.snapshot().git())?;
        let locked = lock::set(&wanted, Lock::read(&root)?, &name, pin)?;
        let admitted = Cache::from_env()?.admit(file)?;
        if admitted != sha256 {
            return Err(Error::environment(format!(
                "--file {} changed while gate read it (sha256 {sha256}, then {admitted}); gate.lock is unchanged: run gate lock --set again once the build has finished",
                file.display()
            )));
        }
        locked.write(&root)?;
        return Ok(format!("{LOCK_FILE}: {line}\n{}", untracked(&workspace)));
    }

    let mut text = String::new();
    let _guard = Guard::take(workspace.snapshot().git())?;
    let existing = match Lock::read(&root) {
        Ok(existing) => existing,
        Err(error) => {
            let _ = writeln!(
                text,
                "the old {LOCK_FILE} is unreadable ({}); resolving everything again",
                error.message()
            );
            None
        }
    };
    let fetcher = Fetcher::from_env()?;
    let resolution = lock::resolve(&wanted, existing.as_ref(), &fetcher)?;
    resolution.lock.write(&root)?;
    for (verb, items) in [
        ("resolved", &resolution.resolved),
        ("kept", &resolution.kept),
        ("removed", &resolution.removed),
    ] {
        for item in items {
            let _ = writeln!(text, "{verb} {item}");
        }
    }
    let _ = writeln!(text, "wrote {LOCK_FILE}");
    text.push_str(&untracked(&workspace));
    Ok(text)
}

fn untracked(workspace: &Workspace) -> String {
    if workspace.snapshot().contains(LOCK_FILE) {
        String::new()
    } else {
        format!("gate reads tracked files only; run git add {LOCK_FILE}\n")
    }
}

/// `gate init`: writes a starting `gate.toml` when there is none, the launcher `bin/gate`, and
/// `gate.lock`.
///
/// # Errors
/// Fails outside a git repository, when a file cannot be written, and when the core's release
/// cannot be resolved.
pub fn init() -> Result<String, Error> {
    let git = Git::discover(Path::new("."))?;
    let root = git.root().to_path_buf();
    let mut text = String::new();

    let config = root.join("gate.toml");
    let fresh = !config.exists();
    if fresh {
        files::replace(&config, starting_config().as_bytes())?;
        text.push_str("wrote gate.toml\n");
    }

    let launcher = root.join(LAUNCHER_PATH);
    if std::fs::read_to_string(&launcher).is_ok_and(|current| current == LAUNCHER) {
        text.push_str("kept bin/gate\n");
    } else {
        if let Some(directory) = launcher.parent() {
            std::fs::create_dir_all(directory).map_err(|error| {
                Error::environment(format!("cannot create {}: {error}", directory.display()))
            })?;
        }
        // Replaced, never written in place: a launcher that runs meanwhile reads the old file
        // or the new one, and the new one is executable before it appears.
        files::replace_executable(&launcher, LAUNCHER.as_bytes())?;
        text.push_str("wrote bin/gate\n");
    }

    // A fresh gate.toml is not tracked yet, so its only pin is the core.
    let wanted = if fresh {
        Wanted::core(VERSION)
    } else {
        Wanted::of(&Workspace::open(&root, Source::WorkTree)?)
    };
    let _guard = Guard::take(&git)?;
    let existing = Lock::read(&root).ok().flatten();
    let resolution = lock::resolve(&wanted, existing.as_ref(), &Fetcher::from_env()?)?;
    resolution.lock.write(&root)?;
    let _ = writeln!(text, "wrote {LOCK_FILE}: core {}", wanted.core);
    text.push_str("commit them: git add gate.toml gate.lock bin/gate\n");
    Ok(text)
}

fn starting_config() -> String {
    format!(
        r#"#:schema https://github.com/ElsiKora/Gate/releases/download/v{VERSION}/gate.schema.json

[gate]
version = "{VERSION}"

# Add plugins and turn their rules on, then run bin/gate lock. For example, branch names and
# commit messages as ElsiKora's linters check them (main names your main branch):
#
# [plugins]
# git = {{ source = "ElsiKora/Gate:git@{VERSION}", options = {{ main = ["refs/remotes/origin/main", "refs/heads/main"] }} }}
#
# [rules]
# "git/branch-name" = {{ level = "error", options = {{ template = ":type/:ticket-:name" }} }}
# "git/commit-message" = "error"
"#
    )
}

/// `gate cache prune`: lists, or with `yes` removes, the cache entries no `gate.lock` of the given
/// repositories (every worktree of each) pins, the derived entries nothing can use, and those the
/// bounds remove.
///
/// # Errors
/// Fails when a repository or a lock cannot be read, or an entry cannot be removed.
pub fn prune(repositories: &[PathBuf], bounds: &Bounds, yes: bool) -> Result<String, Error> {
    let current = [PathBuf::from(".")];
    let repositories = if repositories.is_empty() {
        &current[..]
    } else {
        repositories
    };
    let mut pinned = lock::Pinned::default();
    for repository in repositories {
        lock::pinned_in(repository, &mut pinned)?;
    }
    let referenced = &pinned.checksums;
    let locks = &pinned.files;

    let cache = Cache::from_env()?;
    let plan = cache.plan(referenced, &pinned.plugins, bounds)?;
    let entries = &plan.entries;
    let mut text = format!(
        "{} pinned by {} {}; cache {}\n",
        plural(referenced.len(), "entry", "entries"),
        locks.len(),
        if locks.len() == 1 { "lock" } else { "locks" },
        cache.root().display()
    );
    if locks.is_empty() {
        text.push_str("no gate.lock was found: every entry counts as unreferenced\n");
    }
    let total: u64 = entries.iter().map(|entry| entry.bytes).sum();
    if let Some(max) = plan.over {
        let _ = writeln!(
            text,
            "the cache stays at {}, over --max-size {}: what is left is pinned downloads, modules/ and files in use, which no bound removes",
            size(plan.left),
            size(max)
        );
    }
    if entries.is_empty() {
        text.push_str("nothing to remove\n");
        return Ok(text);
    }
    let verb = if yes { "removed" } else { "would remove" };
    let _ = writeln!(
        text,
        "{verb} {}, {}:",
        plural(entries.len(), "entry", "entries"),
        size(total)
    );
    for entry in entries {
        let _ = writeln!(
            text,
            "  {}  {}  {}",
            relative(&cache, entry),
            size(entry.bytes),
            why(entry.reason)
        );
    }
    if yes {
        cache.remove(entries)?;
    } else {
        text.push_str("run gate cache prune --yes to remove them\n");
    }
    Ok(text)
}

fn why(reason: Reason) -> String {
    match reason {
        Reason::Unpinned => "no lock pins it".to_owned(),
        Reason::Stale => "left by a run that stopped".to_owned(),
        Reason::Unnamed => "no stored answer names it".to_owned(),
        Reason::OldFormat => "an older results format".to_owned(),
        Reason::Older(days) => format!(
            "last used more than {} ago",
            plural(usize::try_from(days).unwrap_or(usize::MAX), "day", "days")
        ),
        Reason::OverSize(max) => format!("least recently used, over --max-size {}", size(max)),
    }
}

fn relative(cache: &Cache, entry: &Entry) -> String {
    entry
        .path
        .strip_prefix(cache.root())
        .unwrap_or(&entry.path)
        .display()
        .to_string()
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes;
    let mut unit = 0;
    while value >= 1024 * 1024 && unit + 1 < UNITS.len() {
        value /= 1024;
        unit += 1;
    }
    let tenths = value * 10 / 1024;
    format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::size;

    #[test]
    fn sizes_read_in_binary_units() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(1536), "1.5 KiB");
        assert_eq!(size(5 * 1024 * 1024), "5.0 MiB");
    }
}

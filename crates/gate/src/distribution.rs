//! `gate init`, `gate lock` and `gate cache prune`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use gate_host::cache::{Cache, Entry};
use gate_host::config::PluginName;
use gate_host::lock::{self, LOCK_FILE, Lock, Pin, Wanted};
use gate_host::{Error, Fetcher, Git, Sha256Sum, Source, Workspace};

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
    /// `--sha256 <hash>`.
    pub sha256: Option<String>,
    /// `--plugin` overrides, which `gate lock` refuses.
    pub overrides: Vec<String>,
}

/// `gate lock`: resolves what the configuration declares into `gate.lock`, or, with `--set`, moves
/// one pin without a download.
///
/// # Errors
/// Fails with an override, on a configuration error, and when a release cannot be resolved.
pub fn lock(request: &LockRequest) -> Result<String, Error> {
    if let Some(first) = request.overrides.first() {
        return Err(Error::config(format!(
            "gate lock refuses --plugin {first}: an override replaces a plugin for one gate check run and never enters gate.lock; release the build and pin it with gate lock --set <name>=<url> --sha256 <hash>"
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
        let sha256 = Sha256Sum::parse(request.sha256.as_deref().unwrap_or_default())
            .map_err(|message| Error::config(format!("--sha256: {message}")))?;
        let pin = Pin {
            url: url.to_owned(),
            sha256,
        };
        let line = format!("pinned {name} to {} sha256 {}", pin.url, pin.sha256);
        let locked = lock::set(&wanted, Lock::read(&root)?, &name, pin)?;
        locked.write(&root)?;
        return Ok(format!("{LOCK_FILE}: {line}\n{}", untracked(&workspace)));
    }

    let mut text = String::new();
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
        write(&config, &starting_config())?;
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
        write(&launcher, LAUNCHER)?;
        executable(&launcher)?;
        text.push_str("wrote bin/gate\n");
    }

    // A fresh gate.toml is not tracked yet, so its only pin is the core.
    let wanted = if fresh {
        Wanted::core(VERSION)
    } else {
        Wanted::of(&Workspace::open(&root, Source::WorkTree)?)
    };
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

# Add plugins and turn their rules on, then run bin/gate lock:
#
# [plugins]
# git = {{ source = "ElsiKora/Gate:git@{VERSION}" }}
#
# [rules]
# "git/commit-subject" = "error"
"#
    )
}

fn write(path: &Path, contents: &str) -> Result<(), Error> {
    std::fs::write(path, contents)
        .map_err(|error| Error::environment(format!("cannot write {}: {error}", path.display())))
}

#[cfg(unix)]
fn executable(path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).map_err(|error| {
        Error::environment(format!(
            "cannot make {} executable: {error}",
            path.display()
        ))
    })
}

#[cfg(not(unix))]
fn executable(_path: &Path) -> Result<(), Error> {
    Ok(())
}

/// `gate cache prune`: lists, or with `yes` removes, the cache entries no `gate.lock` of the given
/// repositories (every worktree of each) pins.
///
/// # Errors
/// Fails when a repository or a lock cannot be read, or an entry cannot be removed.
pub fn prune(repositories: &[PathBuf], yes: bool) -> Result<String, Error> {
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
    let entries = cache.unreferenced(referenced, !pinned.plugins.is_empty())?;
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
    for entry in &entries {
        let _ = writeln!(text, "  {}  {}", relative(&cache, entry), size(entry.bytes));
    }
    if yes {
        cache.remove(&entries)?;
    } else {
        text.push_str("run gate cache prune --yes to remove them\n");
    }
    Ok(text)
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

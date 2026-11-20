//! `gate init`, `gate lock`, `gate cache prune` and the `bin/gate` launcher, against a local
//! fixture server: no test touches the network.

#[path = "../../gate-host/tests/host/http.rs"]
mod http;

#[path = "../../gate-runtime/tests/support/component.rs"]
mod built;

use std::fmt::Write as _;
use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use http::{Route, Server, release_json, sha256};

/// The version of the core under test.
const VERSION: &str = env!("CARGO_PKG_VERSION");

const TARGETS: [&str; 2] = ["aarch64-apple-darwin", "x86_64-unknown-linux-musl"];

/// The released target this machine runs, if any.
fn host_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-musl"),
        _ => None,
    }
}

struct Repo {
    directory: tempfile::TempDir,
    /// A directory first on `PATH`, with a `gh` that is not signed in.
    bin: tempfile::TempDir,
    cache: tempfile::TempDir,
}

impl Repo {
    fn new() -> Self {
        let repo = Self {
            directory: tempfile::tempdir().expect("repository"),
            bin: tempfile::tempdir().expect("bin"),
            cache: tempfile::tempdir().expect("cache"),
        };
        repo.fake_gh("echo 'not signed in' >&2\nexit 1\n");
        repo.git(&["init", "-q", "-b", "main"]);
        repo
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }

    fn write(&self, path: &str, contents: &str) -> &Self {
        let full = self.path().join(path);
        std::fs::create_dir_all(full.parent().expect("parent")).expect("directories");
        std::fs::write(full, contents).expect("write");
        self
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.path().join(path)).expect("read")
    }

    fn fake_gh(&self, body: &str) {
        let path = self.bin.path().join("gh");
        std::fs::write(&path, format!("#!/bin/sh\n{body}")).expect("gh");
        executable(&path);
    }

    fn git(&self, arguments: &[&str]) {
        let output = self
            .environment(
                Command::new("git")
                    .arg("-C")
                    .arg(self.path())
                    .args(arguments),
                None,
            )
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn commit(&self) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", "configure"]);
    }

    /// A command with no credential, no proxy, the test's `gh` first on `PATH`, the cache in a
    /// temporary directory, and GitHub at `server`.
    fn environment<'a>(
        &self,
        command: &'a mut Command,
        server: Option<&Server>,
    ) -> &'a mut Command {
        let path = format!("{}:/usr/bin:/bin", self.bin.path().display());
        command
            .current_dir(self.path())
            .env("PATH", path)
            .env("GATE_CACHE_DIR", self.cache.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid");
        for name in [
            "GIT_DIR",
            "GIT_INDEX_FILE",
            "GIT_WORK_TREE",
            "GATE_GITHUB_TOKEN",
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "GH_ENTERPRISE_TOKEN",
            "GATE_GITHUB_API_URL",
            "GATE_GITHUB_URL",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            command.env_remove(name);
        }
        if let Some(server) = server {
            command
                .env("GATE_GITHUB_API_URL", server.url(""))
                .env("GATE_GITHUB_URL", server.url(""));
        }
        command
    }

    fn gate(&self, arguments: &[&str], server: Option<&Server>, env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_gate"));
        self.environment(command.args(arguments), server);
        for (name, value) in env {
            command.env(name, value);
        }
        command.output().expect("gate")
    }

    /// Runs `bin/gate` under `dash` (the strictest POSIX shell at hand), with its own cache.
    fn launcher(&self, cache: &Path, arguments: &[&str]) -> Command {
        let shell = ["/bin/dash", "/usr/bin/dash"]
            .into_iter()
            .find(|path| Path::new(path).exists())
            .unwrap_or("/bin/sh");
        let mut command = Command::new(shell);
        self.environment(
            command.arg(self.path().join("bin/gate")).args(arguments),
            None,
        );
        command.env("GATE_CACHE_DIR", cache);
        command
    }
}

#[cfg(unix)]
fn executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

#[cfg(not(unix))]
fn executable(_path: &Path) {}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A core archive whose `gate` prints its arguments.
fn core_archive() -> Vec<u8> {
    let directory = tempfile::tempdir().expect("archive");
    let gate = directory.path().join("gate");
    std::fs::write(&gate, "#!/bin/sh\nprintf 'pinned core ran: %s\\n' \"$*\"\n").expect("core");
    executable(&gate);
    let archive = directory.path().join("core.tar.gz");
    let status = Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(directory.path())
        .arg("gate")
        .status()
        .expect("tar");
    assert!(status.success());
    std::fs::read(archive).expect("read archive")
}

/// The core's release of ElsiKora/Gate on the fixture, which stands in for github.com.
fn core_release(server: &Server, archive: &[u8], delay: Duration) {
    let mut assets = Vec::new();
    for (index, target) in TARGETS.into_iter().enumerate() {
        let name = format!("gate-{target}.tar.gz");
        let path = format!("/ElsiKora/Gate/releases/download/v{VERSION}/{name}");
        server.route(&path, Route::ok(archive).delayed(delay));
        assets.push((
            name,
            server.url(&format!("/repos/ElsiKora/Gate/releases/assets/{index}")),
            server.url(&path),
        ));
    }
    let digest = sha256(archive);
    let listed: Vec<(&str, &str, &str, Option<&str>)> = assets
        .iter()
        .map(|(name, api, download)| {
            (
                name.as_str(),
                api.as_str(),
                download.as_str(),
                Some(digest.as_str()),
            )
        })
        .collect();
    server.serve(
        concat!(
            "/repos/ElsiKora/Gate/releases/tags/v",
            env!("CARGO_PKG_VERSION")
        ),
        release_json(&listed),
    );
}

/// Every file below `root`, relative.
fn files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path.strip_prefix(root).expect("below").to_path_buf());
            }
        }
    }
    found.sort();
    found
}

/// A cached core that no longer hashes to what was recorded, or is a link, is installed again
/// before it runs.
fn reinstalls_a_bad_core(repo: &Repo, server: &Server, cache: &Path, core: &Path, target: &str) {
    let downloads = |server: &Server| {
        server
            .requests()
            .iter()
            .filter(|request| request.path.ends_with(&format!("gate-{target}.tar.gz")))
            .count()
    };
    let before = downloads(server);

    // A cached core that no longer hashes to what was recorded, or is a link, is installed
    // again before it runs.
    let installed = cache.join(core);
    std::fs::write(&installed, "#!/bin/sh\necho tampered\n").expect("tamper");
    let output = repo.launcher(cache, &["check"]).output().expect("launcher");
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "pinned core ran: check\n");
    let elsewhere = cache.join("elsewhere");
    std::fs::copy(&installed, &elsewhere).expect("copy");
    std::fs::remove_file(&installed).expect("remove");
    std::os::unix::fs::symlink(&elsewhere, &installed).expect("link");
    let output = repo.launcher(cache, &["check"]).output().expect("launcher");
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(
        !std::fs::symlink_metadata(&installed)
            .expect("core")
            .file_type()
            .is_symlink(),
        "the link was replaced by the verified core"
    );
    assert_eq!(
        downloads(server),
        before + 2,
        "each bad core was fetched again"
    );
    std::fs::remove_file(&elsewhere).expect("remove");
}

#[test]
fn init_writes_the_launcher_and_lock_and_the_launcher_runs_the_pinned_core() {
    let Some(target) = host_target() else {
        return;
    };
    let server = Server::start();
    let archive = core_archive();
    core_release(&server, &archive, Duration::from_millis(300));
    let repo = Repo::new();

    let output = repo.gate(&["init"], Some(&server), &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "wrote gate.toml\nwrote bin/gate\nwrote gate.lock: core ",
            env!("CARGO_PKG_VERSION"),
            "\ncommit them: git add gate.toml gate.lock bin/gate\n"
        )
    );
    assert!(repo.read("gate.toml").contains(concat!(
        "[gate]\nversion = \"",
        env!("CARGO_PKG_VERSION"),
        "\"\n"
    )));
    let lock = repo.read("gate.lock");
    let digest = sha256(&archive);
    assert!(
        lock.contains(&format!(
            "[core.targets.{target}]\nurl = \"{}\"\nsha256 = \"{digest}\"\n",
            server.url(&format!(
                "/ElsiKora/Gate/releases/download/v{VERSION}/gate-{target}.tar.gz"
            ))
        )),
        "{lock}"
    );

    // Two first runs at once, on an empty cache: both run the core, and one entry is installed.
    let fresh = tempfile::tempdir().expect("cache");
    let children: Vec<_> = (0..2)
        .map(|_| {
            repo.launcher(fresh.path(), &["check", "--staged"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("launcher")
        })
        .collect();
    for child in children {
        let output = child.wait_with_output().expect("wait");
        assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
        assert_eq!(text(&output.stdout), "pinned core ran: check --staged\n");
    }
    let core = PathBuf::from("core").join(&digest);
    assert_eq!(
        files(fresh.path()),
        vec![
            PathBuf::from("CACHEDIR.TAG"),
            core.join("gate"),
            core.join("gate.sha256")
        ]
    );
    reinstalls_a_bad_core(&repo, &server, fresh.path(), &core.join("gate"), target);
    server.stop();

    let output = repo
        .launcher(fresh.path(), &["list", "--origin"])
        .output()
        .expect("launcher");
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        "pinned core ran: list --origin\n",
        "cached: no network needed"
    );

    repo.commit();
    let output = repo.gate(&["init"], None, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(
        text(&output.stdout).starts_with("kept bin/gate\n"),
        "a second init keeps the launcher and the lock: {}",
        text(&output.stdout)
    );
}

#[test]
fn the_launcher_fails_closed_when_the_core_download_stops_sending() {
    let Some(target) = host_target() else {
        return;
    };
    let server = Server::start();
    let archive = core_archive();
    // Half the archive, then silence for longer than the launcher's 30 s bound.
    server.route(
        "/core.tar.gz",
        Route::ok(archive.clone()).stalled(archive.len() / 2, Duration::from_secs(120)),
    );
    let repo = Repo::new();
    repo.write("bin/gate", include_str!("../src/launcher.sh"));
    repo.write(
        "gate.lock",
        &format!(
            "[core]\nversion = \"{VERSION}\"\n\n[core.targets.{target}]\nurl = \"{}\"\nsha256 = \"{}\"\n",
            server.url("/core.tar.gz"),
            sha256(&archive)
        ),
    );
    let cache = tempfile::tempdir().expect("cache");

    let started = Instant::now();
    let output = repo
        .launcher(cache.path(), &["check"])
        .output()
        .expect("launcher");
    let waited = started.elapsed();

    assert!(waited < Duration::from_secs(60), "failed after {waited:?}");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    let message = text(&output.stderr);
    assert!(
        message.contains(&format!(
            "cannot download gate-{target}.tar.gz from {}; try again",
            server.url("/core.tar.gz")
        )),
        "{message}"
    );
    assert_eq!(
        files(cache.path()),
        vec![PathBuf::from("CACHEDIR.TAG")],
        "no core installed"
    );
}

#[test]
fn the_launcher_fails_closed_on_a_checksum_mismatch() {
    let Some(target) = host_target() else {
        return;
    };
    let server = Server::start();
    let archive = core_archive();
    server.serve("/core.tar.gz", archive.clone());
    let repo = Repo::new();
    repo.write("bin/gate", include_str!("../src/launcher.sh"));
    let wrong = "f".repeat(64);
    repo.write(
        "gate.lock",
        &format!(
            "[core]\nversion = \"{VERSION}\"\n\n[core.targets.{target}]\nurl = \"{}\"\nsha256 = \"{wrong}\"\n",
            server.url("/core.tar.gz")
        ),
    );
    let cache = tempfile::tempdir().expect("cache");

    let output = repo
        .launcher(cache.path(), &["check"])
        .output()
        .expect("launcher");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    let message = text(&output.stderr);
    assert!(
        message.contains(&format!("checksum mismatch for gate-{target}.tar.gz")),
        "{message}"
    );
    assert!(message.contains(&wrong), "{message}");
    assert!(message.contains(&sha256(&archive)), "{message}");
    assert_eq!(
        files(cache.path()),
        vec![PathBuf::from("CACHEDIR.TAG")],
        "no core installed"
    );
}

#[test]
fn the_launcher_fails_closed_when_the_lock_pins_no_core_for_this_machine() {
    let Some(target) = host_target() else {
        return;
    };
    let other = TARGETS
        .into_iter()
        .find(|item| *item != target)
        .expect("another target");
    let repo = Repo::new();
    repo.write("bin/gate", include_str!("../src/launcher.sh"));
    repo.write(
        "gate.lock",
        &format!(
            "[core]\nversion = \"{VERSION}\"\n\n[core.targets.{other}]\nurl = \"https://github.com/ElsiKora/Gate/releases/download/v{VERSION}/gate-{other}.tar.gz\"\nsha256 = \"{}\"\n",
            "a".repeat(64)
        ),
    );
    let cache = tempfile::tempdir().expect("cache");
    let output = repo
        .launcher(cache.path(), &["check"])
        .output()
        .expect("launcher");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        format!(
            "gate: environment error: gate.lock pins no core for {target}; run gate lock and commit gate.lock\n"
        )
    );
    assert_eq!(files(cache.path()), Vec::<PathBuf>::new());
}

#[test]
fn half_an_enterprise_and_a_bad_redirect_hop_are_refused() {
    let Some(target) = host_target() else {
        return;
    };
    let server = Server::start();
    let repo = Repo::new();
    repo.write("bin/gate", include_str!("../src/launcher.sh"));
    let lock = |path: &str| {
        format!(
            "[core]\nversion = \"{VERSION}\"\n\n[core.targets.{target}]\nurl = \"{}\"\nsha256 = \"{}\"\n",
            server.url(path),
            "a".repeat(64)
        )
    };
    let cache = tempfile::tempdir().expect("cache");

    repo.write("gate.lock", &lock("/core.tar.gz"));
    let output = repo
        .launcher(cache.path(), &["check"])
        .env("GATE_GITHUB_URL", "https://github.example")
        .output()
        .expect("launcher");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text(&output.stderr).contains("only one is set"),
        "{}",
        text(&output.stderr)
    );
    repo.write(
        "gate.toml",
        concat!("[gate]\nversion = \"", env!("CARGO_PKG_VERSION"), "\"\n"),
    );
    repo.commit();
    let output = repo.gate(
        &["lock"],
        None,
        &[("GATE_GITHUB_API_URL", "https://github.example/api/v3")],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text(&output.stderr).contains("only one is set"),
        "{}",
        text(&output.stderr)
    );

    server.route(
        "/core.tar.gz",
        Route::redirect("http://localhost:1@evil.example/core.tar.gz"),
    );
    let output = repo
        .launcher(cache.path(), &["check"])
        .output()
        .expect("launcher");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text(&output.stderr).contains("leads to http://localhost:1@evil.example"),
        "{}",
        text(&output.stderr)
    );
    assert!(
        files(cache.path())
            .iter()
            .all(|path| !path.starts_with("core")),
        "no core installed"
    );
}

/// A release of the core as the GitHub API writes it, keys in its order: the release's own `url`,
/// an `upload_url` with braces in it, the author, then each asset with its `url` and `name` before
/// the nested `uploader`, whose `url` is not an asset's. Asset `name` is asset 7; another target's
/// asset comes before it. Compact as `gh api` gets it, or pretty-printed as curl gets it.
fn github_release_json(server: &Server, name: &str, pretty: bool) -> String {
    let user = |id: u64| {
        serde_json::json!({
            "login": "release-bot",
            "id": id,
            "node_id": "U_kgDOexample",
            "avatar_url": "https://avatars.example.invalid/u/1?v=4",
            "url": server.url("/members/release-bot"),
            "html_url": server.url("/release-bot"),
            "type": "User",
            "site_admin": false
        })
    };
    let asset = |id: u64, file: &str| {
        serde_json::json!({
            "url": server.url(&format!("/repos/ElsiKora/Gate/releases/assets/{id}")),
            "id": id,
            "node_id": "RA_kwDOexample",
            "name": file,
            "label": "",
            "uploader": user(2),
            "content_type": "application/gzip",
            "state": "uploaded",
            "size": 1024,
            "digest": null,
            "download_count": 0,
            "created_at": "2026-11-18T00:00:00Z",
            "updated_at": "2026-11-18T00:00:00Z",
            "browser_download_url": server.url(&format!(
                "/ElsiKora/Gate/releases/download/v{VERSION}/{file}"
            ))
        })
    };
    let release = serde_json::json!({
        "url": server.url("/repos/ElsiKora/Gate/releases/3"),
        "assets_url": server.url("/repos/ElsiKora/Gate/releases/3/assets"),
        "upload_url": server.url("/repos/ElsiKora/Gate/releases/3/assets{?name,label}"),
        "html_url": server.url(&format!("/ElsiKora/Gate/releases/tag/v{VERSION}")),
        "id": 3,
        "author": user(1),
        "node_id": "RE_kwDOexample",
        "tag_name": format!("v{VERSION}"),
        "target_commitish": "main",
        "name": format!("v{VERSION}"),
        "draft": false,
        "immutable": false,
        "prerelease": true,
        "created_at": "2026-11-18T00:00:00Z",
        "published_at": "2026-11-18T00:00:00Z",
        "assets": [asset(6, "gate-other-target.tar.gz"), asset(7, name)],
        "tarball_url": server.url(&format!("/repos/ElsiKora/Gate/tarball/v{VERSION}")),
        "zipball_url": server.url(&format!("/repos/ElsiKora/Gate/zipball/v{VERSION}")),
        "body": format!("Notes, {{\"name\":\"{name}\"}}.\n\nSHA-256:")
    });
    if pretty {
        serde_json::to_string_pretty(&release).expect("json")
    } else {
        release.to_string()
    }
}

/// Serves `archive` as the private core asset `name` of the core's release and returns its download
/// URL: that URL answers 404 without a token; the API answers only `token`, with the release as
/// GitHub writes it (pretty-printed when `pretty`), and the asset redirects to storage that needs
/// no token.
fn private_core(server: &Server, archive: &[u8], name: &str, token: &str, pretty: bool) -> String {
    let download = server.url(&format!(
        "/ElsiKora/Gate/releases/download/v{VERSION}/{name}"
    ));
    server.route(
        concat!(
            "/repos/ElsiKora/Gate/releases/tags/v",
            env!("CARGO_PKG_VERSION")
        ),
        Route::ok(github_release_json(server, name, pretty)).authorized(&format!("Bearer {token}")),
    );
    server.route(
        "/repos/ElsiKora/Gate/releases/assets/7",
        Route::redirect(&server.url("/storage/core")).authorized(&format!("Bearer {token}")),
    );
    server.serve("/storage/core", archive.to_vec());
    download
}

#[test]
fn the_launcher_fetches_a_private_core_with_a_credential_it_never_shows() {
    let Some(target) = host_target() else {
        return;
    };
    let good = "ghp_launcherGood123";
    let wrong = "ghp_launcherWrong456";
    let server = Server::start();
    let archive = core_archive();
    let name = format!("gate-{target}.tar.gz");
    let download = private_core(&server, &archive, &name, good, false);
    let repo = Repo::new();
    repo.write("bin/gate", include_str!("../src/launcher.sh"));
    repo.write(
        "gate.lock",
        &format!(
            "[core]\nversion = \"{VERSION}\"\n\n[core.targets.{target}]\nurl = \"{download}\"\nsha256 = \"{}\"\n",
            sha256(&archive)
        ),
    );
    // A curl that logs its arguments: a token must never be on a command line.
    let arguments = repo.bin.path().join("curl.log");
    let curl = repo.bin.path().join("curl");
    std::fs::write(
        &curl,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >>'{}'\nexec /usr/bin/curl \"$@\"\n",
            arguments.display()
        ),
    )
    .expect("curl");
    executable(&curl);
    let run = |env: &[(&str, &str)]| {
        let cache = tempfile::tempdir().expect("cache");
        let mut command = repo.launcher(cache.path(), &["check"]);
        command
            .env("GATE_GITHUB_API_URL", server.url(""))
            .env("GATE_GITHUB_URL", server.url(""));
        for (key, value) in env {
            command.env(key, value);
        }
        let output = command.output().expect("launcher");
        let mut installed = files(cache.path());
        installed.retain(|path| path.starts_with("core"));
        (output, installed)
    };

    let (output, installed) = run(&[]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stderr));
    let message = text(&output.stderr);
    assert!(message.contains(&name), "{message}");
    assert!(message.contains("GATE_GITHUB_TOKEN"), "{message}");
    assert!(message.contains("ElsiKora/Gate"), "{message}");
    assert!(installed.is_empty(), "nothing installed: {installed:?}");

    let (output, installed) = run(&[("GATE_GITHUB_TOKEN", wrong)]);
    assert_eq!(output.status.code(), Some(2));
    let all = format!("{}{}", text(&output.stdout), text(&output.stderr));
    assert!(all.contains("$GATE_GITHUB_TOKEN"), "{all}");
    assert!(!all.contains(wrong), "the token never shows: {all}");
    assert!(installed.is_empty(), "nothing installed: {installed:?}");

    for env in [
        vec![("GITHUB_TOKEN", good)],
        vec![("GATE_GITHUB_TOKEN", good), ("GITHUB_TOKEN", wrong)],
    ] {
        let (output, installed) = run(&env);
        assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
        assert_eq!(text(&output.stdout), "pinned core ran: check\n");
        assert_eq!(installed.len(), 2, "the core and its sha256: {installed:?}");
    }
    repo.fake_gh(&format!(
        "[ \"$*\" = 'auth token --hostname {}' ] || exit 1\necho {good}\n",
        server.url("").trim_start_matches("http://")
    ));
    let (output, _) = run(&[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "pinned core ran: check\n");

    let logged = std::fs::read_to_string(&arguments).expect("curl log");
    assert!(
        !logged.contains(good) && !logged.contains(wrong),
        "no token on a command line: {logged}"
    );
    for request in server.requests() {
        if request.authorization.is_some() {
            assert!(
                request.path.starts_with("/repos/ElsiKora/Gate/releases/"),
                "a token went to {}",
                request.path
            );
        }
    }
    let storage: Vec<_> = server
        .requests()
        .into_iter()
        .filter(|request| request.path == "/storage/core")
        .collect();
    assert_eq!(storage.len(), 3, "three runs reached the storage");
}

/// GitHub answers curl with pretty-printed JSON, one key per line, and `gh api` with compact JSON:
/// the launcher finds the asset in both, with the system's `awk`.
#[test]
fn the_launcher_finds_a_private_asset_in_compact_and_pretty_release_json() {
    let Some(target) = host_target() else {
        return;
    };
    let token = "ghp_launcherShape789";
    let archive = core_archive();
    let name = format!("gate-{target}.tar.gz");
    for pretty in [false, true] {
        let server = Server::start();
        let download = private_core(&server, &archive, &name, token, pretty);
        let repo = Repo::new();
        repo.write("bin/gate", include_str!("../src/launcher.sh"));
        repo.write(
            "gate.lock",
            &format!(
                "[core]\nversion = \"{VERSION}\"\n\n[core.targets.{target}]\nurl = \"{download}\"\nsha256 = \"{}\"\n",
                sha256(&archive)
            ),
        );
        let cache = tempfile::tempdir().expect("cache");
        let output = repo
            .launcher(cache.path(), &["check"])
            .env("GATE_GITHUB_API_URL", server.url(""))
            .env("GATE_GITHUB_URL", server.url(""))
            .env("GATE_GITHUB_TOKEN", token)
            .output()
            .expect("launcher");
        assert_eq!(
            output.status.code(),
            Some(0),
            "pretty {pretty}: {}",
            text(&output.stderr)
        );
        assert_eq!(
            text(&output.stdout),
            "pinned core ran: check\n",
            "pretty {pretty}"
        );
        let assets: Vec<_> = server
            .requests()
            .into_iter()
            .filter(|request| {
                request
                    .path
                    .starts_with("/repos/ElsiKora/Gate/releases/assets/")
            })
            .map(|request| request.path)
            .collect();
        assert_eq!(
            assets,
            ["/repos/ElsiKora/Gate/releases/assets/7"],
            "pretty {pretty}: only the asset named {name}"
        );
    }
}

/// A repository whose root declares plugin `house` of the private repository acme/rules.
fn private_plugin(server: &Server) -> Repo {
    core_release(server, &core_archive(), Duration::ZERO);
    server.route(
        "/acme/rules/releases/download/plugin-house-v1.2.0/house.wasm",
        Route::status(404),
    );
    server.route(
        "/repos/acme/rules/releases/tags/plugin-house-v1.2.0",
        Route::ok(release_json(&[(
            "house.wasm",
            &server.url("/repos/acme/rules/releases/assets/7"),
            &server.url("/acme/rules/releases/download/plugin-house-v1.2.0/house.wasm"),
            None,
        )]))
        .authorized("Bearer good-credential"),
    );
    server.route(
        "/repos/acme/rules/releases/assets/7",
        Route::redirect(&server.url("/storage/house")).authorized("Bearer good-credential"),
    );
    server.serve("/storage/house", b"\0asm house".to_vec());

    let repo = Repo::new();
    repo.write(
        "gate.toml",
        concat!(
            "[gate]\nversion = \"",
            env!("CARGO_PKG_VERSION"),
            "\"\n\n[plugins]\nhouse = { source = \"acme/rules:house@1.2.0\" }\n"
        ),
    );
    repo.commit();
    repo
}

#[test]
fn a_private_plugin_needs_a_credential_and_the_credential_never_shows() {
    let server = Server::start();
    let repo = private_plugin(&server);

    let output = repo.gate(&["lock"], Some(&server), &[]);
    assert_eq!(output.status.code(), Some(2));
    let message = text(&output.stderr);
    assert!(
        message.contains("plugin-house-v1.2.0 of acme/rules"),
        "{message}"
    );
    assert!(message.contains("GATE_GITHUB_TOKEN"), "{message}");

    let wrong = "wrong-credential-that-must-not-show";
    let output = repo.gate(&["lock"], Some(&server), &[("GATE_GITHUB_TOKEN", wrong)]);
    assert_eq!(output.status.code(), Some(2));
    let all = format!("{}{}", text(&output.stdout), text(&output.stderr));
    assert!(all.contains("$GATE_GITHUB_TOKEN"), "{all}");
    assert!(!all.contains(wrong), "the credential appears: {all}");
    assert!(!repo.path().join("gate.lock").exists(), "no lock written");

    let output = repo.gate(
        &["lock"],
        Some(&server),
        &[("GITHUB_TOKEN", "good-credential")],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(!text(&output.stdout).contains("good-credential"));
    assert!(repo.read("gate.lock").contains(&format!(
        "[plugins.\"acme/rules:house@1.2.0\"]\nurl = \"{}\"\nsha256 = \"{}\"\n",
        server.url("/acme/rules/releases/download/plugin-house-v1.2.0/house.wasm"),
        sha256(b"\0asm house")
    )));

    for request in server.requests() {
        if request.authorization.is_some() {
            assert!(
                request.path.starts_with("/repos/"),
                "a credential went to {}",
                request.path
            );
        }
    }

    std::fs::remove_file(repo.path().join("gate.lock")).expect("remove lock");
    repo.fake_gh("printf 'good-credential\\n'\n");
    let output = repo.gate(&["lock"], Some(&server), &[]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "gh auth token: {}",
        text(&output.stderr)
    );
}

fn pinned(sources: &[(&str, char)]) -> String {
    let mut text = String::from(concat!(
        "[core]\nversion = \"",
        env!("CARGO_PKG_VERSION"),
        "\"\n"
    ));
    for target in TARGETS {
        let _ = write!(
            text,
            "\n[core.targets.{target}]\nurl = \"https://example.invalid/gate-{target}.tar.gz\"\nsha256 = \"{}\"\n",
            "a".repeat(64)
        );
    }
    for (source, digit) in sources {
        let _ = write!(
            text,
            "\n[plugins.\"{source}\"]\nurl = \"https://example.invalid/{digit}.wasm\"\nsha256 = \"{}\"\n",
            digit.to_string().repeat(64)
        );
    }
    text
}

fn with_plugins() -> Repo {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        concat!("[gate]\nversion = \"", env!("CARGO_PKG_VERSION"), "\"\n\n[plugins]\ngit = { source = \"ElsiKora/Gate:git@0.1.0\" }\nhouse = { source = \"acme/rules:house@1.3.0\" }\n"),
    )
    .write(
        "gate.lock",
        &pinned(&[("ElsiKora/Gate:git@0.1.0", 'b'), ("acme/rules:house@1.2.0", 'c')]),
    );
    repo.commit();
    repo
}

#[test]
fn check_fails_when_the_lock_does_not_match_the_configuration() {
    let repo = with_plugins();
    let output = repo.gate(&["check"], None, &[]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        "gate: configuration error: gate.lock does not match the configuration: it does not pin plugin house = acme/rules:house@1.3.0 (gate.toml:6 [plugins]); it pins acme/rules:house@1.2.0, which no gate.toml declares; run gate lock and commit gate.lock\n"
    );
}

#[test]
fn lock_set_moves_exactly_the_one_pin() {
    let repo = with_plugins();
    let before = repo.read("gate.lock");
    let url = "https://github.com/acme/rules/releases/download/plugin-house-v1.3.0/house.wasm";
    let build = tempfile::tempdir().expect("build");
    let file = build.path().join("house.wasm");
    std::fs::write(&file, b"\0asm house 1.3.0").expect("build");
    let hash = sha256(b"\0asm house 1.3.0");

    let output = repo.gate(
        &[
            "lock",
            "--set",
            &format!("house={url}"),
            "--file",
            path(&file),
        ],
        None,
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        format!(
            "gate.lock: pinned house to {url} sha256 {hash} from {}, now in the cache\n",
            file.display()
        )
    );
    let after = repo.read("gate.lock");
    let expected = before
        .replace(
            "[plugins.\"acme/rules:house@1.2.0\"]",
            "[plugins.\"acme/rules:house@1.3.0\"]",
        )
        .replace("https://example.invalid/c.wasm", url)
        .replace(&"c".repeat(64), &hash);
    assert_eq!(
        after,
        format!("# Written by `gate lock`. Commit it; never edit it by hand.\n\n{expected}")
    );

    let output = repo.gate(&["check"], None, &[]);
    assert!(
        !text(&output.stderr).contains("gate.lock"),
        "the moved pin matches: {}",
        text(&output.stderr)
    );
}

fn path(file: &Path) -> &str {
    file.to_str().expect("a UTF-8 path")
}

/// Takes the guard of `repo`'s `gate.lock` as a first run would, before the run under test starts.
fn hold_guard(repo: &Repo) -> std::fs::File {
    let file = std::fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(repo.path().join(".git/gate.lock.guard"))
        .expect("the guard file");
    file.lock().expect("the test takes the guard first");
    file
}

/// Starts `gate` with `arguments` in `repo`; its stderr comes line by line from the receiver.
fn spawn_gate(repo: &Repo, arguments: &[&str]) -> (Child, Receiver<String>) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gate"));
    repo.environment(command.args(arguments), None);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("gate");
    let stderr = child.stderr.take().expect("stderr");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    (child, receiver)
}

/// Whether the run said that it waits for the guard before its stderr ended. Order comes from the
/// run's own output; the minute only turns a hang into a failure.
fn waited(lines: &Receiver<String>) -> bool {
    loop {
        match lines.recv_timeout(Duration::from_secs(60)) {
            Ok(line) if line.starts_with("gate: waiting for another gate run") => return true,
            Ok(_) => {}
            Err(RecvTimeoutError::Disconnected) => return false,
            Err(RecvTimeoutError::Timeout) => {
                panic!("gate neither said it waits nor ended within a minute")
            }
        }
    }
}

const HOUSE_URL: &str =
    "https://github.com/acme/rules/releases/download/plugin-house-v1.3.0/house.wasm";

#[test]
fn two_lock_set_runs_keep_both_pins() {
    let repo = with_plugins();
    let before = repo.read("gate.lock");
    let build = tempfile::tempdir().expect("build");
    let file = build.path().join("house.wasm");
    std::fs::write(&file, b"\0asm house 1.3.0").expect("build");
    let hash = sha256(b"\0asm house 1.3.0");

    // A first run has read gate.lock and holds the guard; the second starts meanwhile.
    let first = hold_guard(&repo);
    let (second, lines) = spawn_gate(
        &repo,
        &[
            "lock",
            "--set",
            &format!("house={HOUSE_URL}"),
            "--file",
            path(&file),
        ],
    );
    let second_waited = waited(&lines);
    // The first run writes what it read with the git plugin moved, then lets go.
    repo.write(
        "gate.lock",
        &before.replace(
            "https://example.invalid/b.wasm",
            "https://example.invalid/moved.wasm",
        ),
    );
    drop(first);
    let output = second.wait_with_output().expect("gate");
    let said: Vec<String> = lines.iter().collect();

    let after = repo.read("gate.lock");
    assert!(
        after.contains("url = \"https://example.invalid/moved.wasm\""),
        "the first run's pin is kept: {after}"
    );
    assert!(
        after.contains(&format!("url = \"{HOUSE_URL}\"\nsha256 = \"{hash}\"\n")),
        "the second run's pin is not lost: {after}"
    );
    assert!(second_waited, "the second run waited: {said:?}");
    assert_eq!(output.status.code(), Some(0), "{said:?}");
}

#[test]
fn a_waiting_lock_set_whose_file_changed_fails_closed() {
    let repo = with_plugins();
    let before = repo.read("gate.lock");
    let build = tempfile::tempdir().expect("build");
    let file = build.path().join("house.wasm");
    std::fs::write(&file, b"\0asm house 1.3.0").expect("build");

    let first = hold_guard(&repo);
    let (second, lines) = spawn_gate(
        &repo,
        &[
            "lock",
            "--set",
            &format!("house={HOUSE_URL}"),
            "--file",
            path(&file),
        ],
    );
    assert!(waited(&lines), "the second run waits for the first");
    // The build changes while the run waits, after it hashed the file.
    std::fs::write(&file, b"\0asm house 1.3.0, rebuilt").expect("rebuild");
    drop(first);
    let output = second.wait_with_output().expect("gate");
    let said = lines.iter().collect::<Vec<_>>().join("\n");

    assert_eq!(output.status.code(), Some(2), "{said}");
    assert!(said.contains("changed while gate read it"), "{said}");
    assert_eq!(repo.read("gate.lock"), before, "gate.lock is unchanged");
}

#[test]
fn a_guard_that_cannot_be_taken_says_what_to_do() {
    let repo = with_plugins();
    let before = repo.read("gate.lock");
    std::fs::create_dir(repo.path().join(".git/gate.lock.guard"))
        .expect("a directory in the guard's place");
    let build = tempfile::tempdir().expect("build");
    let file = build.path().join("house.wasm");
    std::fs::write(&file, b"\0asm house 1.3.0").expect("build");

    let output = repo.gate(
        &[
            "lock",
            "--set",
            &format!("house={HOUSE_URL}"),
            "--file",
            path(&file),
        ],
        None,
        &[],
    );
    let said = text(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{said}");
    assert!(
        said.contains("/.git/gate.lock.guard to update gate.lock: "),
        "{said}"
    );
    assert!(
        said.contains("is writable and on a local file system, or run one gate lock at a time"),
        "{said}"
    );
    assert_eq!(repo.read("gate.lock"), before, "gate.lock is unchanged");
}

/// The entries of the blob cache under `cache`.
fn blobs(cache: &Path) -> Vec<PathBuf> {
    let directory = cache.join("blobs/sha256");
    if !directory.exists() {
        return Vec::new();
    }
    files(&directory)
}

/// A repository whose `gate.toml` turns on `git/commit-message` from the git plugin of this release,
/// pinned in `gate.lock` to `url` with the sha256 `hash`, and committed.
fn git_pinned(url: &str, hash: &str) -> Repo {
    let source = format!("ElsiKora/Gate:git@{VERSION}");
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins]\ngit = {{ source = \"{source}\", options = {{ main = [\"refs/heads/main\"] }} }}\n\n[rules]\n\"git/commit-message\" = \"error\"\n"
        ),
    )
    .write(
        "gate.lock",
        &pinned(&[(&source, 'b')])
            .replace("https://example.invalid/b.wasm", url)
            .replace(&"b".repeat(64), hash),
    );
    repo.commit();
    repo
}

#[test]
fn lock_set_file_admits_the_build_and_check_runs_exactly_it_before_it_is_published() {
    let server = Server::start();
    let wasm = git_component();
    let url = server.url(&format!(
        "/acme/releases/download/plugin-git-v{VERSION}/git.wasm"
    ));
    let repo = git_pinned(&url, &"b".repeat(64));
    let build = tempfile::tempdir().expect("build");
    let file = build.path().join("git.wasm");
    std::fs::write(&file, &wasm).expect("build");
    let hash = sha256(&wasm);

    let output = repo.gate(
        &[
            "lock",
            "--set",
            &format!("git={url}"),
            "--file",
            path(&file),
        ],
        None,
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(
        repo.read("gate.lock")
            .contains(&format!("url = \"{url}\"\nsha256 = \"{hash}\"\n")),
        "{}",
        repo.read("gate.lock")
    );
    let entry = repo.cache.path().join("blobs/sha256").join(&hash);
    assert_eq!(
        std::fs::read(&entry).expect("the build is in the cache"),
        wasm
    );
    repo.commit();

    // The asset is not published: the URL answers 404, so the run can only use the admitted build.
    let output = repo.gate(&["check"], None, &[]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        text(&output.stdout),
        text(&output.stderr)
    );
    assert!(
        text(&output.stdout).contains("gate check (tree): 1 rule, 0 errors"),
        "{}",
        text(&output.stdout)
    );
    assert!(server.requests().is_empty(), "nothing was downloaded");
    server.stop();
}

#[test]
fn another_file_under_the_pinned_sha256_is_refused_and_nothing_runs() {
    let server = Server::start();
    let url = server.url("/acme/releases/download/plugin-git-v9.9.9/git.wasm");
    let repo = git_pinned(&url, &"b".repeat(64));
    let build = tempfile::tempdir().expect("build");
    let file = build.path().join("git.wasm");
    std::fs::write(&file, b"\0asm the build that was pinned").expect("build");
    let output = repo.gate(
        &[
            "lock",
            "--set",
            &format!("git={url}"),
            "--file",
            path(&file),
        ],
        None,
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    repo.commit();
    let hash = sha256(b"\0asm the build that was pinned");
    let entry = repo.cache.path().join("blobs/sha256").join(&hash);
    std::fs::write(&entry, b"\0asm another build").expect("replace the entry");

    let output = repo.gate(&["check"], None, &[]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stderr));
    assert!(
        !text(&output.stdout).contains("gate check"),
        "no rule ran: {}",
        text(&output.stdout)
    );
    assert!(
        !entry.exists(),
        "the entry that is not the pinned file is gone"
    );
    assert!(
        text(&output.stderr).contains(&format!("plugin git = ElsiKora/Gate:git@{VERSION}: "))
            && text(&output.stderr).contains("--file <the built .wasm>"),
        "{}",
        text(&output.stderr)
    );
    server.stop();
}

#[test]
fn a_pin_whose_blob_is_missing_and_cannot_be_downloaded_fails_closed_and_says_what_to_do() {
    let server = Server::start();
    let url = server.url("/acme/releases/download/plugin-git-v9.9.9/git.wasm");
    let repo = git_pinned(&url, &"d".repeat(64));

    let output = repo.gate(&["check"], None, &[]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stderr));
    assert!(
        !text(&output.stdout).contains("gate check"),
        "no rule ran: {}",
        text(&output.stdout)
    );
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains(&format!("plugin git = ElsiKora/Gate:git@{VERSION}: ")),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "; if the asset is not published yet, publish it, or admit the build on this machine with gate lock --set git={url} --file <the built .wasm>"
        )),
        "{stderr}"
    );
    assert!(
        blobs(repo.cache.path()).is_empty(),
        "nothing entered the cache"
    );
    server.stop();
}

#[test]
fn a_refused_download_or_a_missing_credential_gets_no_admit_advice() {
    for status in [401, 403] {
        let server = Server::start();
        let path = "/acme/releases/download/plugin-git-v9.9.9/git.wasm";
        server.route(path, Route::status(status));
        let repo = git_pinned(&server.url(path), &"d".repeat(64));

        let output = repo.gate(&["check"], None, &[]);
        assert_eq!(output.status.code(), Some(2), "{}", text(&output.stderr));
        let stderr = text(&output.stderr);
        assert!(
            stderr.contains(&format!("HTTP {status}")) && !stderr.contains("--file"),
            "{stderr}"
        );
        server.stop();
    }

    // A GitHub release asset refused without a credential may be private: the advice is to set a
    // credential, never to pin a local build.
    let server = Server::start();
    let url = server.url("/acme/rules/releases/download/plugin-git-v9.9.9/git.wasm");
    let repo = git_pinned(&url, &"d".repeat(64));
    let output = repo.gate(&["check"], Some(&server), &[]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stderr));
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains("was refused without a credential (HTTP 404)")
            && !stderr.contains("--file"),
        "{stderr}"
    );
    server.stop();
}

#[test]
fn lock_set_needs_a_readable_file_and_a_declared_name_and_has_no_sha256() {
    let server = Server::start();
    let url = server.url("/acme/releases/download/plugin-git-v9.9.9/git.wasm");
    let repo = git_pinned(&url, &"b".repeat(64));
    let before = repo.read("gate.lock");
    let build = tempfile::tempdir().expect("build");
    let file = build.path().join("git.wasm");
    std::fs::write(&file, b"\0asm a build").expect("build");
    let set = format!("git={url}");

    for arguments in [
        vec!["lock", "--set", &set],
        vec!["lock", "--file", path(&file)],
        vec!["lock", "--set", &set, "--sha256", &"e".repeat(64)],
    ] {
        let output = repo.gate(&arguments, None, &[]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{arguments:?}: {}",
            text(&output.stderr)
        );
    }

    let missing = build.path().join("missing.wasm");
    let output = repo.gate(
        &["lock", "--set", &set, "--file", path(&missing)],
        None,
        &[],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text(&output.stderr).contains(&format!("--file: cannot read {}", missing.display())),
        "{}",
        text(&output.stderr)
    );

    let output = repo.gate(
        &[
            "lock",
            "--set",
            &format!("house={url}"),
            "--file",
            path(&file),
        ],
        None,
        &[],
    );
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stderr));
    assert!(
        text(&output.stderr).contains("house"),
        "{}",
        text(&output.stderr)
    );

    assert_eq!(repo.read("gate.lock"), before, "gate.lock is unchanged");
    assert!(
        blobs(repo.cache.path()).is_empty(),
        "a refused pin admits nothing"
    );
    server.stop();
}

#[test]
fn overrides_are_refused_by_lock_and_by_no_overrides() {
    let repo = with_plugins();
    repo.write("build/house.wasm", "\0asm");

    let output = repo.gate(&["lock", "--plugin", "house=build/house.wasm"], None, &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text(&output.stderr).contains("gate lock refuses --plugin house=build/house.wasm"),
        "{}",
        text(&output.stderr)
    );

    let output = repo.gate(
        &[
            "check",
            "--plugin",
            "house=build/house.wasm",
            "--no-overrides",
        ],
        None,
        &[],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text(&output.stderr).contains("is refused by --no-overrides"),
        "{}",
        text(&output.stderr)
    );
}

#[test]
fn cache_prune_lists_then_removes_only_what_no_lock_pins() {
    let repo = with_plugins();
    let cache = repo.cache.path();
    for (digit, contents) in [('b', "pinned"), ('d', "unpinned")] {
        let path = cache
            .join("blobs/sha256")
            .join(digit.to_string().repeat(64));
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directories");
        std::fs::write(path, contents).expect("write");
    }
    let unpinned = PathBuf::from("blobs/sha256").join("d".repeat(64));

    // Without the tag gate writes on first use, the directory may not be gate's cache.
    let output = repo.gate(&["cache", "prune", "--yes"], None, &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text(&output.stderr).contains("has no CACHEDIR.TAG"),
        "{}",
        text(&output.stderr)
    );
    assert_eq!(files(cache).len(), 2, "nothing removed without the tag");
    std::fs::write(
        cache.join("CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .expect("tag");

    let output = repo.gate(&["cache", "prune"], None, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let listing = text(&output.stdout);
    assert!(
        listing.contains("would remove 1 entry, 8 B:\n"),
        "{listing}"
    );
    assert!(
        listing.contains(&format!("  {}  8 B  no lock pins it\n", unpinned.display())),
        "{listing}"
    );
    assert_eq!(files(cache).len(), 3, "a dry run removes nothing");

    let output = repo.gate(&["cache", "prune", "--yes"], None, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let left = vec![
        PathBuf::from("CACHEDIR.TAG"),
        PathBuf::from("blobs/sha256").join("b".repeat(64)),
    ];
    assert_eq!(files(cache), left);

    // A bound written wrong is refused before anything is listed.
    for (flag, value) in [
        ("--max-size", "2X"),
        ("--max-size", "G"),
        ("--max-size", "1.5G"),
        ("--older-than", "ten"),
        ("--older-than", "+30"),
    ] {
        let output = repo.gate(&["cache", "prune", flag, value], None, &[]);
        assert_eq!(output.status.code(), Some(2), "{flag} {value}");
        let form = if flag == "--max-size" {
            "write a whole number and M or G, such as 512M or 2G"
        } else {
            "is not a number of days; write a whole number, such as 30"
        };
        assert!(
            text(&output.stderr).contains(form),
            "{flag} {value}: {}",
            text(&output.stderr)
        );
    }

    // A pinned download larger than --max-size stays, and prune says why the cache stays over.
    std::fs::write(
        cache.join("blobs/sha256").join("b".repeat(64)),
        vec![b'p'; 2 << 20],
    )
    .expect("a large pinned download");
    let output = repo.gate(
        &[
            "cache",
            "prune",
            "--max-size",
            "1M",
            "--older-than",
            "0",
            "--yes",
        ],
        None,
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(
        text(&output.stdout).contains(
            "over --max-size 1.0 MiB: what is left is pinned downloads, modules/ and files in use, which no bound removes"
        ),
        "{}",
        text(&output.stdout)
    );
    assert_eq!(files(cache), left);
}

/// The git plugin built from this tree for wasm32-wasip2, as its release would carry it.
fn git_component() -> Vec<u8> {
    built::component("gate-plugin-git", "git", "").1.clone()
}

#[test]
fn every_suggestion_of_a_fresh_gate_toml_uncommented_checks_clean() {
    let server = Server::start();
    core_release(&server, &core_archive(), Duration::ZERO);
    let wasm = git_component();
    let tag = format!("plugin-git-v{VERSION}");
    let download = format!("/ElsiKora/Gate/releases/download/{tag}/git.wasm");
    server.serve(&download, wasm.clone());
    server.serve(
        &format!("/repos/ElsiKora/Gate/releases/tags/{tag}"),
        release_json(&[(
            "git.wasm",
            &server.url("/repos/ElsiKora/Gate/releases/assets/9"),
            &server.url(&download),
            Some(&sha256(&wasm)),
        )]),
    );
    let repo = Repo::new();
    let output = repo.gate(&["init"], Some(&server), &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));

    // A newcomer uncomments every suggested line: the tables and the entries in them.
    let fresh = repo.read("gate.toml");
    let uncommented: String = fresh
        .lines()
        .map(|line| match line.strip_prefix("# ") {
            Some(rest) if rest.starts_with('[') || rest.contains(" = ") => rest,
            _ => line,
        })
        .fold(String::new(), |mut text, line| {
            let _ = writeln!(text, "{line}");
            text
        });
    assert!(
        uncommented.contains("\n[plugins]\n") && uncommented.contains("\n[rules]\n"),
        "{uncommented}"
    );
    repo.write("gate.toml", &uncommented);
    // As init says: gate reads tracked files only.
    repo.git(&["add", "gate.toml", "gate.lock", "bin/gate"]);

    let output = repo.gate(&["lock"], Some(&server), &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    repo.commit();
    let output = repo.gate(&["check"], Some(&server), &[]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        text(&output.stdout),
        text(&output.stderr)
    );
    assert!(
        text(&output.stdout).contains("gate check (tree): 2 rules, 0 errors"),
        "every suggested rule ran: {}",
        text(&output.stdout)
    );
    server.stop();
}

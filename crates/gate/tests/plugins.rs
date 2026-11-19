//! `gate check` running real plugins built for wasm32-wasip2: `examples/plugin-hello`, pinned in
//! `gate.lock` and served from the cache, or replaced for one run by `--plugin`; and the runtime's
//! probe, whose history rule reports the range a push run passes.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

use sha2::{Digest as _, Sha256};

/// The version of the core under test.
const VERSION: &str = env!("CARGO_PKG_VERSION");

const SOURCE: &str = "acme/rules:hello@0.1.0";

/// A workspace package built as a component once per test binary: its path and bytes.
fn component(package: &str, library: &str) -> (PathBuf, Vec<u8>) {
    component_with(package, library, "")
}

/// [`component`] with the cargo `features` given (none when empty), in a target directory of
/// their own.
fn component_with(package: &str, library: &str, features: &str) -> (PathBuf, Vec<u8>) {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{package}{features}"));
    let mut arguments = vec![
        "build",
        "--locked",
        "--release",
        "-p",
        package,
        "--target",
        "wasm32-wasip2",
    ];
    if !features.is_empty() {
        arguments.extend(["--features", features]);
    }
    let status = Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .args(&arguments)
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .expect("cargo runs");
    assert!(status.success(), "{package} builds for wasm32-wasip2");
    let path = target.join(format!("wasm32-wasip2/release/{library}.wasm"));
    let bytes = std::fs::read(&path).expect("the component exists");
    (path, bytes)
}

/// The hello component.
fn hello() -> &'static (PathBuf, Vec<u8>) {
    static WASM: OnceLock<(PathBuf, Vec<u8>)> = OnceLock::new();
    WASM.get_or_init(|| component("gate-plugin-hello", "gate_plugin_hello"))
}

/// The runtime's probe component.
fn probe() -> &'static (PathBuf, Vec<u8>) {
    static WASM: OnceLock<(PathBuf, Vec<u8>)> = OnceLock::new();
    WASM.get_or_init(|| component("gate-runtime-probe", "gate_runtime_probe"))
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

struct Repo {
    directory: tempfile::TempDir,
    cache: tempfile::TempDir,
}

impl Repo {
    /// A repository with project `greeter` in `apps/hello`, plugin `hello` declared and pinned
    /// at `url` with `sha256`, and `hello/marker` configured for `apps/*` with `options`.
    fn new(options: &str, url: &str, sha256: &str) -> Self {
        let repo = Self::empty();
        repo.write(
            "gate.toml",
            &format!(
                r#"[gate]
version = "{VERSION}"

[workspace]
projects = ["apps/*"]

[plugins]
hello = {{ source = "{SOURCE}" }}

[[profiles]]
projects = ["apps/*"]

[profiles.rules]
"hello/marker" = {{ level = "error", norm = "RULE-HEL-001", options = {options} }}
"#
            ),
        );
        repo.write("apps/hello/gate.toml", "[project]\nname = \"greeter\"\n");
        repo.write(
            "apps/hello/src/main.rs",
            "fn main() {\n    // FIXME: greet by name\n    println!(\"hello\");\n}\n",
        );
        repo.write("apps/hello/notes.md", "# Notes\n\nNothing is left here.\n");
        repo.write("gate.lock", &lock(SOURCE, url, sha256));
        repo.commit("initial");
        repo
    }

    fn empty() -> Self {
        let repo = Self {
            directory: tempfile::tempdir().expect("temporary directory"),
            cache: tempfile::tempdir().expect("temporary cache"),
        };
        repo.git(&["init", "-q", "-b", "main"]);
        repo
    }

    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
        let output = isolated(
            Command::new("git")
                .arg("-C")
                .arg(self.path())
                .args(["rev-parse", "HEAD"]),
        )
        .output()
        .expect("git");
        text(&output.stdout).trim().to_owned()
    }

    /// The repository with the built plugin pinned and already in the cache.
    fn pinned(options: &str) -> Self {
        let (_, bytes) = hello();
        let sum = sha256(bytes);
        let repo = Self::new(options, "https://example.invalid/hello.wasm", &sum);
        let blob = repo.cache.path().join("blobs/sha256").join(&sum);
        std::fs::create_dir_all(blob.parent().expect("parent")).expect("cache directories");
        std::fs::write(blob, bytes).expect("cache entry");
        repo
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }

    fn write(&self, path: &str, contents: &str) {
        let full = self.path().join(path);
        std::fs::create_dir_all(full.parent().expect("parent")).expect("directories");
        std::fs::write(full, contents).expect("write");
    }

    fn git(&self, arguments: &[&str]) {
        let output = isolated(
            Command::new("git")
                .arg("-C")
                .arg(self.path())
                .args(arguments),
        )
        .output()
        .expect("git");
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn gate(&self, arguments: &[&str]) -> Output {
        isolated(
            Command::new(env!("CARGO_BIN_EXE_gate"))
                .args(arguments)
                .current_dir(self.path())
                .env("GATE_CACHE_DIR", self.cache.path()),
        )
        .output()
        .expect("gate")
    }
}

/// A `gate.lock` for the core under test and one plugin `source` pinned at `url` with `sha256`.
fn lock(source: &str, url: &str, sha256: &str) -> String {
    let mut lock = String::from(concat!(
        "[core]\nversion = \"",
        env!("CARGO_PKG_VERSION"),
        "\"\n"
    ));
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-musl"] {
        let _ = write!(
            lock,
            "\n[core.targets.{target}]\nurl = \"https://example.invalid/gate-{target}.tar.gz\"\nsha256 = \"{}\"\n",
            "a".repeat(64)
        );
    }
    let _ = write!(
        lock,
        "\n[plugins.\"{source}\"]\nurl = \"{url}\"\nsha256 = \"{sha256}\"\n"
    );
    lock
}

fn isolated(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GATE_GITHUB_URL")
        .env_remove("GATE_GITHUB_API_URL")
        .env_remove("GATE_GITHUB_TOKEN")
        .env_remove("GITHUB_TOKEN")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

#[test]
fn the_pinned_plugin_reports_its_finding_with_norm_level_and_target() {
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    let output = repo.gate(&["check", "--format", "json"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout)).expect("json");
    assert_eq!(report["rules"], 1, "{report:#}");
    assert_eq!(report["errors"], 1, "{report:#}");
    let findings = report["findings"].as_array().expect("findings");
    assert_eq!(findings.len(), 1, "{report:#}");
    let finding = &findings[0];
    assert_eq!(finding["rule"], "hello/marker");
    assert_eq!(finding["level"], "error");
    assert_eq!(finding["norm"], "RULE-HEL-001");
    assert_eq!(finding["project"], "greeter");
    assert_eq!(finding["target"], "src/main.rs:2");
    assert_eq!(finding["message"], "the line holds the marker FIXME");
    assert_eq!(finding["location"]["path"], "apps/hello/src/main.rs");
    assert_eq!(finding["location"]["span"]["line"], 2);
    assert!(
        finding["help"]
            .as_str()
            .is_some_and(|help| help.starts_with("finish the work")),
        "{finding:#}"
    );
}

#[test]
fn an_override_replaces_the_pin_for_one_run_and_the_report_names_it() {
    let (path, bytes) = hello();
    let repo = Repo::new(
        r#"{ marker = "FIXME" }"#,
        "https://example.invalid/hello.wasm",
        &"b".repeat(64),
    );
    let argument = format!("hello={}", path.display());
    let output = repo.gate(&["check", "--plugin", &argument]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    assert!(
        stdout.contains("error[hello/marker] apps/hello/src/main.rs:2"),
        "{stdout}"
    );
    assert!(stdout.contains("  norm: RULE-HEL-001\n"), "{stdout}");
    assert!(
        stdout.contains(&format!(
            "; override hello={} sha256 {}",
            path.display(),
            sha256(bytes)
        )),
        "{stdout}"
    );
}

#[test]
fn options_failing_the_schema_fail_the_run_naming_file_and_key() {
    let repo = Repo::pinned("{ marker = 5 }");
    let output = repo.gate(&["check"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    let message = text(&output.stderr);
    assert!(message.contains("gate.toml"), "{message}");
    assert!(message.contains("hello/marker"), "{message}");
    assert!(message.contains("marker"), "{message}");
    assert!(message.contains('5'), "{message}");

    let repo = Repo::pinned(r#"{ marker = "FIXME", colour = "red" }"#);
    let output = repo.gate(&["check"]);
    assert_eq!(output.status.code(), Some(2));
    let message = text(&output.stderr);
    assert!(message.contains("colour"), "{message}");
}

#[test]
fn a_pinned_plugin_that_cannot_be_fetched_fails_closed() {
    let repo = Repo::new(
        r#"{ marker = "FIXME" }"#,
        "http://127.0.0.1:9/hello.wasm",
        &"b".repeat(64),
    );
    let output = repo.gate(&["check"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    let message = text(&output.stderr);
    assert!(message.starts_with("gate: environment error:"), "{message}");
    assert!(message.contains("hello.wasm"), "{message}");
}

#[test]
fn a_pinned_plugin_whose_cached_bytes_differ_is_not_run() {
    let (_, bytes) = hello();
    let repo = Repo::new(
        r#"{ marker = "FIXME" }"#,
        "http://127.0.0.1:9/hello.wasm",
        &sha256(bytes),
    );
    let blob = repo.cache.path().join("blobs/sha256").join(sha256(bytes));
    std::fs::create_dir_all(blob.parent().expect("parent")).expect("cache directories");
    let mut tampered = bytes.clone();
    tampered.push(0);
    std::fs::write(&blob, tampered).expect("cache entry");
    let output = repo.gate(&["check"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    assert_eq!(text(&output.stdout), "");
}

/// Two commits in project `one` with the probe declared and `probe/history` on: the base and tip
/// of a push.
fn history() -> (Repo, String, String) {
    let repo = Repo::empty();
    let source = "acme/rules:probe@0.1.0";
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[workspace]\nprojects = [\"apps/*\"]\n\n[plugins]\nprobe = {{ source = \"{source}\" }}\n\n[[profiles]]\nprojects = [\"apps/*\"]\n\n[profiles.rules]\n\"probe/history\" = \"error\"\n"
        ),
    );
    repo.write("apps/one/gate.toml", "[project]\nname = \"one\"\n");
    repo.write("apps/one/a.txt", "first\n");
    repo.write(
        "gate.lock",
        &lock(
            source,
            "https://example.invalid/probe.wasm",
            &"b".repeat(64),
        ),
    );
    let base = repo.commit("first");
    repo.write("apps/one/a.txt", "second\n");
    let tip = repo.commit("second");
    (repo, base, tip)
}

/// The probe's `range: ..` finding of a JSON report.
fn range_message(output: &Output) -> String {
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout)).expect("json");
    report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter_map(|finding| finding["message"].as_str())
        .find(|message| message.starts_with("range: "))
        .unwrap_or_else(|| panic!("no range finding: {report:#}"))
        .to_owned()
}

#[test]
fn a_history_rule_sees_the_target_ref_with_the_flag_and_none_without_it() {
    let (repo, base, tip) = history();
    let plugin = format!("probe={}", probe().0.display());
    let range = format!("{base}..{tip}");

    let output = repo.gate(&[
        "check",
        "--push",
        &range,
        "--target-ref",
        "refs/heads/main",
        "--plugin",
        &plugin,
        "--format",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    assert_eq!(
        range_message(&output),
        format!("range: {base}..{tip} to refs/heads/main")
    );

    let output = repo.gate(&[
        "check", "--push", &range, "--plugin", &plugin, "--format", "json",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    assert_eq!(
        range_message(&output),
        format!("range: {base}..{tip} to none")
    );

    let output = repo.gate(&[
        "check",
        "--push",
        &range,
        "--target-ref",
        "main",
        "--plugin",
        &plugin,
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text(&output.stderr).contains("\"main\" is not a full ref"),
        "{}",
        text(&output.stderr)
    );

    let output = repo.gate(&["check", "--rev", &tip, "--target-ref", "refs/heads/main"]);
    assert_eq!(output.status.code(), Some(2), "--target-ref needs --push");
    assert!(
        text(&output.stderr).contains("--push"),
        "{}",
        text(&output.stderr)
    );
}

/// A repository with projects `one` (`apps/one`) and `two` (`apps/two`), the probe declared, and
/// `rules` (lines of `[profiles.rules]`) on for `apps/one`. Nothing is committed yet.
fn probe_repo(rules: &str) -> Repo {
    probe_repo_for("apps/one", rules)
}

/// [`probe_repo`] whose profile with `rules` covers `project` instead of `apps/one`.
fn probe_repo_for(project: &str, rules: &str) -> Repo {
    let repo = Repo::empty();
    let source = "acme/rules:probe@0.1.0";
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[workspace]\nprojects = [\"apps/*\"]\n\n[plugins]\nprobe = {{ source = \"{source}\" }}\n\n[[profiles]]\nprojects = [\"{project}\"]\n\n[profiles.rules]\n{rules}\n"
        ),
    );
    repo.write("apps/one/gate.toml", "[project]\nname = \"one\"\n");
    repo.write("apps/two/gate.toml", "[project]\nname = \"two\"\n");
    repo.write(
        "gate.lock",
        &lock(
            source,
            "https://example.invalid/probe.wasm",
            &"b".repeat(64),
        ),
    );
    repo
}

/// `gate check` with the built probe, as JSON: its exit code and finding messages, sorted.
fn probe_check(repo: &Repo, arguments: &[&str]) -> (Option<i32>, Vec<String>) {
    let plugin = format!("probe={}", probe().0.display());
    let mut all = vec!["check", "--plugin", &plugin, "--format", "json"];
    all.extend_from_slice(arguments);
    let output = repo.gate(&all);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("json: {}", text(&output.stderr)));
    let mut messages: Vec<String> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter_map(|finding| finding["message"].as_str().map(str::to_owned))
        .collect();
    messages.sort();
    (output.status.code(), messages)
}

#[test]
fn history_questions_answer_from_real_git_within_the_grant() {
    let repo = probe_repo(
        r#""probe/git" = { level = "error", options = { read_at = [["HEAD", "a.txt"], ["HEAD~1", "a.txt"], ["HEAD", "../two/b.txt"]], changed_paths = [["HEAD~1", "HEAD"]], merge_tree = [["main", "side"], ["main", "feature"]], is_ancestor = [["HEAD~1", "HEAD"], ["HEAD", "HEAD~1"]], log = "HEAD~1..HEAD" } }"#,
    );
    repo.write("apps/one/a.txt", "first\n");
    repo.write("apps/two/b.txt", "secret\n");
    repo.commit("first");
    repo.git(&["branch", "side"]);
    repo.git(&["branch", "feature"]);
    repo.write("apps/one/a.txt", "second\n");
    repo.write("apps/two/b.txt", "secret two\n");
    let message = "second\u{1e}record\u{1f}field";
    repo.commit(message);
    repo.git(&["checkout", "-q", "side"]);
    repo.write("apps/one/c.txt", "side\n");
    repo.commit("side");
    repo.git(&["checkout", "-q", "feature"]);
    repo.write("apps/one/a.txt", "feature\n");
    repo.commit("feature");
    repo.git(&["checkout", "-q", "main"]);

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    let mut expected = vec![
        "read-at HEAD:a.txt: second".to_owned(),
        "read-at HEAD~1:a.txt: first".to_owned(),
        "read-at HEAD:../two/b.txt: none".to_owned(),
        "changed-paths HEAD~1 HEAD: a.txt".to_owned(),
        "merge-tree main side: clean".to_owned(),
        "merge-tree main feature: conflict".to_owned(),
        "is-ancestor HEAD~1 HEAD: true".to_owned(),
        "is-ancestor HEAD HEAD~1: false".to_owned(),
        format!("log: {:?}", format!("{message}\n")),
    ];
    expected.sort();
    assert_eq!(messages, expected);
}

#[test]
fn sync_state_answers_what_sync_produces_at_a_commit_and_what_differs() {
    let repo = probe_repo(
        r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~2", "HEAD~1", "HEAD"] } }"#,
    );
    repo.write(
        "apps/one/gate.toml",
        "[project]\nname = \"one\"\n\n[generate]\n\"probe/write\" = { options = { files = { \"gen/a.md\" = \"a\\n\", \"gen/b.md\" = \"b\\n\" } } }\n",
    );
    repo.write("apps/one/gen/a.md", "a\n");
    repo.write("apps/one/gen/b.md", "b\n");
    repo.write("apps/one/notes.md", "written by hand\n");
    repo.commit("in sync");
    repo.write("apps/one/gen/a.md", "edited by hand\n");
    repo.commit("drift in one file");
    repo.write("apps/one/gen/a.md", "a\n");
    repo.write("apps/one/gen/old.md", "no generator writes this\n");
    repo.commit("a stale owned file");

    // A hand-written file (notes.md) is not produced; a stale owned file is, and drifts.
    let expected = [
        "sync-state HEAD: produced gen/a.md,gen/b.md,gen/old.md; drift gen/old.md",
        "sync-state HEAD~1: produced gen/a.md,gen/b.md; drift gen/a.md",
        "sync-state HEAD~2: produced gen/a.md,gen/b.md; drift none",
    ];
    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(messages, expected);

    let answers = repo.cache.path().join("git").join("sync-state");
    let stored = std::fs::read_dir(&answers)
        .expect("sync-state answers are stored")
        .flat_map(|directory| std::fs::read_dir(directory.expect("entry").path()).expect("shard"))
        .count();
    assert_eq!(
        stored,
        3,
        "one answer per commit under {}",
        answers.display()
    );
    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages, expected,
        "the stored answers give the same result"
    );
}

#[test]
fn sync_state_holds_the_cores_hook_and_ci_files_and_what_sync_would_remove() {
    let repo = Repo::empty();
    let source = "acme/rules:probe@0.1.0";
    let action = "0123456789abcdef0123456789abcdef01234567";
    let configuration = |hooks: &str| {
        format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins]\nprobe = {{ source = \"{source}\" }}\n\n[generate]\n\"probe/write\" = {{ options = {{ files = {{ \"gen/a.md\" = \"a\\n\" }} }} }}\n\n[rules]\n\"probe/git\" = {{ level = \"error\", options = {{ sync_state = [\"HEAD~1\", \"HEAD\"] }} }}\n\n[hooks]\n{hooks}\n\n[ci.github]\nworkflow = \".github/workflows/gate.yml\"\ncheckout = \"actions/checkout@{action}\"\n\n[ci.github.jobs.lint]\nname = \"Lint\"\nrun = [\"check\"]\n"
        )
    };
    repo.write(
        "gate.toml",
        &configuration(r#"pre-commit = ["check --staged"]"#),
    );
    repo.write(
        "gate.lock",
        &lock(
            source,
            "https://example.invalid/probe.wasm",
            &"b".repeat(64),
        ),
    );
    // A single-project root: the rule's grant is the whole repository, hooks and CI included.
    repo.write("notes.md", "written by hand\n");
    repo.git(&["add", "-A"]);
    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["sync", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    // The commits below must not run the hooks gate sync just installed.
    repo.git(&["config", "--unset", "core.hooksPath"]);
    repo.commit("hooks, workflow and outputs in sync");
    repo.write(
        "gate.toml",
        &configuration(r#"pre-push = ["check --push"]"#),
    );
    repo.commit("the hook moves to pre-push without gate sync");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        [
            "sync-state HEAD: produced .githooks/pre-commit,.githooks/pre-push,.github/workflows/gate.yml,gen/a.md; drift .githooks/pre-commit,.githooks/pre-push",
            "sync-state HEAD~1: produced .githooks/pre-commit,.github/workflows/gate.yml,gen/a.md; drift none",
        ]
    );
}

/// `apps/one/gate.toml` with a `probe/write` entry of these options.
fn writing(options: &str) -> String {
    writing_in("one", options)
}

/// The `gate.toml` of project `name` with a `probe/write` entry of these options.
fn writing_in(name: &str, options: &str) -> String {
    format!(
        "[project]\nname = \"{name}\"\n\n[generate]\n\"probe/write\" = {{ options = {{ {options} }} }}\n"
    )
}

#[test]
fn sync_state_before_the_move_to_gate_uses_the_runs_configuration_and_never_fails_the_call() {
    let repo = probe_repo(
        r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~3", "HEAD~2", "HEAD~1", "HEAD"] } }"#,
    );
    let adopted = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    let files = r#"files = { "gen/a.md" = "a\n", "gen/b.md" = "b\n" }"#;

    repo.write(
        "gate.toml",
        concat!(
            "[gate]\nversion = \"",
            env!("CARGO_PKG_VERSION"),
            "\"\n\n[agents]\nrules = \"docs/rules\"\n"
        ),
    );
    repo.write("apps/one/gate.toml", "[project]\nname = \"one\"\n");
    repo.write("apps/one/gen/a.md", "old\n");
    repo.commit("old format: the run's configuration answers");

    repo.write("gate.toml", &adopted);
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"files = { "gen/a.md" = "v1\n" }"#),
    );
    repo.write("apps/one/gen/a.md", "v1\n");
    repo.commit("a valid, different configuration answers for itself");

    repo.write(
        "apps/one/gate.toml",
        &writing(&format!("fail = true, {files}")),
    );
    repo.commit("the generator fails");

    repo.write("apps/one/gate.toml", &writing(files));
    repo.write("apps/one/gen/a.md", "a\n");
    repo.write("apps/one/gen/b.md", "b\n");
    repo.commit("moved to Gate and in sync");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        [
            "sync-state HEAD: produced gen/a.md,gen/b.md; drift none",
            "sync-state HEAD~1: produced gen/a.md,gen/b.md; drift gen/a.md,gen/b.md",
            "sync-state HEAD~2: produced gen/a.md; drift none",
            "sync-state HEAD~3: produced gen/a.md,gen/b.md; drift none",
        ],
        "before the move to Gate gen/a.md differs and gen/b.md is missing, and only paths count; \
         with its own configuration a commit still drifts"
    );
}

#[test]
fn a_generator_failing_under_the_fallback_answers_its_paths_as_produced_only() {
    let repo =
        probe_repo(r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1"] } }"#);
    let adopted = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    repo.write(
        "gate.toml",
        concat!(
            "[gate]\nversion = \"",
            env!("CARGO_PKG_VERSION"),
            "\"\n\n[agents]\nrules = \"docs/rules\"\n"
        ),
    );
    // Under the run's configuration the generator writes what gen.list names: here a core file,
    // which sync refuses.
    repo.write("apps/one/gen.list", "gate.toml\n");
    repo.commit("old format: the run's configuration answers, and its generator fails");
    repo.write("gate.toml", &adopted);
    repo.write("apps/one/gate.toml", &writing(r#"list = "gen.list""#));
    repo.write("apps/one/gen.list", "gen/a.md\n");
    repo.write("apps/one/gen/a.md", "listed\n");
    repo.commit("moved to Gate: it writes what gen.list names");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        ["sync-state HEAD~1: produced gen/a.md; drift none"]
    );
}

/// The `sync-state` answers stored in `repo`'s cache.
fn stored_answers(repo: &Repo) -> usize {
    let answers = repo.cache.path().join("git").join("sync-state");
    std::fs::read_dir(&answers).map_or(0, |shards| {
        shards
            .flat_map(|shard| std::fs::read_dir(shard.expect("entry").path()).expect("shard"))
            .count()
    })
}

#[test]
fn a_failing_generators_answer_follows_the_runs_configuration() {
    let repo =
        probe_repo(r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1"] } }"#);
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"fail = true, files = { "gen/a.md" = "a\n" }"#),
    );
    repo.commit("the generator fails");
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"files = { "gen/a.md" = "a\n" }"#),
    );
    repo.write("apps/one/gen/a.md", "a\n");
    repo.commit("it writes gen/a.md");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        ["sync-state HEAD~1: produced gen/a.md; drift gen/a.md"]
    );
    assert_eq!(
        stored_answers(&repo),
        0,
        "a generator's failure may pass: the answer holds for this run only"
    );

    // The run's configuration now writes gen/c.md: the answer at the same commit follows it.
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"files = { "gen/c.md" = "c\n" }"#),
    );
    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        ["sync-state HEAD~1: produced gen/c.md; drift gen/c.md"]
    );
}

#[test]
fn a_generator_failing_in_one_project_answers_only_that_projects_paths() {
    let repo = probe_repo_for(
        "apps/two",
        r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1"] } }"#,
    );
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"fail = true, files = { "gen/a.md" = "a\n" }"#),
    );
    repo.write(
        "apps/two/gate.toml",
        &writing_in("two", r#"files = { "gen/a.md" = "a\n" }"#),
    );
    repo.write("apps/two/gen/a.md", "a\n");
    repo.commit("one fails, two is in sync");
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"files = { "gen/a.md" = "a\n" }"#),
    );
    repo.write("apps/one/gen/a.md", "a\n");
    repo.commit("both write gen/a.md");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        ["sync-state HEAD~1: produced gen/a.md; drift none"]
    );
}

#[test]
fn a_generator_whose_output_sync_refuses_answers_its_paths_without_failing() {
    let repo =
        probe_repo(r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1"] } }"#);
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"files = { "gen/a.md" = "a\n", "gate.toml" = "overwritten\n" }"#),
    );
    repo.write("apps/one/gen/a.md", "a\n");
    repo.commit("the generator writes a core file");
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"files = { "gen/a.md" = "a\n" }"#),
    );
    repo.commit("it writes gen/a.md only");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        ["sync-state HEAD~1: produced gen/a.md; drift gen/a.md"]
    );
    assert_eq!(
        stored_answers(&repo),
        2,
        "a refused output is the configuration's: the answer is stored"
    );
}

#[test]
fn a_run_dependent_answer_follows_the_files_the_runs_generators_read() {
    let repo =
        probe_repo(r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1"] } }"#);
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"list = "gen.list", files = { "gate.toml" = "overwritten\n" }"#),
    );
    repo.write("apps/one/gen.list", "gen/a.md\n");
    repo.commit("the generator writes a core file, which sync refuses");
    repo.write("apps/one/gate.toml", &writing(r#"list = "gen.list""#));
    repo.write("apps/one/gen/a.md", "listed\n");
    repo.commit("it writes what gen.list names");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        ["sync-state HEAD~1: produced gen/a.md; drift gen/a.md"]
    );
    assert_eq!(stored_answers(&repo), 2);

    // The run's configuration is the same; the file its generator reads is not.
    repo.write("apps/one/gen.list", "gen/b.md\n");
    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        ["sync-state HEAD~1: produced gen/b.md; drift gen/b.md"]
    );
    assert_eq!(
        stored_answers(&repo),
        3,
        "a new answer under the new outputs"
    );
}

#[test]
fn a_commit_whose_gate_lock_does_not_match_its_gate_toml_falls_back() {
    let repo =
        probe_repo(r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1"] } }"#);
    let adopted = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    repo.write(
        "gate.toml",
        &adopted.replace(
            "[plugins]\n",
            "[plugins]\nother = { source = \"acme/rules:other@0.2.0\" }\n",
        ),
    );
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"files = { "gen/a.md" = "a\n" }"#),
    );
    repo.write("apps/one/gen/a.md", "a\n");
    repo.commit("a plugin added to gate.toml without gate lock");
    repo.write("gate.toml", &adopted);
    repo.commit("the plugin removed again");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        ["sync-state HEAD~1: produced gen/a.md; drift none"]
    );
    assert_eq!(
        stored_answers(&repo),
        2,
        "the commit's entry points at the run's answer"
    );
}

/// Puts a component this core cannot load in `repo`'s cache (fetched and verified: the cache
/// holds it under its pin) and returns `toml` and `lock` with plugin `other` declared and pinned
/// to it.
fn with_unloadable_other(repo: &Repo, toml: &str, lock: &str) -> (String, String) {
    with_other(repo, b"a component of another interface", toml, lock)
}

/// Puts `bytes` in `repo`'s cache as a fetched and verified component and returns `toml` and
/// `lock` with plugin `other` declared and pinned to it.
fn with_other(repo: &Repo, bytes: &[u8], toml: &str, lock: &str) -> (String, String) {
    let checksum = sha256(bytes);
    let blobs = repo.cache.path().join("blobs").join("sha256");
    std::fs::create_dir_all(&blobs).expect("cache");
    std::fs::write(blobs.join(&checksum), bytes).expect("blob");
    let other = "acme/rules:other@0.2.0";
    (
        toml.replace(
            "[plugins]\n",
            &format!("[plugins]\nother = {{ source = \"{other}\" }}\n"),
        ),
        format!(
            "{lock}\n[plugins.\"{other}\"]\nurl = \"https://example.invalid/other.wasm\"\nsha256 = \"{checksum}\"\n"
        ),
    )
}

#[test]
fn a_pinned_component_that_does_not_load_under_this_core_answers_without_failing() {
    let repo =
        probe_repo(r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1"] } }"#);
    let adopted = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    let pinned = std::fs::read_to_string(repo.path().join("gate.lock")).expect("gate.lock");
    let (toml, lock) = with_unloadable_other(&repo, &adopted, &pinned);
    repo.write("gate.toml", &toml);
    repo.write("gate.lock", &lock);
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"files = { "gen/a.md" = "a\n" }"#),
    );
    repo.write("apps/one/gen/a.md", "edited by hand\n");
    repo.commit("a plugin this core cannot load, and the probe's output edited by hand");
    repo.write("gate.toml", &adopted);
    repo.write("gate.lock", &pinned);
    repo.commit("back to the probe only");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        ["sync-state HEAD~1: produced gen/a.md; drift gen/a.md"],
        "the commit's own configuration answers, not the fallback: the plugins that load still \
         judge their files"
    );
    assert_eq!(
        stored_answers(&repo),
        1,
        "a component that does not load under this core fails the same way next run: stored"
    );
}

#[test]
fn a_component_whose_describe_fails_in_a_way_that_may_pass_fails_the_call() {
    let repo =
        probe_repo(r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1"] } }"#);
    let adopted = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    let pinned = std::fs::read_to_string(repo.path().join("gate.lock")).expect("gate.lock");
    let (_, trapping) =
        component_with("gate-runtime-probe", "gate_runtime_probe", "describe-traps");
    let (toml, lock) = with_other(&repo, &trapping, &adopted, &pinned);
    repo.write("gate.toml", &toml);
    repo.write("gate.lock", &lock);
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"files = { "gen/a.md" = "a\n" }"#),
    );
    repo.write("apps/one/gen/a.md", "a\n");
    repo.commit("a plugin whose describe traps");
    repo.write("gate.toml", &adopted);
    repo.write("gate.lock", &pinned);
    repo.commit("back to the probe only");

    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["check", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let message = text(&output.stderr);
    assert!(message.starts_with("gate: environment error:"), "{message}");
    assert!(
        message.contains(
            "plugin other, which this commit's gate.lock pins, did not load in a way that may pass"
        ),
        "{message}"
    );
    assert_eq!(
        stored_answers(&repo),
        0,
        "a trap may pass: nothing is stored, and the plugin is not marked unloadable"
    );
}

#[test]
fn a_commit_with_an_unloadable_plugin_is_still_validated_against_the_plugins_that_load() {
    let repo =
        probe_repo(r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1"] } }"#);
    let adopted = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    let pinned = std::fs::read_to_string(repo.path().join("gate.lock")).expect("gate.lock");
    let (toml, lock) = with_unloadable_other(&repo, &adopted, &pinned);
    // probe/needs on without its required option: the commit's configuration does not validate
    // against the probe, which loads, so the run's configuration answers.
    repo.write(
        "gate.toml",
        &toml.replace(
            "[profiles.rules]\n",
            "[profiles.rules]\n\"probe/needs\" = \"error\"\n",
        ),
    );
    repo.write("gate.lock", &lock);
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"files = { "gen/a.md" = "a\n" }"#),
    );
    repo.write("apps/one/gen/a.md", "a\n");
    repo.commit("an unloadable plugin and an invalid probe rule");
    repo.write("gate.toml", &adopted);
    repo.write("gate.lock", &pinned);
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"files = { "gen/b.md" = "b\n" }"#),
    );
    repo.git(&["rm", "-q", "apps/one/gen/a.md"]);
    repo.write("apps/one/gen/b.md", "b\n");
    repo.commit("the run's configuration writes gen/b.md");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        ["sync-state HEAD~1: produced gen/a.md,gen/b.md; drift none"],
        "the run's configuration answers (gen/b.md, and gen/a.md under the glob it owns), not \
         the commit's own, which would answer gen/a.md alone"
    );
}

#[test]
fn a_commit_whose_plugin_options_fail_the_plugins_schema_falls_back_to_the_runs_configuration() {
    let repo =
        probe_repo(r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1"] } }"#);
    let adopted = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    // The commit's [plugins.probe] options carry a key the probe's schema does not have.
    repo.write(
        "gate.toml",
        &adopted.replace(
            "probe = { source = \"acme/rules:probe@0.1.0\" }",
            "probe = { source = \"acme/rules:probe@0.1.0\", options = { colour = \"red\" } }",
        ),
    );
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"files = { "gen/a.md" = "a\n" }"#),
    );
    repo.write("apps/one/gen/a.md", "a\n");
    repo.commit("plugin options the probe does not declare");
    repo.write("gate.toml", &adopted);
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"files = { "gen/b.md" = "b\n" }"#),
    );
    repo.git(&["rm", "-q", "apps/one/gen/a.md"]);
    repo.write("apps/one/gen/b.md", "b\n");
    repo.commit("the run's configuration writes gen/b.md");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        ["sync-state HEAD~1: produced gen/a.md,gen/b.md; drift none"],
        "the run's configuration answers (gen/b.md, and gen/a.md under the glob it owns), not \
         the commit's own, which would answer gen/a.md alone"
    );
}

#[test]
fn the_ci_workflow_answers_as_drifting_when_a_component_of_the_commit_does_not_load() {
    let repo = Repo::empty();
    let source = "acme/rules:probe@0.1.0";
    let action = "0123456789abcdef0123456789abcdef01234567";
    let toml = format!(
        "[gate]\nversion = \"{VERSION}\"\n\n[plugins]\nprobe = {{ source = \"{source}\" }}\n\n[generate]\n\"probe/write\" = {{ options = {{ files = {{ \"gen/a.md\" = \"a\\n\" }} }} }}\n\n[rules]\n\"probe/git\" = {{ level = \"error\", options = {{ sync_state = [\"HEAD~1\"] }} }}\n\n[ci.github]\nworkflow = \".github/workflows/gate.yml\"\ncheckout = \"actions/checkout@{action}\"\n\n[ci.github.jobs.lint]\nname = \"Lint\"\nrun = [\"check\"]\n"
    );
    let pinned = lock(
        source,
        "https://example.invalid/probe.wasm",
        &"b".repeat(64),
    );
    repo.write("gate.toml", &toml);
    repo.write("gate.lock", &pinned);
    repo.git(&["add", "-A"]);
    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["sync", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    // The workflow and gen/a.md are in sync; the commit adds a plugin this core cannot load.
    let (with_other, locked) = with_unloadable_other(&repo, &toml, &pinned);
    repo.write("gate.toml", &with_other);
    repo.write("gate.lock", &locked);
    repo.commit("a plugin this core cannot load");
    repo.write("gate.toml", &toml);
    repo.write("gate.lock", &pinned);
    repo.commit("back to the probe only");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        [
            "sync-state HEAD~1: produced .github/workflows/gate.yml,gen/a.md; drift .github/workflows/gate.yml"
        ],
        "which jobs read history depends on the plugin that did not load"
    );
}

#[test]
fn a_generator_that_breaks_the_interface_fails_sync_as_an_environment_error_like_check() {
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        concat!("[gate]\nversion = \"", env!("CARGO_PKG_VERSION"), "\"\n\n[plugins]\nprobe = { source = \"acme/rules:probe@0.1.0\" }\n\n[generate]\n\"probe/write\" = { options = { files = { \"gate.toml\" = \"overwritten\\n\" } } }\n"),
    );
    repo.commit("a generator that writes a core file");
    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["sync", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let message = text(&output.stderr);
    assert!(
        message.starts_with("gate: environment error:") && message.contains("probe"),
        "a failure of the plugin, not of the configuration: {message}"
    );
}

#[test]
fn what_a_generator_reports_during_sync_fails_the_run_as_a_gate_sync_finding() {
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        concat!("[gate]\nversion = \"", env!("CARGO_PKG_VERSION"), "\"\n\n[plugins]\nprobe = { source = \"acme/rules:probe@0.1.0\" }\n\n[generate]\n\"probe/write\" = { options = { files = { \"gen/a.md\" = \"a\\n\" }, report = \"the template is out of date\" } }\n"),
    );
    repo.commit("a generator that reports while it writes");
    let plugin = format!("probe={}", probe().0.display());
    let finding = "error[gate/sync#probe/write] gate.toml: the template is out of date";

    let output = repo.gate(&["sync", "--plugin", &plugin]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(stdout.contains(finding), "{stdout}");
    assert!(
        stdout.contains("wrote gen/a.md"),
        "the files are written all the same: {stdout}"
    );

    repo.git(&["add", "-A"]);
    let output = repo.gate(&["sync", "--check", "--plugin", &plugin]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(stdout.contains(finding), "{stdout}");
    assert!(
        stdout.contains("0 out of date, 1 reported by generators"),
        "{stdout}"
    );
}

#[test]
fn a_plugin_the_commit_pins_and_cannot_fetch_fails_the_call_naming_it() {
    let repo =
        probe_repo(r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1"] } }"#);
    let adopted = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    let pinned = std::fs::read_to_string(repo.path().join("gate.lock")).expect("gate.lock");
    let other = "acme/rules:other@0.2.0";
    repo.write(
        "gate.toml",
        &adopted.replace(
            "[plugins]\n",
            &format!("[plugins]\nother = {{ source = \"{other}\" }}\n"),
        ),
    );
    repo.write(
        "gate.lock",
        &format!(
            "{pinned}\n[plugins.\"{other}\"]\nurl = \"http://127.0.0.1:9/other.wasm\"\nsha256 = \"{}\"\n",
            "c".repeat(64)
        ),
    );
    repo.commit("a valid configuration with a plugin that cannot be fetched");
    repo.write("gate.toml", &adopted);
    repo.write("gate.lock", &pinned);
    repo.commit("back to the probe only");

    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["check", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let message = text(&output.stderr);
    assert!(message.contains("git.sync-state at commit"), "{message}");
    assert!(message.contains("cannot be fetched"), "{message}");
    assert!(
        message.contains(&format!("plugin other = {other}")),
        "{message}"
    );
    assert!(message.contains("--plugin <name>=<path.wasm>"), "{message}");
}

#[test]
fn a_new_pin_in_gate_lock_does_not_reuse_answers_of_the_runs_configuration() {
    let repo =
        probe_repo(r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1"] } }"#);
    let adopted = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    repo.write(
        "gate.toml",
        concat!(
            "[gate]\nversion = \"",
            env!("CARGO_PKG_VERSION"),
            "\"\n\n[agents]\nrules = \"docs/rules\"\n"
        ),
    );
    repo.commit("old format");
    repo.write("gate.toml", &adopted);
    repo.commit("moved to Gate");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(messages, ["sync-state HEAD~1: produced none; drift none"]);
    assert_eq!(
        stored_answers(&repo),
        2,
        "the commit's entry and the run's answer"
    );
    probe_check(&repo, &[]);
    assert_eq!(stored_answers(&repo), 2, "the same pins reuse the answer");

    let repinned = std::fs::read_to_string(repo.path().join("gate.lock"))
        .expect("gate.lock")
        .replace(&"b".repeat(64), &"d".repeat(64));
    repo.write("gate.lock", &repinned);
    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        stored_answers(&repo),
        3,
        "a new pin is a new configuration hash"
    );
}

#[test]
fn sync_takes_a_local_build_of_an_unpinned_plugin_for_one_run() {
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        concat!("[gate]\nversion = \"", env!("CARGO_PKG_VERSION"), "\"\n\n[plugins]\nprobe = { source = \"acme/rules:probe@0.1.0\" }\n\n[generate]\n\"probe/write\" = { options = { files = { \"gen/a.md\" = \"a\\n\" } } }\n"),
    );
    repo.commit("a generator, no gate.lock");
    let plugin = format!("probe={}", probe().0.display());

    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    assert!(
        text(&output.stderr).contains("gate.lock"),
        "{}",
        text(&output.stderr)
    );

    let output = repo.gate(&["sync", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(
        text(&output.stdout).contains("; override probe="),
        "{}",
        text(&output.stdout)
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("gen/a.md")).expect("written"),
        "a\n"
    );
    repo.commit("synced");

    let output = repo.gate(&["sync", "--check", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stdout));

    repo.write(
        "gate.toml",
        &std::fs::read_to_string(repo.path().join("gate.toml"))
            .expect("gate.toml")
            .replace("\"a\\n\"", "\"b\\n\""),
    );
    let output = repo.gate(&["sync", "--check", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stdout));
    assert!(
        text(&output.stdout).contains("gen/a.md"),
        "{}",
        text(&output.stdout)
    );
}

#[test]
fn the_lock_check_skips_a_plugin_an_override_covers_in_check_run_and_sync() {
    // A plugin declared and never locked: there is no gate.lock at all.
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        concat!("[gate]\nversion = \"", env!("CARGO_PKG_VERSION"), "\"\n\n[plugins]\nprobe = { source = \"acme/rules:probe@0.1.0\" }\n\n[rules]\n\"probe/norms\" = { level = \"error\", norm = \"N-1\" }\n\n[generate]\n\"probe/write\" = { options = { files = { \"gen/a.md\" = \"a\\n\" } } }\n\n[steps]\nhello = { run = \"echo hello\" }\n"),
    );
    repo.write("gen/a.md", "a\n");
    repo.commit("a plugin and no gate.lock");
    let plugin = format!("probe={}", probe().0.display());

    for command in [&["check"][..], &["sync", "--check"][..]] {
        let output = repo.gate(command);
        assert_eq!(output.status.code(), Some(2), "{command:?}");
        assert!(
            text(&output.stderr).contains("gate.lock"),
            "{command:?}: {}",
            text(&output.stderr)
        );
        let mut overridden = command.to_vec();
        overridden.extend(["--plugin", &plugin]);
        let output = repo.gate(&overridden);
        assert!(
            !text(&output.stderr).contains("gate.lock"),
            "{overridden:?}: {}",
            text(&output.stderr)
        );
    }
    let output = repo.gate(&["check", "--plugin", &plugin]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "the rule ran: {}",
        text(&output.stdout)
    );
    let output = repo.gate(&["sync", "--check", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stdout));

    // gate run loads no plugin and takes no --plugin: its steps run with the lock as it is.
    let output = repo.gate(&["run", "hello"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));

    // The override covers its own plugin only: a second plugin, neither overridden nor pinned,
    // still fails the lock check.
    let toml = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    repo.write(
        "gate.toml",
        &toml.replace(
            "[plugins]\n",
            "[plugins]\nhello = { source = \"acme/rules:hello@0.1.0\" }\n",
        ),
    );
    repo.commit("a second plugin, not pinned");
    for command in [&["check"][..], &["sync", "--check"][..]] {
        let mut overridden = command.to_vec();
        overridden.extend(["--plugin", &plugin]);
        let output = repo.gate(&overridden);
        assert_eq!(output.status.code(), Some(2), "{overridden:?}");
        let message = text(&output.stderr);
        assert!(message.contains("gate.lock"), "{overridden:?}: {message}");
    }
}

#[test]
fn a_rule_a_preset_enables_with_its_required_option_can_be_turned_off() {
    let with_preset = |rules: &str| {
        let repo = probe_repo(rules);
        let toml = std::fs::read_to_string(repo.path().join("gate.toml"))
            .expect("gate.toml")
            .replace(
                "projects = [\"apps/one\"]\n",
                "projects = [\"apps/one\"]\nextends = [\"probe/needs\"]\n",
            );
        repo.write("gate.toml", &toml);
        repo.commit("the preset enables probe/needs");
        repo
    };

    let (code, messages) = probe_check(&with_preset(""), &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(messages, ["needs from the preset"]);

    let (code, messages) = probe_check(&with_preset(r#""probe/needs" = "off""#), &[]);
    assert_eq!(code, Some(0), "{messages:?}");
    assert_eq!(messages, Vec::<String>::new());
}

#[test]
fn a_rule_turned_off_through_its_family_needs_no_options() {
    // A preset enables probe/needs without its required option, the root gives it a norm only,
    // and the project turns the whole family off: the rule does not run, so nothing is missing.
    let repo = probe_repo(r#""probe/needs" = { norm = "N-1" }"#);
    let toml = std::fs::read_to_string(repo.path().join("gate.toml"))
        .expect("gate.toml")
        .replace(
            "projects = [\"apps/one\"]\n",
            "projects = [\"apps/one\"]\nextends = [\"probe/needs-level\"]\n",
        );
    repo.write("gate.toml", &toml);
    repo.write(
        "apps/one/gate.toml",
        "[project]\nname = \"one\"\n\n[rules]\n\"probe/*\" = \"off\"\n",
    );
    repo.commit("the family is off in the project");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(0), "{messages:?}");
    assert_eq!(messages, Vec::<String>::new());

    // Without the family off, the preset's rule runs and its option is missing.
    repo.write("apps/one/gate.toml", "[project]\nname = \"one\"\n");
    repo.commit("the family is on again");
    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["check", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    assert!(
        text(&output.stderr).contains("\"probe/needs\" options"),
        "{}",
        text(&output.stderr)
    );
}

#[test]
fn a_preset_sets_aspect_levels_that_the_projects_own_levels_override() {
    // The project extends probe/quiet, which makes the aspect quiet of probe/aspects a warning.
    let run = |rules: &str| {
        let repo = probe_repo(rules);
        let toml = std::fs::read_to_string(repo.path().join("gate.toml"))
            .expect("gate.toml")
            .replace(
                "projects = [\"apps/one\"]\n",
                "projects = [\"apps/one\"]\nextends = [\"probe/quiet\"]\n",
            );
        repo.write("gate.toml", &toml);
        repo.commit("the preset sets an aspect level");
        let plugin = format!("probe={}", probe().0.display());
        let output = repo.gate(&["check", "--plugin", &plugin, "--format", "json"]);
        let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
            .unwrap_or_else(|_| panic!("json: {}", text(&output.stderr)));
        let mut levels: Vec<(String, String)> = report["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .map(|finding| {
                (
                    finding["message"].as_str().unwrap_or_default().to_owned(),
                    finding["level"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        levels.sort();
        (output.status.code(), levels)
    };
    let pair = |message: &str, level: &str| (message.to_owned(), level.to_owned());

    // The preset enables the rule; the aspect it names is a warning, the other the default error.
    let (code, levels) = run("");
    assert_eq!(code, Some(1), "{levels:?}");
    assert_eq!(
        levels,
        [
            pair("aspect loud", "error"),
            pair("aspect quiet", "warning")
        ]
    );

    // The project's own configuration wins: a rule-wide level applies to every aspect, the
    // preset's too.
    let (_, levels) = run(r#""probe/aspects" = "note""#);
    assert_eq!(
        levels,
        [pair("aspect loud", "note"), pair("aspect quiet", "note")]
    );

    // A level of the aspect itself wins over the rule-wide one.
    let (_, levels) = run(r#""probe/aspects" = { level = "note", levels = { quiet = "error" } }"#);
    assert_eq!(
        levels,
        [pair("aspect loud", "note"), pair("aspect quiet", "error")]
    );

    // Off on the rule turns every aspect off, the preset's too.
    let (code, levels) = run(r#""probe/aspects" = "off""#);
    assert_eq!(code, Some(0), "{levels:?}");
    assert_eq!(levels, Vec::<(String, String)>::new());

    // Where [rules] gives the rule no level, the preset's aspect level holds: with the other
    // aspect off, its warning alone does not fail the run.
    let (code, levels) = run(r#""probe/aspects" = { levels = { loud = "off" } }"#);
    assert_eq!(code, Some(0), "{levels:?}");
    assert_eq!(levels, [pair("aspect quiet", "warning")]);
}

#[test]
fn a_call_carries_the_norms_of_every_configured_rule_of_every_plugin() {
    let repo = probe_repo(
        r#""probe/norms" = { level = "error", norm = "N-probe" }
"hello/marker" = { level = "off", norm = "N-hello" }"#,
    );
    let toml = std::fs::read_to_string(repo.path().join("gate.toml"))
        .expect("gate.toml")
        .replace(
            "[plugins]\n",
            "[plugins]\nhello = { source = \"acme/rules:hello@0.1.0\" }\n",
        );
    repo.write("gate.toml", &toml);
    repo.commit("two plugins, one rule each");

    let probe = format!("probe={}", probe().0.display());
    let hello = format!("hello={}", hello().0.display());
    let output = repo.gate(&[
        "check", "--plugin", &probe, "--plugin", &hello, "--format", "json",
    ]);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("json: {}", text(&output.stderr)));
    let messages: Vec<&str> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter_map(|finding| finding["message"].as_str())
        .collect();
    assert_eq!(
        messages,
        ["norms: N-hello,N-probe"],
        "{}",
        text(&output.stderr)
    );
}

#[test]
fn a_rev_without_a_base_ranges_from_its_parent_and_a_root_commit_is_a_new_branch() {
    let (repo, base, tip) = history();
    let plugin = format!("probe={}", probe().0.display());
    for (rev, expected) in [
        (&tip, format!("range: {base}..{tip} to none")),
        (&base, format!("range: none..{base} to none")),
    ] {
        let output = repo.gate(&[
            "check", "--rev", rev, "--plugin", &plugin, "--format", "json",
        ]);
        assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
        assert_eq!(range_message(&output), expected);
    }
}

#[test]
fn head_is_the_commit_of_the_snapshot_under_rev() {
    let (repo, base, tip) = history();
    repo.git(&["checkout", "-q", "-b", "other", &base]);
    let (code, messages) = probe_check(&repo, &["--rev", &tip]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert!(
        messages.contains(&format!("head: {tip}")),
        "head names the --rev commit, not the checked-out {base}: {messages:?}"
    );
}

#[test]
fn reads_by_absolute_backslashed_or_recased_paths_are_absent() {
    let repo = probe_repo(
        r#""probe/read" = { level = "error", options = { paths = ["a.txt", "./a.txt", "A.TXT", "/apps/one/a.txt", "/a.txt", '..\two\b.txt', "../TWO/b.txt", "../two/b.txt"] } }"#,
    );
    repo.write("apps/one/a.txt", "first\n");
    repo.write("apps/two/b.txt", "secret\n");
    repo.commit("first");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    let mut expected = vec![
        "a.txt: first\n",
        "./a.txt: first\n",
        "A.TXT: none",
        "/apps/one/a.txt: none",
        "/a.txt: none",
        "..\\two\\b.txt: none",
        "../TWO/b.txt: none",
        "../two/b.txt: none",
    ];
    expected.sort_unstable();
    assert_eq!(messages, expected);
}

#[test]
fn stdout_past_the_limit_is_dropped_and_the_run_goes_on() {
    let repo = probe_repo(r#""probe/flood" = { level = "error", options = { stdout = 2048 } }"#);
    repo.commit("first");
    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(messages, ["done"]);
}

/// The canonical agent rule file of the shared-plugins repository below.
const AGENT_RULE: &str =
    "---\ntitle: API rules\narea: API\napplies: always\n---\n\n# API\n\nBody.\n";

/// What `agents/adapters` writes for [`AGENT_RULE`] whose body is `body`.
fn adapter(body: &str) -> String {
    format!(
        "<!-- Generated by gate sync from docs/agents/rules/api.md; edit the canonical file. -->\n\n# API\n\n{body}\n"
    )
}

#[test]
fn main_history_judges_generated_content_only_after_the_move_to_gate_through_sync_state() {
    static GIT: OnceLock<(PathBuf, Vec<u8>)> = OnceLock::new();
    static AGENTS: OnceLock<(PathBuf, Vec<u8>)> = OnceLock::new();
    let git = GIT.get_or_init(|| component("gate-plugin-git", "git"));
    let agents = AGENTS.get_or_init(|| component("gate-plugin-agents", "agents"));

    let repo = Repo::empty();
    // Before the move to Gate: a configuration this core does not read, so sync-state answers the
    // run's own produced paths and no drift; content another tool wrote is not judged.
    repo.write("gate.toml", "[checks]\nformat = \"before-gate\"\n");
    repo.write("AGENTS.md", "# Instructions\n");
    repo.write("docs/agents/rules/api.md", AGENT_RULE);
    repo.write(".claude/rules/api.md", &adapter("Body."));
    let start = repo.commit("chore: start");
    repo.write(".claude/rules/api.md", "edited by hand\n");
    let edited = repo.commit("docs: edit the adapter by hand");
    repo.write(
        "docs/agents/rules/api.md",
        &AGENT_RULE.replace("Body.", "Body, revised."),
    );
    repo.write(".claude/rules/api.md", &adapter("Body, revised."));
    repo.commit("docs: revise the rule and its adapter together");

    // The move to Gate: this core's configuration, then a hand edit it reads itself.
    repo.write(
        "gate.toml",
        &format!(
            r#"[gate]
version = "{VERSION}"

[project]
name = "synthetic"

[plugins.agents]
source = "acme/rules:agents@0.1.0"

[plugins.git]
source = "acme/rules:git@0.1.0"
options = {{ activation = "{start}", main = ["refs/heads/main"], release-subject = '^chore\(release\): (\d+\.\d+\.\d+)$', branch-pattern = "^feature/[a-z0-9-]+$", branch-length = [5, 50], documentation = ["docs/**", "*.md"], dependabot = ["package.json"] }}

[rules]
"git/main-history" = {{ level = "error" }}

[generate]
"agents/adapters" = {{}}
"#
        ),
    );
    let mut pins = lock(
        "acme/rules:agents@0.1.0",
        "https://example.invalid/agents.wasm",
        &"c".repeat(64),
    );
    let _ = write!(
        pins,
        "\n[plugins.\"acme/rules:git@0.1.0\"]\nurl = \"https://example.invalid/git.wasm\"\nsha256 = \"{}\"\n",
        "d".repeat(64)
    );
    repo.write("gate.lock", &pins);
    let adopted = repo.commit("chore: move the configuration to Gate");
    repo.write(".claude/rules/api.md", "edited by hand again\n");
    let again = repo.commit("docs: edit the adapter by hand again");
    // A stale file: under the adapters' owned glob, written by no generator.
    repo.write(".claude/rules/stale.md", "no canonical rule file\n");
    let stale = repo.commit("docs: add an adapter by hand");

    let agents_plugin = format!("agents={}", agents.0.display());
    let git_plugin = format!("git={}", git.0.display());
    let output = repo.gate(&[
        "check",
        "--plugin",
        &agents_plugin,
        "--plugin",
        &git_plugin,
        "--format",
        "json",
    ]);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("json: {}", text(&output.stderr)));
    let mut messages: Vec<String> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter_map(|finding| finding["message"].as_str().map(str::to_owned))
        .collect();
    messages.sort();
    let short = |commit: &str| commit[..12].to_owned();
    let mut expected = vec![
        format!(
            "direct commit {} changes .claude/rules/api.md, which differs from gate sync at that commit",
            short(&again)
        ),
        format!(
            "direct commit {} changes .claude/rules/stale.md, which differs from gate sync at that commit",
            short(&stale)
        ),
        format!(
            "direct commit {} changes code path gate.lock; code reaches main through a branch merge",
            short(&adopted)
        ),
        format!(
            "direct commit {} changes code path gate.toml; code reaches main through a branch merge",
            short(&adopted)
        ),
    ];
    expected.sort();
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    assert!(
        messages
            .iter()
            .all(|message| !message.contains(&short(&edited))),
        "a hand edit before the move to Gate is not judged: {messages:?}"
    );
    assert_eq!(messages, expected);
}

#[test]
fn rule_enforcement_sees_a_norm_mapped_on_another_plugins_rule() {
    static GIT: OnceLock<(PathBuf, Vec<u8>)> = OnceLock::new();
    static AGENTS: OnceLock<(PathBuf, Vec<u8>)> = OnceLock::new();
    let git = GIT.get_or_init(|| component("gate-plugin-git", "git"));
    let agents = AGENTS.get_or_init(|| component("gate-plugin-agents", "agents"));

    let repo = Repo::empty();
    repo.write("README.md", "# Synthetic\n");
    let start = repo.commit("chore: start");
    // RULE-GIT-001 is the norm of a `git/*` rule; RULE-GIT-002 is the norm of no rule.
    repo.write(
        "docs/agents/rules/git.md",
        "---\ntitle: Git rules\narea: GIT\napplies: always\n---\n\n# Git\n\n\
         ## RULE-GIT-001 No CI skip\n\nEnforcement: gate · Status: stable\n\nText.\n\n\
         ## RULE-GIT-002 Unmapped\n\nEnforcement: gate · Status: stable\n\nText.\n",
    );
    repo.write(
        "gate.toml",
        &format!(
            r#"[gate]
version = "{VERSION}"

[project]
name = "synthetic"

[plugins.agents]
source = "acme/rules:agents@0.1.0"

[plugins.git]
source = "acme/rules:git@0.1.0"
options = {{ activation = "{start}", main = ["refs/heads/main"], release-subject = '^chore\(release\): (\d+\.\d+\.\d+)$' }}

[rules]
"agents/rule-enforcement" = {{ level = "error" }}
"git/no-ci-skip" = {{ level = "error", norm = "RULE-GIT-001" }}
"#
        ),
    );
    let mut pins = lock(
        "acme/rules:agents@0.1.0",
        "https://example.invalid/agents.wasm",
        &"c".repeat(64),
    );
    let _ = write!(
        pins,
        "\n[plugins.\"acme/rules:git@0.1.0\"]\nurl = \"https://example.invalid/git.wasm\"\nsha256 = \"{}\"\n",
        "d".repeat(64)
    );
    repo.write("gate.lock", &pins);
    repo.commit("chore: configure gate");

    let agents_plugin = format!("agents={}", agents.0.display());
    let git_plugin = format!("git={}", git.0.display());
    let output = repo.gate(&[
        "check",
        "--plugin",
        &agents_plugin,
        "--plugin",
        &git_plugin,
        "--format",
        "json",
    ]);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("json: {}", text(&output.stderr)));
    let findings: Vec<(String, String)> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| {
            (
                finding["rule"].as_str().unwrap_or_default().to_owned(),
                finding["message"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    assert_eq!(
        findings,
        [(
            "agents/rule-enforcement".to_owned(),
            "RULE-GIT-002 declares gate enforcement, but no check in gate.toml reports it"
                .to_owned()
        )]
    );
}

#[test]
fn main_history_allows_generated_files_that_gate_sync_removes_or_renames_with_their_sources() {
    static GIT: OnceLock<(PathBuf, Vec<u8>)> = OnceLock::new();
    static AGENTS: OnceLock<(PathBuf, Vec<u8>)> = OnceLock::new();
    let git = GIT.get_or_init(|| component("gate-plugin-git", "git"));
    let agents = AGENTS.get_or_init(|| component("gate-plugin-agents", "agents"));
    let agents_plugin = format!("agents={}", agents.0.display());
    let git_plugin = format!("git={}", git.0.display());
    let sync = |repo: &Repo| {
        repo.git(&["add", "-A"]);
        let output = repo.gate(&["sync", "--plugin", &agents_plugin, "--plugin", &git_plugin]);
        assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
        // The commits below must not run the hooks gate sync installs; git exits 5 when it
        // installed none.
        let unset = isolated(Command::new("git").arg("-C").arg(repo.path()).args([
            "config",
            "--unset-all",
            "core.hooksPath",
        ]))
        .output()
        .expect("git");
        assert!(matches!(unset.status.code(), Some(0 | 5)), "{unset:?}");
    };

    let repo = Repo::empty();
    repo.write("README.md", "# Synthetic\n");
    let start = repo.commit("chore: start");
    repo.write(
        "gate.toml",
        &format!(
            r#"[gate]
version = "{VERSION}"

[project]
name = "synthetic"

[plugins.agents]
source = "acme/rules:agents@0.1.0"

[plugins.git]
source = "acme/rules:git@0.1.0"
options = {{ activation = "{start}", main = ["refs/heads/main"], release-subject = '^chore\(release\): (\d+\.\d+\.\d+)$', branch-pattern = "^feature/[a-z0-9-]+$", branch-length = [5, 50], documentation = ["docs/**", "*.md"], dependabot = ["package.json"] }}

[rules]
"git/main-history" = {{ level = "error" }}

[generate]
"agents/adapters" = {{}}
"#
        ),
    );
    let mut pins = lock(
        "acme/rules:agents@0.1.0",
        "https://example.invalid/agents.wasm",
        &"c".repeat(64),
    );
    let _ = write!(
        pins,
        "\n[plugins.\"acme/rules:git@0.1.0\"]\nurl = \"https://example.invalid/git.wasm\"\nsha256 = \"{}\"\n",
        "d".repeat(64)
    );
    repo.write("gate.lock", &pins);
    repo.write("AGENTS.md", "# Instructions\n");
    repo.write("docs/agents/rules/api.md", AGENT_RULE);
    repo.write(
        "docs/agents/rules/old.md",
        &AGENT_RULE
            .replace("API rules", "Old rules")
            .replace("# API", "# Old"),
    );
    sync(&repo);
    let configured = repo.commit("chore: configure gate and its generated files");
    let adapter = |name: &str| repo.path().join(".claude/rules").join(name).is_file();
    assert!(
        adapter("api.md") && adapter("old.md"),
        "gate sync wrote the adapters"
    );

    // The source goes; gate sync deletes its adapter.
    repo.git(&["rm", "-q", "docs/agents/rules/old.md"]);
    sync(&repo);
    let deleted = repo.commit("docs: retire the old rule");
    assert!(!adapter("old.md"), "gate sync deleted the old adapter");
    // The source is renamed; gate sync renames its adapter.
    repo.git(&[
        "mv",
        "docs/agents/rules/api.md",
        "docs/agents/rules/interface.md",
    ]);
    sync(&repo);
    let renamed = repo.commit("docs: rename the API rule file");
    assert!(
        !adapter("api.md") && adapter("interface.md"),
        "gate sync renamed the adapter"
    );

    let output = repo.gate(&[
        "check",
        "--plugin",
        &agents_plugin,
        "--plugin",
        &git_plugin,
        "--format",
        "json",
    ]);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("json: {}", text(&output.stderr)));
    let mut messages: Vec<String> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter_map(|finding| finding["message"].as_str().map(str::to_owned))
        .collect();
    messages.sort();
    let short = |commit: &str| commit[..12].to_owned();
    // Only the configuration itself is code; the generated files come and go with their sources.
    let expected = vec![
        format!(
            "direct commit {} changes code path gate.lock; code reaches main through a branch merge",
            short(&configured)
        ),
        format!(
            "direct commit {} changes code path gate.toml; code reaches main through a branch merge",
            short(&configured)
        ),
    ];
    assert_eq!(messages, expected, "{deleted} {renamed}");
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
}

#[test]
fn a_generate_key_naming_no_generator_of_its_plugin_fails_every_command_before_anything_runs() {
    let repo = Repo::empty();
    let configuration = |generator: &str| {
        format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins]\nprobe = {{ source = \"acme/rules:probe@0.1.0\" }}\n\n[generate]\n\"probe/{generator}\" = {{ options = {{ files = {{ \"gen/a.md\" = \"a\\n\" }} }} }}\n"
        )
    };
    let plugin = format!("probe={}", probe().0.display());

    repo.write("gate.toml", &configuration("counts"));
    repo.commit("a generator the plugin does not have");
    for command in [&["check"][..], &["sync"], &["sync", "--check"]] {
        let arguments: Vec<&str> = command
            .iter()
            .copied()
            .chain(["--plugin", &plugin])
            .collect();
        let output = repo.gate(&arguments);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{command:?}: {}",
            text(&output.stdout)
        );
        assert!(
            text(&output.stderr).contains(
                "gate: configuration error: gate.toml:8 [generate]: \"probe/counts\": plugin probe has no generator counts; its generators are probe/write, probe/toml"
            ),
            "{command:?}: {}",
            text(&output.stderr)
        );
    }
    assert!(!repo.path().join("gen/a.md").exists(), "sync wrote nothing");

    repo.write("gate.toml", &configuration("write"));
    repo.commit("a generator the plugin has");
    let output = repo.gate(&["sync", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    repo.commit("synced");
    for command in [&["check"][..], &["sync", "--check"]] {
        let arguments: Vec<&str> = command
            .iter()
            .copied()
            .chain(["--plugin", &plugin])
            .collect();
        let output = repo.gate(&arguments);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{command:?}: {}{}",
            text(&output.stdout),
            text(&output.stderr)
        );
    }
}

#[test]
fn plugin_options_failing_the_plugins_schema_fail_every_command_before_anything_runs() {
    let repo = Repo::empty();
    let configuration = |options: &str| {
        format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins]\nprobe = {{ source = \"acme/rules:probe@0.1.0\", options = {options} }}\n\n[rules]\n\"probe/read\" = {{ level = \"warning\" }}\n\n[generate]\n\"probe/write\" = {{ options = {{ files = {{ \"gen/a.md\" = \"a\\n\" }} }} }}\n"
        )
    };
    let plugin = format!("probe={}", probe().0.display());

    for (options, expected) in [
        (
            r#"{ label = "x", colour = "red" }"#,
            "'colour' was unexpected",
        ),
        ("{ label = 5 }", "/label: 5 is not of type"),
    ] {
        repo.write("gate.toml", &configuration(options));
        repo.commit(options);
        for command in [&["check"][..], &["sync"], &["sync", "--check"]] {
            let arguments: Vec<&str> = command
                .iter()
                .copied()
                .chain(["--plugin", &plugin])
                .collect();
            let output = repo.gate(&arguments);
            let message = text(&output.stderr);
            assert_eq!(
                output.status.code(),
                Some(2),
                "{command:?} {options}: {message}"
            );
            assert_eq!(text(&output.stdout), "", "{command:?} {options}");
            assert!(
                message.contains("gate: configuration error: gate.toml:")
                    && message.contains("plugin probe options: ")
                    && message.contains(expected),
                "{command:?} {options}: {message}"
            );
        }
        assert!(!repo.path().join("gen/a.md").exists(), "sync wrote nothing");
    }

    repo.write("gate.toml", &configuration(r#"{ label = "x" }"#));
    repo.commit("valid plugin options");
    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(0), "{messages:?}");
    assert!(messages.contains(&"label x".to_owned()), "{messages:?}");
}

#[test]
fn a_missing_git_plugin_option_is_a_configuration_error_at_load() {
    static GIT: OnceLock<(PathBuf, Vec<u8>)> = OnceLock::new();
    let git = GIT.get_or_init(|| component("gate-plugin-git", "git"));
    let plugin = format!("git={}", git.0.display());
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        concat!("[gate]\nversion = \"", env!("CARGO_PKG_VERSION"), "\"\n\n[plugins.git]\nsource = \"acme/rules:git@0.1.0\"\noptions = { release-subject = '^chore\\(release\\): (\\d+\\.\\d+\\.\\d+)$' }\n\n[rules]\n\"git/no-ci-skip\" = { level = \"error\" }\n"),
    );
    repo.commit("no main");
    let output = repo.gate(&["check", "--plugin", &plugin]);
    let message = text(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{message}");
    assert_eq!(text(&output.stdout), "", "nothing ran");
    assert!(
        message.contains("gate: configuration error: gate.toml:")
            && message.contains("plugin git options: ")
            && message.contains("\"main\" is a required property"),
        "{message}"
    );
}

/// A repository with the git plugin built from this tree, for checking `[plugins.git] options`.
struct GitOptions {
    repo: Repo,
    plugin: String,
    /// The repository's first commit, an activation commit.
    root: String,
}

const RELEASE_SUBJECT: &str = r"release-subject = '^chore\(release\): (\d+\.\d+\.\d+)$'";
const NO_CI_SKIP: &str = "\"git/no-ci-skip\" = { level = \"error\" }";
const BRANCH_NAME: &str =
    "\"git/branch-name\" = { level = \"error\", options = { pattern = \"^main$\" } }";

impl GitOptions {
    fn new() -> Self {
        static GIT: OnceLock<(PathBuf, Vec<u8>)> = OnceLock::new();
        let git = GIT.get_or_init(|| component("gate-plugin-git", "git"));
        let repo = Repo::empty();
        repo.write("README.md", "# r\n");
        let root = repo.commit("init");
        Self {
            repo,
            plugin: format!("git={}", git.0.display()),
            root,
        }
    }

    /// `gate check` with `arguments` on a committed configuration of `[plugins.git] options`
    /// `main` and then `options`, and `[rules]` `rules`.
    fn check(&self, options: &str, rules: &str, arguments: &[&str]) -> Output {
        self.repo.write(
            "gate.toml",
            &format!(
                "[gate]\nversion = \"{VERSION}\"\n\n[plugins.git]\nsource = \"acme/rules:git@0.1.0\"\noptions = {{ main = [\"refs/heads/main\"]{options} }}\n\n[rules]\n{rules}\n"
            ),
        );
        self.repo.git(&["add", "-A"]);
        self.repo
            .git(&["commit", "-q", "--allow-empty", "-m", "configure"]);
        let mut all = vec!["check", "--plugin", self.plugin.as_str()];
        all.extend_from_slice(arguments);
        self.repo.gate(&all)
    }

    fn refused(&self, options: &str, rules: &str, arguments: &[&str], expected: &str) {
        let output = self.check(options, rules, arguments);
        let message = text(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{options}: {message}");
        assert_eq!(text(&output.stdout), "", "nothing ran");
        assert!(
            message.starts_with("gate: configuration error: gate.toml:")
                && message.contains(expected),
            "{options}: {message}"
        );
    }

    fn passes(&self, options: &str, rules: &str, arguments: &[&str]) {
        let output = self.check(options, rules, arguments);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{options}: {}{}",
            text(&output.stdout),
            text(&output.stderr)
        );
    }
}

#[test]
fn a_git_option_an_enabled_rule_reads_is_a_configuration_error_at_load_when_missing() {
    let git = GitOptions::new();
    // A key every run of the rule reads, named with the rule and what to do.
    git.refused(
        &format!(", {RELEASE_SUBJECT}"),
        NO_CI_SKIP,
        &[],
        "[plugins.git] options has no activation, which \"git/no-ci-skip\" reads: the rule reads \
         history after it; write the 40-hex id of the commit the policy starts after",
    );
    // A key the default forms read: they hold a release form.
    let activation = format!(", activation = \"{}\"", git.root);
    git.refused(
        &activation,
        NO_CI_SKIP,
        &[],
        "[plugins.git] options has no release-subject, which \"git/no-ci-skip\" reads",
    );
    // The keys of main-history's default merge and direct forms.
    git.refused(
        &format!("{activation}, {RELEASE_SUBJECT}"),
        "\"git/main-history\" = \"error\"",
        &[],
        "[plugins.git] options has no branch-pattern, which \"git/main-history\" reads",
    );
    git.refused(
        &format!(
            "{activation}, {RELEASE_SUBJECT}, branch-pattern = '^feature/.+$', branch-length = [5, 50], dependabot = [\"package.json\"]"
        ),
        "\"git/main-history\" = \"error\"",
        &[],
        "[plugins.git] options has no documentation, which \"git/main-history\" reads",
    );
    // A rule the run selects with --rule is checked.
    git.refused(
        "",
        &format!("{NO_CI_SKIP}\n{BRANCH_NAME}"),
        &["--rule", "git/no-ci-skip"],
        "has no activation, which \"git/no-ci-skip\" reads",
    );
}

#[test]
fn a_git_option_no_enabled_rule_reads_may_be_missing() {
    let git = GitOptions::new();
    // Forms without a release form read no release-subject.
    git.passes(
        &format!(
            ", activation = \"{}\", forms = [{{ kind = \"direct\", parents = 1, checks = [] }}]",
            git.root
        ),
        NO_CI_SKIP,
        &[],
    );
    // A rule that does not read a key does not need it, nor does a rule the run leaves out or
    // one that is off.
    git.passes("", BRANCH_NAME, &[]);
    git.passes(
        "",
        &format!("{NO_CI_SKIP}\n{BRANCH_NAME}"),
        &["--rule", "git/branch-name"],
    );
    git.passes(
        "",
        &format!("\"git/no-ci-skip\" = \"off\"\n{BRANCH_NAME}"),
        &[],
    );
}

#[test]
fn the_commit_msg_hook_checks_the_message_being_written() {
    let git = GitOptions::new();
    let repo = &git.repo;
    repo.write(
        "bin/gate",
        &format!(
            "#!/bin/sh\nexec '{}' \"$@\" --plugin '{}'\n",
            env!("CARGO_BIN_EXE_gate").replace('\'', "'\\''"),
            git.plugin.replace('\'', "'\\''")
        ),
    );
    {
        use std::os::unix::fs::PermissionsExt as _;
        let path = repo.path().join("bin/gate");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins.git]\nsource = \"acme/rules:git@0.1.0\"\noptions = {{ main = [\"refs/heads/main\"], activation = \"{}\", {RELEASE_SUBJECT} }}\n\n[rules]\n\"git/commit-message\" = \"error\"\n{NO_CI_SKIP}\n\n[hooks]\ncommit-msg = [\"check --commit-message\"]\n",
            git.root
        ),
    );
    // Committed before the hooks exist, so this commit runs none.
    repo.commit("chore(core): configure gate");
    let output = repo.gate(&["sync", "--plugin", &git.plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));

    // `git commit` runs the generated hook, which runs bin/gate with the message file.
    // Each paragraph is one -m, as git takes them.
    let commit = |paragraphs: &[&str]| {
        repo.write("work.txt", &paragraphs.join("\n"));
        repo.git(&["add", "-A"]);
        let mut arguments = vec!["commit", "-q"];
        for paragraph in paragraphs {
            arguments.extend(["-m", paragraph]);
        }
        let output = isolated(
            Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(&arguments),
        )
        .env("GATE_CACHE_DIR", repo.cache.path())
        .output()
        .expect("git");
        (
            output.status.success(),
            format!("{}{}", text(&output.stdout), text(&output.stderr)),
        )
    };

    let (committed, said) = commit(&["feat(core): add the thing [skip ci]"]);
    assert!(!committed, "a CI-skip directive fails the hook: {said}");
    assert!(
        said.contains("the commit message carries the forbidden CI-skip directive [skip ci]")
            && said.contains("gate check (tree, commit message): "),
        "{said}"
    );
    // git keeps a `#` line given with -m, so the directive in it would be stored.
    let (committed, said) = commit(&["feat(core): add x", "#12 done [skip ci]"]);
    assert!(!committed, "a directive in a # line fails the hook: {said}");
    assert!(
        said.contains("the commit message carries the forbidden CI-skip directive [skip ci]"),
        "{said}"
    );
    let (committed, said) = commit(&["Added the thing"]);
    assert!(!committed, "a message out of form fails the hook: {said}");
    assert!(said.contains("git/commit-message#type-empty"), "{said}");
    let (committed, said) = commit(&["feat(core): add the thing"]);
    assert!(committed, "a good message passes the hook: {said}");
}

#[test]
fn a_projects_rule_level_wins_over_the_aspect_levels_of_git_recommended() {
    let git = GitOptions::new();
    let repo = &git.repo;
    let message = repo.path().join("message.txt");
    std::fs::write(&message, "ci(a): x\n").expect("message");
    let message = message.display().to_string();
    let check = |rules: &str| {
        repo.write(
            "gate.toml",
            &format!(
                "[gate]\nversion = \"{VERSION}\"\n\n[project]\nname = \"one\"\nextends = [\"git/recommended\"]\n\n[plugins.git]\nsource = \"acme/rules:git@0.1.0\"\noptions = {{ main = [\"refs/heads/main\"] }}\n\n[rules]\n{rules}\n"
            ),
        );
        repo.git(&["add", "gate.toml"]);
        repo.git(&["commit", "-q", "--allow-empty", "-m", "configure"]);
        let output = repo.gate(&[
            "check",
            "--plugin",
            &git.plugin,
            "--commit-message",
            &message,
        ]);
        (output.status.code(), text(&output.stdout))
    };
    let at = |level: &str, aspect: &str| format!("{level}[git/commit-message#{aspect}]");

    // The preset makes header-min-length a warning; subject-min-length keeps the default error.
    let (code, said) = check("");
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains(&at("warning", "header-min-length"))
            && said.contains(&at("error", "subject-min-length")),
        "{said}"
    );
    // A level the project gives the whole rule applies to every aspect, the preset's too.
    let (code, said) = check("\"git/commit-message\" = \"note\"");
    assert_eq!(code, Some(0), "{said}");
    assert!(
        said.contains(&at("note", "header-min-length"))
            && said.contains(&at("note", "subject-min-length")),
        "{said}"
    );
    // A level the project gives the aspect wins over both.
    let (code, said) = check(
        "\"git/commit-message\" = { level = \"note\", levels = { header-min-length = \"error\" } }",
    );
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains(&at("error", "header-min-length"))
            && said.contains(&at("note", "subject-min-length")),
        "{said}"
    );
}

/// `gate check --timings` with the built probe: the exit code, the finding messages sorted, the
/// scopes whose call ran (not from the result cache), sorted, and the `sync-state` timing line.
fn probe_timed(repo: &Repo) -> (Option<i32>, Vec<String>, Vec<String>, Option<String>) {
    let (code, messages, ran, states, _) = probe_stale(repo);
    (code, messages, ran, states)
}

/// For each call that ran although a result of it was stored: its scope and the changed input.
type Stale = Vec<(String, String)>;

/// [`probe_timed`], and for each call that ran although a result of it was stored, its scope and
/// the input `--timings` names as changed, sorted.
fn probe_stale(repo: &Repo) -> (Option<i32>, Vec<String>, Vec<String>, Option<String>, Stale) {
    probe_stale_from(repo, &probe().0)
}

/// [`probe_stale`] with the probe component read from `wasm`.
fn probe_stale_from(
    repo: &Repo,
    wasm: &Path,
) -> (Option<i32>, Vec<String>, Vec<String>, Option<String>, Stale) {
    let plugin = format!("probe={}", wasm.display());
    let output = repo.gate(&[
        "check",
        "--plugin",
        &plugin,
        "--format",
        "json",
        "--timings",
    ]);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("json: {}", text(&output.stderr)));
    let mut messages: Vec<String> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter_map(|finding| finding["message"].as_str().map(str::to_owned))
        .collect();
    messages.sort();
    let stderr = text(&output.stderr);
    let mut ran = Vec::new();
    let mut stale = Vec::new();
    let mut calls = 0;
    for line in stderr.lines() {
        let Some(call) = line.strip_prefix("timings: call probe ") else {
            continue;
        };
        calls += 1;
        let (scope, outcome) = call.split_once(": ").expect("scope and outcome");
        if outcome.starts_with("ran ") {
            ran.push(scope.to_owned());
            if let Some((_, changed)) = outcome.split_once("; its stored result no longer holds: ")
            {
                stale.push((scope.to_owned(), changed.to_owned()));
            }
        } else {
            assert!(outcome.starts_with("cached, checked in "), "{line}");
        }
    }
    assert!(calls > 0, "--timings names each call: {stderr}");
    ran.sort();
    stale.sort();
    let states = stderr
        .lines()
        .find_map(|line| line.strip_prefix("timings: sync-state: "))
        .map(str::to_owned);
    (output.status.code(), messages, ran, states, stale)
}

/// A probe repository whose two projects, `one` and `two`, each run `probe/read` on their own
/// `a.txt` and list their own files.
fn two_readers() -> Repo {
    let repo = probe_repo_for(
        "apps/*",
        r#""probe/read" = { level = "error", options = { paths = ["a.txt"], list = "" } }"#,
    );
    repo.write("apps/one/a.txt", "one\n");
    repo.write("apps/two/a.txt", "two\n");
    repo.commit("two readers");
    repo
}

#[test]
fn a_second_run_on_an_unchanged_tree_runs_no_plugin_call() {
    let repo = two_readers();
    let (code, first, ran, _) = probe_timed(&repo);
    assert_eq!(code, Some(1), "{first:?}");
    assert_eq!(ran, ["one", "two"], "a cold run runs every call");
    assert!(first.contains(&"a.txt: one\n".to_owned()), "{first:?}");

    let (code, second, ran, _) = probe_timed(&repo);
    assert_eq!(code, Some(1), "{second:?}");
    assert!(ran.is_empty(), "a warm run runs no call: {ran:?}");
    assert_eq!(second, first, "the stored results report the same findings");
}

#[test]
fn changing_a_rules_options_or_the_plugins_sha256_reruns_its_calls() {
    let repo = two_readers();
    let (_, first, ran, _) = probe_timed(&repo);
    assert_eq!(ran, ["one", "two"]);
    let (_, _, ran, _) = probe_timed(&repo);
    assert!(ran.is_empty(), "warm: {ran:?}");

    // Other options are another request: no stored result answers it.
    let config =
        std::fs::read_to_string(repo.directory.path().join("gate.toml")).expect("gate.toml");
    repo.write(
        "gate.toml",
        &config.replace(r#"paths = ["a.txt"]"#, r#"paths = ["a.txt", "b.txt"]"#),
    );
    let (_, _, ran, _) = probe_timed(&repo);
    assert_eq!(ran, ["one", "two"], "changed options rerun every call");
    // So are other options of the plugin itself.
    let labelled = config.replace(
        r#"probe = { source = "acme/rules:probe@0.1.0" }"#,
        r#"probe = { source = "acme/rules:probe@0.1.0", options = { label = "x" } }"#,
    );
    assert_ne!(labelled, config);
    repo.write("gate.toml", &labelled);
    let (_, messages, ran, _) = probe_timed(&repo);
    assert_eq!(
        ran,
        ["one", "two"],
        "changed plugin options rerun every call"
    );
    assert!(messages.contains(&"label x".to_owned()), "{messages:?}");
    repo.write("gate.toml", &config);
    let (_, back, ran, _) = probe_timed(&repo);
    assert!(
        ran.is_empty(),
        "the first options' results are still stored: {ran:?}"
    );
    assert_eq!(back, first);

    // The same component with one more custom section: other bytes, another sha256.
    let mut bytes = probe().1.clone();
    bytes.extend_from_slice(&[0, 6, 5]);
    bytes.extend_from_slice(b"other");
    let outside = tempfile::tempdir().expect("temporary directory");
    let other = outside.path().join("probe.wasm");
    std::fs::write(&other, &bytes).expect("write the changed component");
    let (code, messages, ran, _, _) = probe_stale_from(&repo, &other);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(ran, ["one", "two"], "another sha256 reruns every call");
    assert_eq!(messages, first);
    let (_, _, ran, _, _) = probe_stale_from(&repo, &other);
    assert!(ran.is_empty(), "and stores its own results: {ran:?}");
}

#[test]
fn changing_one_read_file_reruns_exactly_the_calls_that_read_it() {
    let repo = two_readers();
    probe_timed(&repo);

    repo.write("apps/one/a.txt", "one, changed\n");
    repo.commit("change a file one reads");
    let (_, messages, ran, _, stale) = probe_stale(&repo);
    assert_eq!(ran, ["one"], "only the call that read the file runs");
    assert_eq!(
        stale,
        [("one".to_owned(), "the file \"apps/one/a.txt\"".to_owned())],
        "--timings names the input that changed"
    );
    assert!(
        messages.contains(&"a.txt: one, changed\n".to_owned()),
        "{messages:?}"
    );
    assert!(
        messages.contains(&"a.txt: two\n".to_owned()),
        "{messages:?}"
    );

    repo.write("apps/two/b.txt", "new\n");
    repo.commit("add a file below what two lists");
    let (_, messages, ran, _, stale) = probe_stale(&repo);
    assert_eq!(ran, ["two"], "only the call whose listing changed runs");
    assert_eq!(
        stale,
        [("two".to_owned(), "the listing of \"apps/two/\"".to_owned())],
        "--timings names the listing that changed"
    );
    assert!(
        messages.contains(&"list : a.txt,b.txt,gate.toml".to_owned()),
        "{messages:?}"
    );

    let (_, _, ran, _) = probe_timed(&repo);
    assert!(ran.is_empty(), "{ran:?}");
}

#[test]
fn an_uncommitted_edit_reruns_the_call_even_one_git_sees_as_no_change() {
    let repo = two_readers();
    probe_timed(&repo);

    repo.write("apps/one/a.txt", "one, edited\n");
    let (_, messages, ran, _) = probe_timed(&repo);
    assert_eq!(ran, ["one"], "an edit not committed counts");
    assert!(
        messages.contains(&"a.txt: one, edited\n".to_owned()),
        "{messages:?}"
    );

    // With core.autocrlf, git sees an edit of line endings alone as no change; the plugin
    // reads other bytes, so its call runs again.
    repo.git(&["config", "core.autocrlf", "true"]);
    repo.commit("the edit");
    probe_timed(&repo);
    repo.write("apps/one/a.txt", "one, edited\r\n");
    let (_, messages, ran, _) = probe_timed(&repo);
    assert_eq!(ran, ["one"], "a line-ending edit counts");
    assert!(
        messages.contains(&"a.txt: one, edited\r\n".to_owned()),
        "{messages:?}"
    );
}

#[test]
fn a_moved_ref_reruns_only_the_calls_whose_read_set_holds_a_ref_answer() {
    let repo = Repo::empty();
    let source = "acme/rules:probe@0.1.0";
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[workspace]\nprojects = [\"apps/*\"]\n\n[plugins]\nprobe = {{ source = \"{source}\" }}\n\n[[profiles]]\nprojects = [\"apps/one\"]\n\n[profiles.rules]\n\"probe/history\" = \"error\"\n\n[[profiles]]\nprojects = [\"apps/two\"]\n\n[profiles.rules]\n\"probe/read\" = {{ level = \"error\", options = {{ paths = [\"a.txt\"] }} }}\n"
        ),
    );
    repo.write("apps/one/gate.toml", "[project]\nname = \"one\"\n");
    repo.write("apps/two/gate.toml", "[project]\nname = \"two\"\n");
    repo.write("apps/two/a.txt", "two\n");
    repo.write(
        "gate.lock",
        &lock(
            source,
            "https://example.invalid/probe.wasm",
            &"b".repeat(64),
        ),
    );
    repo.commit("a history reader and a file reader");
    let (_, _, ran, _) = probe_timed(&repo);
    assert_eq!(ran, ["one", "two"]);

    repo.git(&["branch", "other"]);
    let (_, _, ran, _) = probe_timed(&repo);
    assert!(
        ran.is_empty(),
        "a ref no call asked about moves nothing: {ran:?}"
    );

    repo.git(&["commit", "-q", "--allow-empty", "-m", "move HEAD and main"]);
    let (_, messages, ran, _) = probe_timed(&repo);
    assert_eq!(
        ran,
        ["one"],
        "only the call that asked for HEAD and main runs"
    );
    assert!(messages.contains(&"log: 2".to_owned()), "{messages:?}");
}

/// The calls that ran, and the `git answers` line of `--timings`.
fn probe_git_answers(repo: &Repo) -> (Vec<String>, Option<String>) {
    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["check", "--plugin", &plugin, "--timings"]);
    let stderr = text(&output.stderr);
    let mut ran: Vec<String> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("timings: call probe "))
        .filter_map(|call| call.split_once(": ran ").map(|(scope, _)| scope.to_owned()))
        .collect();
    ran.sort();
    let answers = stderr
        .lines()
        .find_map(|line| line.strip_prefix("timings: git answers: "))
        .map(str::to_owned);
    (ran, answers)
}

#[test]
fn after_main_moves_a_history_call_asks_git_only_about_the_new_commits() {
    let repo = probe_repo_for(
        "apps/*",
        r#""probe/git" = { level = "error", options = { changed_paths = [["HEAD~2", "HEAD~1"], ["HEAD~1", "HEAD"]], is_ancestor = [["HEAD~1", "HEAD"]] } }"#,
    );
    for message in ["first", "second", "third"] {
        repo.write("apps/one/a.txt", &format!("{message}\n"));
        repo.commit(message);
    }

    // Two calls ask the same three questions: git answers each once, nothing is stored yet.
    let (ran, answers) = probe_git_answers(&repo);
    assert_eq!(ran, ["one", "two"]);
    assert_eq!(answers.as_deref(), Some("3 asked, 0 from the cache"));

    // Main moves by one commit: the calls run again; the pair of old commits is answered from
    // the cache, git is asked about the new commit only.
    repo.write("apps/one/a.txt", "fourth\n");
    repo.commit("fourth");
    let (ran, answers) = probe_git_answers(&repo);
    assert_eq!(ran, ["one", "two"]);
    assert_eq!(answers.as_deref(), Some("2 asked, 1 from the cache"));

    // Nothing moved: no call runs, no question is asked.
    let (ran, answers) = probe_git_answers(&repo);
    assert!(ran.is_empty(), "{ran:?}");
    assert_eq!(answers, None);
}

#[test]
fn a_commit_the_clone_lacks_is_no_ancestor_until_a_fetch_brings_it() {
    // Two commits of another repository, which this one fetches later.
    let other = Repo::empty();
    other.write("x.txt", "base\n");
    let base = other.commit("base");
    other.write("x.txt", "tip\n");
    let tip = other.commit("tip");
    let repo = probe_repo_for(
        "apps/one",
        &format!(
            r#""probe/git" = {{ level = "error", options = {{ is_ancestor = [["{base}", "{tip}"]] }} }}"#
        ),
    );
    repo.write("apps/one/a.txt", "one\n");
    repo.commit("first");
    let answer = |answer: &str| format!("is-ancestor {base} {tip}: {answer}");

    let (_, messages, ran, _) = probe_timed(&repo);
    assert_eq!(ran, ["one"]);
    assert!(messages.contains(&answer("false")), "{messages:?}");

    // The fetch brings both commits: the call runs again, and the no is not reused from the
    // stored git answers.
    let source = other.path().to_str().expect("a UTF-8 path");
    repo.git(&["fetch", "-q", source, "main"]);
    let (_, messages, ran, _) = probe_timed(&repo);
    assert_eq!(ran, ["one"]);
    assert!(messages.contains(&answer("true")), "{messages:?}");

    let (_, _, ran, _) = probe_timed(&repo);
    assert!(ran.is_empty(), "{ran:?}");
}

#[test]
fn merge_tree_answers_are_kept_per_git_version_and_never_from_a_shallow_clone() {
    let repo = probe_repo_for(
        "apps/one",
        r#""probe/git" = { level = "error", options = { merge_tree = [["main", "side"]] } }"#,
    );
    repo.write("apps/one/a.txt", "base\n");
    let base = repo.commit("base");
    repo.git(&["checkout", "-q", "-b", "side"]);
    repo.write("apps/one/b.txt", "side\n");
    repo.commit("side");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("apps/one/a.txt", "main\n");
    repo.commit("main");
    // Each run below runs the call: only the stored git answers carry over.
    let fresh = || {
        let _gone = std::fs::remove_dir_all(repo.cache.path().join("results"));
        probe_git_answers(&repo).1
    };

    assert_eq!(fresh().as_deref(), Some("1 asked, 0 from the cache"));
    assert_eq!(fresh().as_deref(), Some("0 asked, 1 from the cache"));

    // Another git version may merge differently: the question is asked again.
    let wrapper = tempfile::tempdir().expect("a directory");
    let git = wrapper.path().join("git");
    let real = String::from_utf8(
        Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .expect("sh")
            .stdout,
    )
    .expect("utf-8");
    std::fs::write(
        &git,
        format!(
            "#!/bin/sh\nfor last; do :; done\nif [ \"$last\" = --version ]; then echo 'git version 0.0.0.test'; exit 0; fi\nexec {} \"$@\"\n",
            real.trim()
        ),
    )
    .expect("write the wrapper");
    std::fs::set_permissions(&git, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("make the wrapper executable");
    let path = format!(
        "{}:{}",
        wrapper.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let _gone = std::fs::remove_dir_all(repo.cache.path().join("results"));
    let plugin = format!("probe={}", probe().0.display());
    let output = isolated(
        Command::new(env!("CARGO_BIN_EXE_gate"))
            .args(["check", "--plugin", &plugin, "--timings"])
            .current_dir(repo.path())
            .env("GATE_CACHE_DIR", repo.cache.path())
            .env("PATH", path),
    )
    .output()
    .expect("gate");
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains("timings: git answers: 1 asked, 0 from the cache"),
        "{stderr}"
    );

    // A shallow clone may lack the merge base: nothing is read from or kept in the store.
    std::fs::write(repo.path().join(".git/shallow"), format!("{base}\n")).expect("shallow");
    assert_eq!(fresh(), None);
}

#[test]
fn a_cache_that_cannot_be_written_warns_once_and_the_check_still_reports() {
    use std::os::unix::fs::PermissionsExt as _;
    let repo = probe_repo_for(
        "apps/one",
        r#""probe/git" = { level = "error", options = { is_ancestor = [["HEAD~1", "HEAD"]], sync_state = ["HEAD"] } }"#,
    );
    for message in ["first", "second"] {
        repo.write("apps/one/a.txt", &format!("{message}\n"));
        repo.commit(message);
    }
    // Every write goes through the cache's temporary directory: none can be made.
    let temporary = repo.cache.path().join("tmp");
    std::fs::create_dir_all(&temporary).expect("tmp");
    std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o555))
        .expect("read-only");

    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["check", "--plugin", &plugin, "--format", "json"]);
    std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o755))
        .expect("writable again");
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout)).expect("json");
    let messages: Vec<&str> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter_map(|finding| finding["message"].as_str())
        .collect();
    assert!(
        messages.contains(&"is-ancestor HEAD~1 HEAD: true"),
        "{messages:?}"
    );
    let warnings: Vec<&str> = stderr
        .lines()
        .filter(|line| line.starts_with("gate: warning: cannot write to the cache"))
        .collect();
    assert_eq!(warnings.len(), 1, "{stderr}");
    assert!(warnings[0].contains("GATE_CACHE_DIR"), "{stderr}");
}

#[test]
fn a_cold_history_run_computes_one_sync_state_per_commit_for_every_call_and_rule() {
    let repo = probe_repo_for(
        "apps/*",
        r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~2", "HEAD~1", "HEAD"], sync_state_log = "HEAD~3..HEAD" } }"#,
    );
    repo.write("apps/one/a.txt", "first\n");
    repo.commit("first");
    for message in ["second", "third", "fourth"] {
        repo.write("apps/one/a.txt", &format!("{message}\n"));
        repo.commit(message);
    }

    // Two calls (one per project), each asking every commit twice: once by name, once from the
    // log. Each commit is computed once in the run, then read from the cache.
    let (code, messages, ran, states) = probe_timed(&repo);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(ran, ["one", "two"]);
    assert_eq!(states.as_deref(), Some("3 computed, 0 from the cache"));

    let (_, _, ran, states) = probe_timed(&repo);
    assert!(ran.is_empty(), "{ran:?}");
    assert_eq!(states.as_deref(), Some("0 computed, 3 from the cache"));
}

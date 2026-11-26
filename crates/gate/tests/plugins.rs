//! `gate check` running real plugins built for wasm32-wasip2: `examples/plugin-hello`, pinned in
//! `gate.lock` and served from the cache, or replaced for one run by `--plugin`; and the runtime's
//! probe, whose history rule reports the range a push run passes; the git and typescript plugins
//! for their option and setting errors.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

use sha2::{Digest as _, Sha256};

#[path = "../../gate-runtime/tests/support/component.rs"]
mod built;

/// The version of the core under test.
const VERSION: &str = env!("CARGO_PKG_VERSION");

const SOURCE: &str = "acme/rules:hello@0.1.0";

/// A workspace package built as a component once per process: its path and bytes.
fn component(package: &str, library: &str) -> &'static (PathBuf, Vec<u8>) {
    built::component(package, library, "")
}

/// [`component`] with the cargo `features` given, in a target directory of their own.
fn component_with(package: &str, library: &str, features: &str) -> &'static (PathBuf, Vec<u8>) {
    built::component(package, library, features)
}

/// The hello component.
fn hello() -> &'static (PathBuf, Vec<u8>) {
    component("gate-plugin-hello", "gate_plugin_hello")
}

/// The typescript plugin's component.
fn typescript() -> &'static (PathBuf, Vec<u8>) {
    component("gate-plugin-typescript", "typescript")
}

/// The ledger plugin's component.
fn ledger() -> &'static (PathBuf, Vec<u8>) {
    component("gate-plugin-ledger", "ledger")
}

/// The agents plugin's component.
fn agents() -> &'static (PathBuf, Vec<u8>) {
    component("gate-plugin-agents", "agents")
}

/// The runtime's probe component.
fn probe() -> &'static (PathBuf, Vec<u8>) {
    component("gate-runtime-probe", "gate_runtime_probe")
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
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins]\nprobe = {{ source = \"{source}\" }}\n\n[generate]\n\"probe/write\" = {{ options = {{ files = {{ \"gen/a.md\" = \"a\\n\" }} }} }}\n\n[rules]\n\"probe/git\" = {{ level = \"error\", options = {{ sync_state = [\"HEAD~1\", \"HEAD\"] }} }}\n\n[hooks]\n{hooks}\n\n[ci.github]\nworkflow = \".github/workflows/gate.yml\"\ncheckout = \"actions/checkout@{action}\"\n\n[ci.github.jobs.lint]\nname = \"Lint\"\ntimeout-minutes = 10\nrun = [\"check\"]\n"
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

/// One `gate check --timings` with the built probe: its findings, sorted, the calls that ran,
/// sorted, and its standard error.
fn probe_timings(repo: &Repo) -> (Vec<String>, Vec<String>, String) {
    probe_timings_from(repo, &probe().0)
}

/// [`probe_timings`] with the probe component read from `wasm`.
fn probe_timings_from(repo: &Repo, wasm: &Path) -> (Vec<String>, Vec<String>, String) {
    let plugin = format!("probe={}", wasm.display());
    let output = repo.gate(&[
        "check",
        "--plugin",
        &plugin,
        "--format",
        "json",
        "--timings",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
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
    let mut ran: Vec<String> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("timings: call probe "))
        .filter_map(|call| call.split_once(": ran ").map(|(scope, _)| scope.to_owned()))
        .collect();
    ran.sort();
    (messages, ran, stderr)
}

/// The `--timings` line that starts with `prefix`, without it.
fn timing(stderr: &str, prefix: &str) -> Option<String> {
    stderr
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .map(str::to_owned)
}

/// [`probe_timings`] with the line of the generators that ran and came from the cache.
fn probe_generated(repo: &Repo) -> (Vec<String>, Vec<String>, Option<String>) {
    let (messages, ran, stderr) = probe_timings(repo);
    (messages, ran, timing(&stderr, "timings: generators: "))
}

/// How many generators ran in a run of [`probe_timings_from`], and how many came from the cache.
fn generators_from(repo: &Repo, wasm: &Path) -> (usize, usize) {
    let (_, _, stderr) = probe_timings_from(repo, wasm);
    let line = timing(&stderr, "timings: generators: ").unwrap_or_else(|| panic!("{stderr}"));
    let count = |part: &str| {
        part.split(' ')
            .next()
            .and_then(|count| count.parse().ok())
            .unwrap_or_else(|| panic!("{line}"))
    };
    let (ran, stored) = line.split_once(", ").unwrap_or_else(|| panic!("{line}"));
    (count(ran), count(stored))
}

/// A repository whose generator `probe/write` writes, in `apps/one`, what `options` ask, and
/// whose rule asks sync-state at `HEAD~1`. There the generator also writes a core file, so the
/// answer follows the run's configuration and every run runs the run's generators on its tree.
fn generating(options: &str) -> Repo {
    let repo =
        probe_repo(r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1"] } }"#);
    let core = r#"files = { "gate.toml" = "overwritten\n" }"#;
    repo.write(
        "apps/one/gate.toml",
        &writing(&format!("{options}, {core}")),
    );
    repo.write("apps/one/inputs/a.txt", "a\n");
    repo.commit("the generator writes a core file: the run's configuration answers");
    repo.write("apps/one/gate.toml", &writing(options));
    repo.commit("the generator writes what its options name");
    repo
}

#[test]
fn other_generator_options_or_another_build_of_its_plugin_run_the_generator_again() {
    let options = r#"list_under = "inputs""#;
    let repo = generating(options);
    let built = probe().0.clone();
    let generators = || generators_from(&repo, &built);
    assert_ne!(generators().0, 0, "cold");
    assert_eq!(generators().0, 0, "warm");

    // Other options: no stored output answers them.
    let other = r#"list_under = "inputs", files = { "gen/z.md" = "z\n" }"#;
    repo.write("apps/one/gate.toml", &writing(other));
    assert_ne!(generators().0, 0, "other options");
    assert_eq!(generators().0, 0, "other options, warm");
    // The first options' output is still stored.
    repo.write("apps/one/gate.toml", &writing(options));
    assert_eq!(generators().0, 0, "the first options again");

    // The same component with one more custom section: other bytes, another sha256.
    let mut bytes = probe().1.clone();
    bytes.extend_from_slice(&[0, 6, 5]);
    bytes.extend_from_slice(b"other");
    let outside = tempfile::tempdir().expect("temporary directory");
    let rebuilt = outside.path().join("probe.wasm");
    std::fs::write(&rebuilt, &bytes).expect("write the changed component");
    assert_ne!(generators_from(&repo, &rebuilt).0, 0, "another build");
    assert_eq!(generators_from(&repo, &rebuilt).0, 0, "another build, warm");
}

#[test]
fn a_file_that_appears_where_a_generator_looked_runs_it_again() {
    let repo = generating(r#"list_under = "inputs", list = "gen.list""#);
    let built = probe().0.clone();
    let generators = || generators_from(&repo, &built);
    assert_ne!(generators().0, 0, "cold");
    assert_eq!(generators().0, 0, "warm");

    // A new file under the directory the generator listed.
    repo.write("apps/one/inputs/b.txt", "b\n");
    repo.git(&["add", "apps/one/inputs/b.txt"]);
    assert_ne!(generators().0, 0, "a new file under a listed directory");
    assert_eq!(generators().0, 0, "warm again");

    // The file the generator read and did not find is created.
    repo.write("apps/one/gen.list", "gen/c.md\n");
    repo.git(&["add", "apps/one/gen.list"]);
    assert_ne!(generators().0, 0, "a file that was missing is created");
    assert_eq!(generators().0, 0, "warm at last");
}

/// `gate sync --check` with the built probe: its exit code and output.
fn sync_check(repo: &Repo) -> (Option<i32>, String) {
    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["sync", "--check", "--plugin", &plugin]);
    (
        output.status.code(),
        format!("{}{}", text(&output.stdout), text(&output.stderr)),
    )
}

/// Rewrites every stored generator output whose text is `from` to `to`.
fn tamper_stored_outputs(cache: &Path, from: &str, to: &str) -> usize {
    let mut tampered = 0;
    let mut directories = vec![cache.join("results")];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory)
            .expect("the stored results")
            .map(|entry| entry.expect("a directory entry").path())
        {
            if entry.is_dir() {
                directories.push(entry);
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&entry) else {
                continue;
            };
            let changed = content.replace(from, to);
            if changed != content {
                std::fs::write(&entry, changed).expect("rewrite a stored result");
                tampered += 1;
            }
        }
    }
    tampered
}

#[test]
fn sync_check_answers_from_the_stored_outputs_until_an_input_changes() {
    let repo = probe_repo(r#""probe/git" = { level = "error", options = { read_at = [] } }"#);
    repo.write("apps/one/gate.toml", &writing(r#"list = "gen.list""#));
    repo.write("apps/one/gen.list", "gen/a.md\n");
    repo.commit("the generator's input");
    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["sync", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    repo.commit("generated");
    let (code, said) = sync_check(&repo);
    assert_eq!(code, Some(0), "{said}");

    // The stored output says otherwise than the generator: a check that reports it out of date
    // was answered from the cache.
    let stored = r#""text":"listed\n""#;
    let tampered = tamper_stored_outputs(repo.cache.path(), stored, r#""text":"stored\n""#);
    assert_eq!(tampered, 1);
    let (code, said) = sync_check(&repo);
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains("1 out of date"), "{said}");

    // The file the generator reads changes, to the same outputs: it runs again.
    repo.write("apps/one/gen.list", "gen/a.md\n\n");
    let (code, said) = sync_check(&repo);
    assert_eq!(code, Some(0), "{said}");
}

#[test]
fn a_rule_that_asked_only_sizes_is_answered_from_the_cache_until_a_size_changes() {
    let repo = probe_repo(
        r#""probe/read" = { level = "error", options = { sizes = ["big.bin", "absent.bin", "../two/gate.toml"] } }"#,
    );
    repo.write("apps/one/big.bin", "12345");
    repo.commit("a file whose size the rule asks");
    let sizes = [
        "size ../two/gate.toml: none",
        "size absent.bin: none",
        "size big.bin: 5",
    ];

    let (messages, ran, _) = probe_timings(&repo);
    assert_eq!(messages, sizes);
    assert_eq!(ran, ["one"]);

    // Other contents of the same size: the stored result holds.
    repo.write("apps/one/big.bin", "abcde");
    let (messages, ran, _) = probe_timings(&repo);
    assert_eq!(messages, sizes);
    assert!(ran.is_empty(), "{ran:?}");

    // Another size: the call runs again.
    repo.write("apps/one/big.bin", "123456");
    let (messages, ran, _) = probe_timings(&repo);
    assert_eq!(messages[2], "size big.bin: 6");
    assert_eq!(ran, ["one"]);

    // The file that was absent appears: the call runs again.
    repo.write("apps/one/absent.bin", "x");
    repo.git(&["add", "apps/one/absent.bin"]);
    let (messages, ran, _) = probe_timings(&repo);
    assert_eq!(messages[1], "size absent.bin: 1");
    assert_eq!(ran, ["one"]);
}

#[test]
fn a_warm_run_answers_the_runs_generators_from_the_cache_until_a_file_they_read_changes() {
    let repo =
        probe_repo(r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1"] } }"#);
    repo.write(
        "apps/one/gate.toml",
        &writing(r#"list = "gen.list", files = { "gate.toml" = "overwritten\n" }"#),
    );
    repo.write("apps/one/gen.list", "gen/a.md\n");
    repo.commit("the generator writes a core file: the run's configuration answers");
    repo.write("apps/one/gate.toml", &writing(r#"list = "gen.list""#));
    repo.write("apps/one/gen/a.md", "listed\n");
    repo.commit("it writes what gen.list names");

    let (_, ran, generated) = probe_generated(&repo);
    assert_eq!(ran, ["one"]);
    assert_eq!(generated.as_deref(), Some("1 ran, 0 from the cache"));

    // Nothing changed: the call's stored result asks sync-state again, whose answer follows the
    // run's generators; their outputs come from the cache, nothing runs.
    let (messages, ran, generated) = probe_generated(&repo);
    assert_eq!(
        messages,
        ["sync-state HEAD~1: produced gen/a.md; drift gen/a.md"]
    );
    assert!(ran.is_empty(), "{ran:?}");
    assert_eq!(generated.as_deref(), Some("0 ran, 1 from the cache"));

    // The file the generator reads changes: it runs again, and so does the call.
    repo.write("apps/one/gen.list", "gen/b.md\n");
    let (messages, ran, generated) = probe_generated(&repo);
    assert_eq!(
        messages,
        ["sync-state HEAD~1: produced gen/b.md; drift gen/b.md"]
    );
    assert_eq!(ran, ["one"]);
    assert_eq!(generated.as_deref(), Some("1 ran, 0 from the cache"));
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
    let (toml, lock) = with_other(&repo, trapping, &adopted, &pinned);
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
        "[gate]\nversion = \"{VERSION}\"\n\n[plugins]\nprobe = {{ source = \"{source}\" }}\n\n[generate]\n\"probe/write\" = {{ options = {{ files = {{ \"gen/a.md\" = \"a\\n\" }} }} }}\n\n[rules]\n\"probe/git\" = {{ level = \"error\", options = {{ sync_state = [\"HEAD~1\"] }} }}\n\n[ci.github]\nworkflow = \".github/workflows/gate.yml\"\ncheckout = \"actions/checkout@{action}\"\n\n[ci.github.jobs.lint]\nname = \"Lint\"\ntimeout-minutes = 10\nrun = [\"check\"]\n"
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

/// A probe repository whose project `apps/one` extends `extends` and sets `rules` in its own file.
fn project_extends(extends: &str, rules: &str) -> Repo {
    let repo = probe_repo("");
    repo.write(
        "apps/one/gate.toml",
        &format!("[project]\nname = \"one\"\nextends = [{extends}]\n\n[rules]\n{rules}\n"),
    );
    repo.commit("the project extends a preset");
    repo
}

/// `gate check` with the built probe as text: its exit code and what it printed.
fn gate_probe(repo: &Repo) -> (Option<i32>, String) {
    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["check", "--plugin", &plugin]);
    (
        output.status.code(),
        format!("{}{}", text(&output.stdout), text(&output.stderr)),
    )
}

#[test]
fn options_a_preset_gives_that_break_the_schema_are_the_plugins_error() {
    // probe/needs-broken gives probe/needs a number where its pattern is a string.
    let (code, said) = gate_probe(&project_extends(
        "\"probe/needs-broken\"",
        r#""probe/needs" = "warning""#,
    ));
    assert_eq!(code, Some(2), "{said}");
    assert!(
        said.contains("environment error")
            && said.contains(
                "[project] extends: \"probe/needs\" options the preset probe/needs-broken gives:"
            )
            && said.contains("the plugin is broken, report it to its authors")
            && !said.contains("[rules]: \"probe/needs\""),
        "{said}"
    );
}

#[test]
fn a_level_in_rules_keeps_the_options_a_preset_gives_the_rule() {
    // probe/needs requires the option pattern; the preset probe/needs gives it.
    let profile = |rules: &str| {
        let repo = probe_repo(rules);
        let toml = std::fs::read_to_string(repo.path().join("gate.toml"))
            .expect("gate.toml")
            .replace(
                "projects = [\"apps/one\"]\n",
                "projects = [\"apps/one\"]\nextends = [\"probe/needs\"]\n",
            );
        repo.write("gate.toml", &toml);
        repo.commit("the profile extends probe/needs");
        repo
    };
    let project = |rules: &str| project_extends("\"probe/needs\"", rules);

    // A level alone keeps the preset's options, in a profile and in a project's own file.
    for repo in [
        profile(r#""probe/needs" = "warning""#),
        project(r#""probe/needs" = "warning""#),
    ] {
        let (code, messages) = probe_check(&repo, &[]);
        assert_eq!(code, Some(0), "{messages:?}");
        assert_eq!(messages, ["needs from the preset"]);
    }

    // Options of the entry's own replace the preset's.
    let (code, messages) = probe_check(
        &project(r#""probe/needs" = { level = "warning", options = { pattern = "of its own" } }"#),
        &[],
    );
    assert_eq!(code, Some(0), "{messages:?}");
    assert_eq!(messages, ["needs of its own"]);

    // The entry's own options are checked even when the preset's would do.
    let (code, said) = gate_probe(&project(
        r#""probe/needs" = { level = "warning", options = {} }"#,
    ));
    assert_eq!(code, Some(2), "{said}");
    assert!(said.contains("[rules]: \"probe/needs\" options:"), "{said}");

    // A later preset that sets the rule replaces the earlier one's settings, options included.
    let (code, said) = gate_probe(&project_extends(
        "\"probe/needs\", \"probe/needs-level\"",
        r#""probe/needs" = "warning""#,
    ));
    assert_eq!(code, Some(2), "{said}");
    assert!(said.contains("[rules]: \"probe/needs\" options:"), "{said}");

    // Without a preset that gives them, a level alone still lacks the required option.
    let repo = probe_repo(r#""probe/needs" = "warning""#);
    repo.commit("no preset");
    let (code, said) = gate_probe(&repo);
    assert_eq!(code, Some(2), "{said}");
    assert!(
        said.contains("[[profiles]] rules: \"probe/needs\" options:") && said.contains("pattern"),
        "{said}"
    );
}

/// `gate check` with the built probe in each report format: its exit code and output, by format.
fn probe_check_formats(repo: &Repo) -> Vec<(&'static str, Option<i32>, String)> {
    let plugin = format!("probe={}", probe().0.display());
    ["text", "json", "sarif"]
        .into_iter()
        .map(|format| {
            let output = repo.gate(&["check", "--plugin", &plugin, "--format", format]);
            (format, output.status.code(), text(&output.stdout))
        })
        .collect()
}

#[test]
fn a_finding_in_a_nested_projects_option_file_names_its_path_once() {
    let repo = probe_repo("");
    repo.write(
        "apps/one/gate.toml",
        "[project]\nname = \"one\"\n\n[rules]\n\"probe/read\" = { level = \"error\", options-file = \"gate/options/probe/read.json\" }\n",
    );
    repo.write(
        "apps/one/gate/options/probe/read.json",
        "{ \"at_options_file\": true }\n",
    );
    repo.commit("an option file in a nested project");

    // The rule addresses its option file from the project's root and can read it.
    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        ["options file gate/options/probe/read.json: { \"at_options_file\": true }"]
    );
    for (format, code, output) in probe_check_formats(&repo) {
        assert_eq!(code, Some(1), "{format}: {output}");
        assert!(
            output.contains("apps/one/gate/options/probe/read.json"),
            "{format}: {output}"
        );
        assert!(!output.contains("apps/one/apps/one"), "{format}: {output}");
    }
}

#[test]
fn a_projects_rule_reads_a_nested_projects_file_but_reports_nothing_on_it() {
    // `apps/one/inner` is a project of its own, nested in `one`: by a `gate.toml` of its own, or
    // found by a workspace glob with none. A rule of `one` still reads the nested file, and reports
    // at its own option file even when that lies in the nested folder; its finding on the nested
    // file is the nested project's to judge and is not reported.
    for (how, inner_gate_toml, globs) in [
        ("a gate.toml", true, r#"["apps/*"]"#),
        ("a glob", false, r#"["apps/*", "apps/one/inner"]"#),
    ] {
        let repo = probe_repo("");
        let root = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
        repo.write(
            "gate.toml",
            &root.replace(r#"projects = ["apps/*"]"#, &format!("projects = {globs}")),
        );
        repo.write(
            "apps/one/gate.toml",
            "[project]\nname = \"one\"\n\n[rules]\n\"probe/read\" = { level = \"error\", options-file = \"inner/read.json\" }\n",
        );
        repo.write(
            "apps/one/inner/read.json",
            "{ \"paths\": [\"inner/a.txt\"], \"at\": \"inner/a.txt\", \"at_message\": \"on the nested file\", \"at_options_file\": true }\n",
        );
        if inner_gate_toml {
            repo.write("apps/one/inner/gate.toml", "[project]\nname = \"inner\"\n");
        }
        repo.write("apps/one/inner/a.txt", "alpha\n");
        repo.commit("a project nested in one");

        let (code, messages) = probe_check(&repo, &[]);
        assert_eq!(code, Some(1), "{how}: {messages:?}");
        assert_eq!(
            messages,
            [
                "inner/a.txt: alpha\n",
                "options file inner/read.json: { \"paths\": [\"inner/a.txt\"], \"at\": \"inner/a.txt\", \"at_message\": \"on the nested file\", \"at_options_file\": true }",
            ],
            "{how}"
        );
    }
}

#[test]
fn two_rules_of_one_call_each_read_and_report_at_their_own_option_file() {
    // Both rules of the probe run in one call for `apps/one`, each with an option file of its own:
    // each reads its own file and reports at its own repository path.
    let repo = probe_repo("");
    repo.write(
        "apps/one/gate.toml",
        "[project]\nname = \"one\"\n\n[rules]\n\"probe/read\" = { level = \"error\", options-file = \"gate/options/probe/read.json\" }\n\"probe/needs\" = { level = \"error\", options-file = \"gate/options/needs.json\" }\n",
    );
    repo.write(
        "apps/one/gate/options/probe/read.json",
        "{ \"at_options_file\": true }\n",
    );
    repo.write(
        "apps/one/gate/options/needs.json",
        "{ \"pattern\": \"x\", \"at_options_file\": true }\n",
    );
    repo.commit("two rules with option files of their own");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        [
            "needs x",
            "options file gate/options/needs.json: { \"pattern\": \"x\", \"at_options_file\": true }",
            "options file gate/options/probe/read.json: { \"at_options_file\": true }",
        ]
    );
    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["check", "--plugin", &plugin, "--format", "json"]);
    let report: serde_json::Value =
        serde_json::from_str(&text(&output.stdout)).expect("a JSON report");
    let mut at_file: Vec<(&str, &str)> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter(|finding| {
            finding["message"]
                .as_str()
                .is_some_and(|message| message.starts_with("options file "))
        })
        .filter_map(|finding| {
            Some((
                finding["rule"].as_str()?,
                finding["location"]["path"].as_str()?,
            ))
        })
        .collect();
    at_file.sort_unstable();
    assert_eq!(
        at_file,
        [
            ("probe/needs", "apps/one/gate/options/needs.json"),
            ("probe/read", "apps/one/gate/options/probe/read.json"),
        ]
    );
}

#[test]
fn a_failing_rule_in_a_nested_project_names_the_project_its_paths_are_relative_to() {
    // The plugin's error names a path as the rule addresses it, from the project's root; the core
    // cannot rewrite the text, so its error names the project and that root.
    let repo = probe_repo("");
    repo.write(
        "apps/one/gate.toml",
        "[project]\nname = \"one\"\n\n[rules]\n\"probe/read\" = { level = \"error\", options-file = \"gate/options/probe/read.json\" }\n",
    );
    repo.write(
        "apps/one/gate/options/probe/read.json",
        "{ \"fail\": \"cannot use it\" }\n",
    );
    repo.commit("a rule that fails in a nested project");

    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["check", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let message = text(&output.stderr);
    assert!(
        message.contains(
            "plugin probe in project one (its paths are relative to apps/one): rule read failed: gate/options/probe/read.json: cannot use it"
        ),
        "{message}"
    );
}

#[test]
fn a_rule_reads_a_root_profiles_option_file_and_reports_at_its_repository_path() {
    // The root's profile gives a rule of `apps/one` an option file outside the project, and the
    // plugin declares no extra reads: the call may read that file, and only it.
    let repo = probe_repo(
        r#""probe/read" = { level = "error", options-file = "gate/options/probe/read.json" }"#,
    );
    repo.write(
        "gate/options/probe/read.json",
        "{ \"at_options_file\": true, \"paths\": [\"../../gate/options/probe/other.json\"] }\n",
    );
    repo.write("gate/options/probe/other.json", "{}\n");
    repo.commit("an option file from the root's profile");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        [
            "../../gate/options/probe/other.json: none",
            "options file ../../gate/options/probe/read.json: { \"at_options_file\": true, \"paths\": [\"../../gate/options/probe/other.json\"] }",
        ]
    );
    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["check", "--plugin", &plugin, "--format", "json"]);
    let report: serde_json::Value =
        serde_json::from_str(&text(&output.stdout)).expect("a JSON report");
    let at_file: Vec<&str> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter(|finding| {
            finding["message"]
                .as_str()
                .is_some_and(|message| message.starts_with("options file "))
        })
        .filter_map(|finding| finding["location"]["path"].as_str())
        .collect();
    assert_eq!(at_file, ["gate/options/probe/read.json"]);
}

#[test]
fn a_finding_in_the_option_file_of_a_single_project_repository_names_its_path() {
    let repo = Repo::empty();
    let source = "acme/rules:probe@0.1.0";
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins]\nprobe = {{ source = \"{source}\" }}\n\n[rules]\n\"probe/read\" = {{ level = \"error\", options-file = \"gate/options/probe/read.json\" }}\n"
        ),
    );
    repo.write(
        "gate.lock",
        &lock(
            source,
            "https://example.invalid/probe.wasm",
            &"b".repeat(64),
        ),
    );
    repo.write(
        "gate/options/probe/read.json",
        "{ \"at_options_file\": true }\n",
    );
    repo.commit("an option file at the root");

    let (code, messages) = probe_check(&repo, &[]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages,
        ["options file gate/options/probe/read.json: { \"at_options_file\": true }"]
    );
    for (format, code, output) in probe_check_formats(&repo) {
        assert_eq!(code, Some(1), "{format}: {output}");
        assert!(
            output.contains("gate/options/probe/read.json"),
            "{format}: {output}"
        );
        assert!(
            !output.contains("gate/options/probe/gate/"),
            "{format}: {output}"
        );
    }
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

/// `gate check --rule probe/aspects` with the built probe in [`probe_repo`]: the profile of
/// `apps/one` sets `rules` and extends `extends`, `project` is appended to `apps/one/gate.toml` and
/// `root` to the root's. The decisions of every rule are checked at load; `--rule` keeps the run to
/// one quiet rule. The exit code and the standard error.
fn decision_check(rules: &str, extends: &str, project: &str, root: &str) -> (Option<i32>, String) {
    let repo = probe_repo(rules);
    let mut toml = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    if !extends.is_empty() {
        toml = toml.replace(
            "projects = [\"apps/one\"]\n",
            &format!("projects = [\"apps/one\"]\nextends = [\"{extends}\"]\n"),
        );
    }
    toml.push_str(root);
    repo.write("gate.toml", &toml);
    repo.write(
        "apps/one/gate.toml",
        &format!("[project]\nname = \"one\"\n{project}"),
    );
    repo.commit("levels and a decision policy");
    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["check", "--plugin", &plugin, "--rule", "probe/aspects"]);
    (output.status.code(), text(&output.stderr))
}

const POLICY: &str = "\n[decision-policy]\nrequire-decision = true\n";

/// Asserts the run loaded its configuration: whatever it found, it did not exit 2.
fn loads((code, stderr): (Option<i32>, String)) {
    assert_ne!(code, Some(2), "{stderr}");
}

/// Asserts the run failed at load because a level below its default names no decision, with
/// every part in the message.
fn refused((code, stderr): (Option<i32>, String), parts: &[&str]) {
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("[decision-policy] requires a decision for that; "),
        "{stderr}"
    );
    for part in parts {
        assert!(stderr.contains(part), "{stderr:?} lacks {part:?}");
    }
}

#[test]
fn a_rule_no_preset_names_needs_no_decision_at_any_level() {
    // It runs only because its entry or a family turns it on: stricter than not running.
    for rules in [
        r#""probe/aspects" = "warning""#,
        r#""probe/aspects" = "off""#,
        r#""probe/aspects" = { levels = { loud = "note" } }"#,
        r#""probe/*" = "warning""#,
        r#""probe/*" = "off""#,
    ] {
        loads(decision_check(rules, "", "", POLICY));
    }
}

#[test]
fn under_a_decision_policy_a_level_below_the_presets_needs_a_decision() {
    // probe/quiet sets the aspect quiet to warning, and no rule level.
    loads(decision_check(
        r#""probe/aspects" = { levels = { quiet = "warning" } }"#,
        "probe/quiet",
        "",
        POLICY,
    ));
    refused(
        decision_check(
            r#""probe/aspects" = { levels = { quiet = "note" } }"#,
            "probe/quiet",
            "",
            POLICY,
        ),
        &[
            "error: gate.toml:",
            "\"probe/aspects\" (levels.quiet \"note\") sets probe/aspects#quiet below its default warning (the preset probe/quiet sets probe/aspects#quiet to warning), and gate.toml [decision-policy]",
            "add decision = \"<record>\" to the \"probe/aspects\" table",
        ],
    );
    loads(decision_check(
        r#""probe/aspects" = { levels = { quiet = "note" }, decision = "D-1" }"#,
        "probe/quiet",
        "",
        POLICY,
    ));
    refused(
        decision_check(r#""probe/aspects" = "note""#, "probe/quiet", "", POLICY),
        &[
            "\"probe/aspects\" (level \"note\") sets probe/aspects#quiet below its default warning (the preset probe/quiet sets probe/aspects#quiet to warning)",
        ],
    );
    loads(decision_check(
        r#""probe/aspects" = "error""#,
        "probe/quiet",
        "",
        POLICY,
    ));

    // probe/lenient sets the rule to warning and its aspect loud to error.
    refused(
        decision_check(r#""probe/aspects" = "note""#, "probe/lenient", "", POLICY),
        &[
            "\"probe/aspects\" (level \"note\") sets probe/aspects below its default warning (the preset probe/lenient sets probe/aspects to warning)",
        ],
    );
    // The string form is told the table form that holds a decision.
    refused(
        decision_check(r#""probe/aspects" = "off""#, "probe/lenient", "", POLICY),
        &["write it as \"probe/aspects\" = { level = \"off\", decision = \"<record>\" }"],
    );
    loads(decision_check(
        r#""probe/aspects" = { level = "off", decision = "D-1" }"#,
        "probe/lenient",
        "",
        POLICY,
    ));
    // An aspect the preset does not name falls back to the preset's rule level.
    loads(decision_check(
        r#""probe/aspects" = { levels = { quiet = "warning" } }"#,
        "probe/lenient",
        "",
        POLICY,
    ));
    refused(
        decision_check(
            r#""probe/aspects" = { levels = { quiet = "note" } }"#,
            "probe/lenient",
            "",
            POLICY,
        ),
        &[
            "sets probe/aspects#quiet below its default warning (the preset probe/lenient sets probe/aspects to warning)",
        ],
    );
}

#[test]
fn a_whole_rule_level_meets_each_preset_aspect_level_it_replaces_and_a_family_is_named_as_written()
{
    // A rule-wide warning keeps probe/lenient's rule level but replaces its aspect loud's error.
    refused(
        decision_check(
            r#""probe/aspects" = "warning""#,
            "probe/lenient",
            "",
            POLICY,
        ),
        &[
            "\"probe/aspects\" (level \"warning\") sets probe/aspects#loud below its default error (the preset probe/lenient sets probe/aspects#loud to error)",
        ],
    );
    loads(decision_check(
        r#""probe/aspects" = { level = "warning", levels = { loud = "error" } }"#,
        "probe/lenient",
        "",
        POLICY,
    ));
    refused(
        decision_check(r#""probe/*" = "off""#, "probe/lenient", "", POLICY),
        &[
            "\"probe/*\" (level \"off\") sets probe/aspects below its default warning (the preset probe/lenient sets probe/aspects to warning)",
            "write it as \"probe/*\" = { level = \"off\", decision = \"<record>\" }",
        ],
    );
    loads(decision_check(
        r#""probe/*" = { level = "off", decision = "D-2" }"#,
        "probe/lenient",
        "",
        POLICY,
    ));
}

#[test]
fn a_decision_covers_its_own_entry_and_a_project_policy_governs_its_own_file() {
    // The root decided its rule-wide note; the project's file lowers an aspect the root's entry
    // does not name, so it needs its own decision under the root's policy.
    let decided = r#""probe/aspects" = { level = "note", decision = "D-1" }"#;
    let lowered = "\n[rules]\n\"probe/aspects\" = { levels = { quiet = \"note\" } }\n";
    refused(
        decision_check(decided, "probe/lenient", lowered, POLICY),
        &[
            "error: apps/one/gate.toml:",
            "\"probe/aspects\" (levels.quiet \"note\") sets probe/aspects#quiet below its default warning",
            "and gate.toml [decision-policy]",
        ],
    );
    loads(decision_check(
        decided,
        "probe/lenient",
        "\n[rules]\n\"probe/aspects\" = { levels = { quiet = \"note\" }, decision = \"D-3\" }\n",
        POLICY,
    ));

    // A project's own policy governs its own file, over the root's: off, it loads; on, it refuses
    // and names itself.
    let own =
        |required: bool| format!("\n[decision-policy]\nrequire-decision = {required}\n{lowered}");
    loads(decision_check(
        decided,
        "probe/lenient",
        &own(false),
        POLICY,
    ));
    for root in [POLICY, ""] {
        refused(
            decision_check(decided, "probe/lenient", &own(true), root),
            &[
                "error: apps/one/gate.toml:",
                "and apps/one/gate.toml [decision-policy]",
            ],
        );
    }
    // The root's own entries stay under the root's policy, whatever the project's says.
    refused(
        decision_check(
            r#""probe/aspects" = "note""#,
            "probe/lenient",
            &own(false),
            POLICY,
        ),
        &["error: gate.toml:", "and gate.toml [decision-policy]"],
    );

    // Without a policy, or with one that requires nothing, every level loads.
    for root in ["", "\n[decision-policy]\nrequire-decision = false\n"] {
        loads(decision_check(
            r#""probe/aspects" = "off""#,
            "probe/lenient",
            "",
            root,
        ));
        loads(decision_check(
            r#""probe/*" = "off""#,
            "probe/lenient",
            "",
            root,
        ));
        loads(decision_check(
            r#""probe/aspects" = { levels = { quiet = "note" } }"#,
            "probe/quiet",
            "",
            root,
        ));
    }
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

/// `gate check --rule probe/decisions` with the built probe, as JSON: its exit code, and each
/// finding's message with the repository path and line it is reported at, sorted.
fn decisions_check(repo: &Repo) -> (Option<i32>, Vec<String>) {
    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&[
        "check",
        "--plugin",
        &plugin,
        "--format",
        "json",
        "--rule",
        "probe/decisions",
    ]);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("json: {}", text(&output.stderr)));
    let mut seen: Vec<String> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| {
            format!(
                "{} [{}:{}]",
                finding["message"].as_str().unwrap_or_default(),
                finding["location"]["path"].as_str().unwrap_or_default(),
                finding["location"]["span"]["line"]
            )
        })
        .collect();
    seen.sort();
    (output.status.code(), seen)
}

/// Project `one`'s own `gate.toml`, naming a decision on a family and on a rule entry, the first
/// with white space around it.
const ONE_DECISIONS: &str = "[project]\nname = \"one\"\n\n[rules]\n\"probe/*\" = { level = \"off\", decision = \" D-FAM \" }\n\"probe/decisions\" = { level = \"error\", decision = \"D-ONE\" }\n";

#[test]
fn a_project_call_carries_the_decisions_its_own_gate_toml_names_under_any_policy() {
    // The root file is no project's own here: its decision reaches the repository call only, where
    // this project-scope rule does not run. Project two names none, and its call carries none of
    // project one's.
    for policy in [
        "",
        POLICY,
        "\n[decision-policy]\nrequire-decision = false\n",
    ] {
        let repo = probe_repo_for(
            "apps/*",
            r#""probe/decisions" = "error"
"probe/aspects" = { level = "off", decision = "D-ROOT" }"#,
        );
        let mut toml = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
        toml.push_str(policy);
        repo.write("gate.toml", &toml);
        repo.write("apps/one/gate.toml", ONE_DECISIONS);
        repo.commit("decisions in project one");

        assert_eq!(
            decisions_check(&repo),
            (
                Some(1),
                vec![
                    "decision probe/* D-FAM at gate.toml:5 [apps/one/gate.toml:5]".to_owned(),
                    "decision probe/decisions D-ONE at gate.toml:6 [apps/one/gate.toml:6]"
                        .to_owned(),
                ]
            ),
            "policy {policy:?}"
        );
    }
}

#[test]
fn the_root_files_decisions_go_to_the_root_projects_call_only() {
    // The top is a project (no [workspace]); apps/one is one by its own gate.toml. The profile's
    // decision is written in the root file, so it goes to the root project's call although the
    // profile covers apps/one, whose own file names none.
    let repo = Repo::empty();
    let source = "acme/rules:probe@0.1.0";
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins]\nprobe = {{ source = \"{source}\" }}\n\n[rules]\n\"probe/decisions\" = {{ level = \"error\", decision = \"D-ROOT\" }}\n\n[[profiles]]\nprojects = [\"apps/one\"]\n\n[profiles.rules]\n\"probe/decisions\" = {{ level = \"warning\", decision = \"D-PROFILE\" }}\n"
        ),
    );
    repo.write("apps/one/gate.toml", "[project]\nname = \"one\"\n");
    repo.write(
        "gate.lock",
        &lock(
            source,
            "https://example.invalid/probe.wasm",
            &"b".repeat(64),
        ),
    );
    repo.commit("a root project");

    assert_eq!(
        decisions_check(&repo),
        (
            Some(1),
            vec![
                "decision probe/decisions D-PROFILE at gate.toml:14 [gate.toml:14]".to_owned(),
                "decision probe/decisions D-ROOT at gate.toml:8 [gate.toml:8]".to_owned(),
            ]
        )
    );
}

#[test]
fn a_changed_decision_is_not_answered_from_the_result_cache() {
    // The plugin never reads gate.toml, so only the request can tell the cached result is stale.
    let repo = probe_repo_for("apps/*", r#""probe/decisions" = "error""#);
    repo.write("apps/one/gate.toml", ONE_DECISIONS);
    repo.commit("decisions in project one");
    let (_, first) = decisions_check(&repo);
    assert!(
        first.iter().any(|line| line.contains(" D-ONE ")),
        "{first:?}"
    );

    repo.write(
        "apps/one/gate.toml",
        &ONE_DECISIONS.replace("D-ONE", "D-TWO"),
    );
    repo.commit("another decision");
    let (_, second) = decisions_check(&repo);
    assert!(
        second.iter().any(|line| line.contains(" D-TWO ")),
        "{second:?}"
    );
    assert!(
        !second.iter().any(|line| line.contains(" D-ONE ")),
        "{second:?}"
    );
}

/// `gate check` with the built probe and `--rule` for each of `rules`, as JSON: its exit code, and
/// each finding as its project (`-` for the repository), its message and, when it has one, the
/// repository path and line it is reported at, sorted.
fn scope_check(repo: &Repo, rules: &[&str]) -> (Option<i32>, Vec<String>) {
    let plugin = format!("probe={}", probe().0.display());
    let mut arguments = vec!["check", "--plugin", &plugin, "--format", "json"];
    for &rule in rules {
        arguments.extend(["--rule", rule]);
    }
    let output = repo.gate(&arguments);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("json: {}", text(&output.stderr)));
    let mut seen: Vec<String> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| {
            let project = finding["project"].as_str().unwrap_or("-");
            let message = finding["message"].as_str().unwrap_or_default();
            match finding["location"]["path"].as_str() {
                Some(path) => format!(
                    "{project} {message} [{path}:{}]",
                    finding["location"]["span"]["line"]
                ),
                None => format!("{project} {message}"),
            }
        })
        .collect();
    seen.sort();
    (output.status.code(), seen)
}

const SCOPE_RULES: [&str; 3] = [
    "probe/root-decisions",
    "probe/decisions",
    "probe/config-file",
];

/// A workspace whose root is not a project: a decision on a profile entry (line 15) and on a root
/// `[rules]` entry (line 18), and project `one` naming its own at line 5.
fn non_project_root_with_decisions() -> Repo {
    let repo = probe_repo_for(
        "apps/*",
        r#""probe/config-file" = "error"
"probe/decisions" = { level = "error", decision = "D-PROFILE" }"#,
    );
    let mut toml = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    toml.push_str(
        "\n[rules]\n\"probe/root-decisions\" = { level = \"error\", decision = \"D-ROOT\" }\n",
    );
    repo.write("gate.toml", &toml);
    repo.write(
        "apps/one/gate.toml",
        "[project]\nname = \"one\"\n\n[rules]\n\"probe/decisions\" = { level = \"error\", decision = \"D-ONE\" }\n",
    );
    repo.commit("decisions at a root that is not a project");
    repo
}

#[test]
fn a_root_that_is_not_a_project_gives_its_decisions_and_file_to_the_repository_call() {
    // The root file is the repository scope's own: its decisions, of profile and root entries
    // alike, arrive in the repository call with config-file gate.toml, and in no project's call.
    // Each project's call names its own file, relative to its root, and only its own decisions.
    let repo = non_project_root_with_decisions();
    assert_eq!(
        scope_check(&repo, &SCOPE_RULES),
        (
            Some(1),
            vec![
                "- config-file gate.toml".to_owned(),
                "- decision probe/decisions D-PROFILE at gate.toml:15 [gate.toml:15]".to_owned(),
                "- decision probe/root-decisions D-ROOT at gate.toml:18 [gate.toml:18]".to_owned(),
                "one config-file gate.toml".to_owned(),
                "one decision probe/decisions D-ONE at gate.toml:5 [apps/one/gate.toml:5]"
                    .to_owned(),
                "two config-file gate.toml".to_owned(),
            ]
        )
    );
}

#[test]
fn under_a_root_project_the_repository_call_has_no_decisions_and_no_file() {
    // The root file is the root project's: its call carries every decision the file names,
    // repository-scope entries included, and the repository call none, so a rule of each scope
    // never judges the same entry twice.
    let repo = Repo::empty();
    let source = "acme/rules:probe@0.1.0";
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[project]\nname = \"top\"\n\n[plugins]\nprobe = {{ source = \"{source}\" }}\n\n[rules]\n\"probe/root-decisions\" = {{ level = \"error\", decision = \"D-REPO\" }}\n\"probe/config-file\" = \"error\"\n\"probe/decisions\" = {{ level = \"error\", decision = \"D-TOP\" }}\n"
        ),
    );
    repo.write(
        "gate.lock",
        &lock(
            source,
            "https://example.invalid/probe.wasm",
            &"b".repeat(64),
        ),
    );
    repo.commit("a root project");

    assert_eq!(
        scope_check(&repo, &SCOPE_RULES),
        (
            Some(1),
            vec![
                "- config-file -".to_owned(),
                "top config-file gate.toml".to_owned(),
                "top decision probe/decisions D-TOP at gate.toml:13 [gate.toml:13]".to_owned(),
                "top decision probe/root-decisions D-REPO at gate.toml:11 [gate.toml:11]"
                    .to_owned(),
            ]
        )
    );
}

#[test]
fn a_changed_root_decision_is_not_answered_from_the_result_cache() {
    let repo = non_project_root_with_decisions();
    let (_, first) = scope_check(&repo, &["probe/root-decisions"]);
    assert!(
        first.iter().any(|line| line.contains(" D-ROOT ")),
        "{first:?}"
    );

    let toml = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    repo.write("gate.toml", &toml.replace("D-ROOT", "D-NEXT"));
    repo.commit("another root decision");
    let (_, second) = scope_check(&repo, &["probe/root-decisions"]);
    assert!(
        second.iter().any(|line| line.contains(" D-NEXT ")),
        "{second:?}"
    );
    assert!(
        !second.iter().any(|line| line.contains(" D-ROOT ")),
        "{second:?}"
    );
}

/// A workspace whose root is not a project, with the built ledger pinned, `rules` in the root
/// `[rules]` from line 11, project `one`, and the decision index listing `D-API-002` as `status`.
fn ledger_root(rules: &str, status: &str) -> Repo {
    let repo = Repo::empty();
    let source = "acme/rules:ledger@0.1.0";
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[workspace]\nprojects = [\"apps/*\"]\n\n[plugins]\nledger = {{ source = \"{source}\" }}\n\n[rules]\n{rules}"
        ),
    );
    repo.write(
        "gate.lock",
        &lock(
            source,
            "https://example.invalid/ledger.wasm",
            &"c".repeat(64),
        ),
    );
    repo.write("apps/one/gate.toml", "[project]\nname = \"one\"\n");
    repo.write(
        "docs/ledger/decision-index.md",
        &format!("| `D-API-002` | `{status}` | Api | `api.md` |\n"),
    );
    repo
}

/// `gate check` with the built ledger, as JSON: its exit code, and each finding as its rule, the
/// repository path and line it is reported at, and its message, sorted.
fn ledger_check(repo: &Repo) -> (Option<i32>, Vec<String>) {
    let plugin = format!("ledger={}", ledger().0.display());
    let output = repo.gate(&["check", "--plugin", &plugin, "--format", "json"]);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("json: {}", text(&output.stderr)));
    let mut seen: Vec<String> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| {
            format!(
                "{} {}:{} {}",
                finding["rule"].as_str().unwrap_or_default(),
                finding["location"]["path"].as_str().unwrap_or_default(),
                finding["location"]["span"]["line"],
                finding["message"].as_str().unwrap_or_default()
            )
        })
        .collect();
    seen.sort();
    (output.status.code(), seen)
}

const ROOT_ENTRY: &str = "\"ledger/root-rule-decisions\" = \"error\"\n\"ledger/root-exception-owners\" = { level = \"off\", decision = \"D-API-002\" }\n";

#[test]
fn a_root_that_is_not_a_project_is_held_to_the_decisions_its_rule_entries_name() {
    let repo = ledger_root(ROOT_ENTRY, "proposed");
    repo.commit("a root relaxation that names a proposed decision");
    assert_eq!(
        ledger_check(&repo),
        (
            Some(1),
            vec![
                "ledger/root-rule-decisions gate.toml:12 the rule entry \"ledger/root-exception-owners\" names the decision D-API-002, which is `proposed`; a rule entry needs an accepted decision".to_owned()
            ]
        )
    );

    let repo = ledger_root(ROOT_ENTRY, "accepted");
    repo.commit("the decision is accepted");
    assert_eq!(ledger_check(&repo), (Some(0), vec![]));
}

#[test]
fn a_root_that_is_not_a_project_is_held_to_the_decisions_its_exceptions_name() {
    // The root exception accepts the finding of an unlisted decision on a root entry by naming a
    // decision of its own, which must be accepted.
    let rules = "\"ledger/root-rule-decisions\" = \"error\"\n\"ledger/root-exception-decisions\" = \"error\"\n\"ledger/root-exception-owners\" = { level = \"off\", decision = \"D-NOPE-009\" }\n";
    let exception = "[[exception]]\nrule = \"ledger/root-rule-decisions\"\ntarget = \"gate.toml\"\nreason = \"kept for now\"\ndecision = \"D-API-002\"\n";
    let repo = ledger_root(rules, "proposed");
    repo.write("gate.exceptions.toml", exception);
    repo.commit("a root exception that names a proposed decision");
    let (code, seen) = ledger_check(&repo);
    assert_eq!(code, Some(1), "{seen:?}");
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert!(
        seen[0].starts_with("ledger/root-exception-decisions gate.exceptions.toml:1 "),
        "{seen:?}"
    );
    assert!(seen[0].contains("D-API-002"), "{seen:?}");

    let repo = ledger_root(rules, "accepted");
    repo.write("gate.exceptions.toml", exception);
    repo.commit("the exception's decision is accepted");
    assert_eq!(ledger_check(&repo), (Some(0), vec![]));
}

#[test]
fn a_root_that_is_not_a_project_takes_the_root_rules_from_the_recommended_preset() {
    // [workspace] extends takes only the preset's repository-scope rules at such a root.
    for (status, expected) in [
        (
            "proposed",
            (
                Some(1),
                vec!["ledger/root-rule-decisions gate.toml:12 the rule entry \"ledger/root-exception-owners\" names the decision D-API-002, which is `proposed`; a rule entry needs an accepted decision".to_owned()],
            ),
        ),
        ("accepted", (Some(0), vec![])),
    ] {
        let repo = ledger_root(
            "\"ledger/root-exception-owners\" = { level = \"off\", decision = \"D-API-002\" }\n",
            status,
        );
        let toml = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
        repo.write(
            "gate.toml",
            &toml.replace(
                "projects = [\"apps/*\"]\n",
                "projects = [\"apps/*\"]\nextends = [\"ledger/recommended\"]\n",
            ),
        );
        repo.commit("a root that extends the ledger preset");
        assert_eq!(ledger_check(&repo), expected, "{status}");
    }
}

#[test]
fn a_single_project_repository_on_the_recommended_preset_reports_a_decision_once() {
    let repo = Repo::empty();
    let source = "acme/rules:ledger@0.1.0";
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[project]\nname = \"one\"\nextends = [\"ledger/recommended\"]\n\n[plugins]\nledger = {{ source = \"{source}\" }}\n\n[rules]\n\"ledger/manifest\" = {{ level = \"off\", decision = \"D-NOPE-009\" }}\n"
        ),
    );
    repo.write(
        "gate.lock",
        &lock(
            source,
            "https://example.invalid/ledger.wasm",
            &"c".repeat(64),
        ),
    );
    repo.write(
        "docs/ledger/decision-index.md",
        "| `D-API-002` | `accepted` | Api | `api.md` |\n",
    );
    repo.commit("one project on the preset with an unlisted decision");
    let (code, seen) = ledger_check(&repo);
    assert_eq!(code, Some(1), "{seen:?}");
    // The preset's other rules report the ledger files this tree lacks; of the rules about
    // decisions and exceptions, only the project rule reports the entry.
    let judged: Vec<&String> = seen
        .iter()
        .filter(|line| {
            line.starts_with("ledger/rule-decisions ")
                || line.starts_with("ledger/root-")
                || line.starts_with("ledger/exception-")
        })
        .collect();
    assert_eq!(
        judged,
        [
            "ledger/rule-decisions gate.toml:12 the rule entry \"ledger/manifest\" names the decision D-NOPE-009, which docs/ledger/decision-index.md does not list; name an accepted decision of the index"
        ],
        "{seen:?}"
    );
}

#[test]
fn a_single_project_repository_reports_an_unlisted_decision_once() {
    // The root file is the project's: the project rules judge it, and the root rules, with no
    // decisions and no file of their own in the repository call, judge nothing.
    let repo = Repo::empty();
    let source = "acme/rules:ledger@0.1.0";
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins]\nledger = {{ source = \"{source}\" }}\n\n[rules]\n\"ledger/rule-decisions\" = \"error\"\n\"ledger/root-rule-decisions\" = \"error\"\n\"ledger/exception-decisions\" = \"error\"\n\"ledger/root-exception-decisions\" = \"error\"\n\"ledger/exception-owners\" = \"error\"\n\"ledger/root-exception-owners\" = \"error\"\n\"ledger/decisions\" = {{ level = \"off\", decision = \"D-NOPE-009\" }}\n"
        ),
    );
    repo.write(
        "gate.lock",
        &lock(
            source,
            "https://example.invalid/ledger.wasm",
            &"c".repeat(64),
        ),
    );
    repo.write(
        "docs/ledger/decision-index.md",
        "| `D-API-002` | `accepted` | Api | `api.md` |\n",
    );
    repo.write(
        "gate.exceptions.toml",
        "[[exception]]\nrule = \"ledger/rule-decisions\"\ntarget = \"gate.toml\"\nreason = \"kept for now\"\ndecision = \"D-NOPE-008\"\n",
    );
    repo.commit("one project with an unlisted decision on an entry and on an exception");
    let (code, seen) = ledger_check(&repo);
    assert_eq!(code, Some(1), "{seen:?}");
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert!(
        seen[0].starts_with("ledger/exception-decisions gate.exceptions.toml:1 "),
        "{seen:?}"
    );
}

#[test]
fn ledger_rule_decisions_refuses_a_proposed_decision_and_passes_an_accepted_one() {
    let repo = Repo::empty();
    let source = "acme/rules:ledger@0.1.0";
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins]\nledger = {{ source = \"{source}\" }}\n\n[rules]\n\"ledger/rule-decisions\" = \"error\"\n\"ledger/decisions\" = {{ level = \"off\", decision = \"D-API-002\" }}\n"
        ),
    );
    repo.write(
        "gate.lock",
        &lock(
            source,
            "https://example.invalid/ledger.wasm",
            &"c".repeat(64),
        ),
    );
    let index = "docs/ledger/decision-index.md";
    repo.write(index, "| `D-API-002` | `proposed` | Api | `api.md` |\n");
    repo.commit("a relaxation that names a proposed decision");

    let plugin = format!("ledger={}", ledger().0.display());
    let check = || {
        let output = repo.gate(&["check", "--plugin", &plugin]);
        (
            output.status.code(),
            format!("{}{}", text(&output.stdout), text(&output.stderr)),
        )
    };
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("the rule entry \"ledger/decisions\" names the decision D-API-002, which is `proposed`; a rule entry needs an accepted decision"),
        "{said}"
    );
    assert!(said.contains("gate.toml:9"), "{said}");

    repo.write(index, "| `D-API-002` | `accepted` | Api | `api.md` |\n");
    repo.commit("the decision is accepted");
    let (code, said) = check();
    assert_eq!(code, Some(0), "{said}");
}

/// A single-project repository with the `plugin` plugin pinned (the tests run a local build of it)
/// and `rules`, then `[generate]` entries `generate` when not empty, over a README only.
fn doc_repo(plugin: &str, rules: &str, generate: &str) -> Repo {
    let repo = Repo::empty();
    let source = format!("acme/rules:{plugin}@0.1.0");
    let generate = if generate.is_empty() {
        String::new()
    } else {
        format!("\n[generate]\n{generate}")
    };
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins]\n{plugin} = {{ source = \"{source}\" }}\n\n[rules]\n{rules}{generate}"
        ),
    );
    repo.write(
        "gate.lock",
        &lock(
            &source,
            &format!("https://example.invalid/{plugin}.wasm"),
            &"c".repeat(64),
        ),
    );
    repo.write("README.md", "# Project\n");
    repo.commit("a project without its docs");
    repo
}

/// Exit code and output of `gate <arguments>` with the local build `wasm` of `plugin`.
fn doc_run(repo: &Repo, plugin: &str, wasm: &Path, arguments: &[&str]) -> (Option<i32>, String) {
    let local = format!("{plugin}={}", wasm.display());
    let mut arguments = arguments.to_vec();
    arguments.extend(["--plugin", local.as_str()]);
    let output = repo.gate(&arguments);
    (
        output.status.code(),
        format!("{}{}", text(&output.stdout), text(&output.stderr)),
    )
}

#[test]
fn ledger_rules_on_a_project_without_a_ledger_fail_naming_every_file_they_read() {
    let repo = doc_repo(
        "ledger",
        concat!(
            "\"ledger/manifest\" = \"error\"\n",
            "\"ledger/decisions\" = \"error\"\n",
            "\"ledger/records\" = \"error\"\n",
            "\"ledger/blockers\" = \"error\"\n",
            "\"ledger/closing-notes\" = { level = \"error\", options = { field = \"`TEMPORARY_DELTA` records\" } }\n",
            "\"ledger/reuses\" = { level = \"error\", options = { catalog = \"docs/reference/mechanisms.md\" } }\n",
            "\"ledger/exception-owners\" = \"error\"\n",
            "\"ledger/exception-decisions\" = \"error\"\n",
            "\"ledger/rule-decisions\" = \"error\"\n",
            "\"ledger/root-exception-owners\" = \"error\"\n",
            "\"ledger/root-exception-decisions\" = \"error\"\n",
            "\"ledger/root-rule-decisions\" = \"error\"\n",
        ),
        "",
    );
    let (code, said) = doc_run(&repo, "ledger", &ledger().0, &["check"]);
    assert_eq!(code, Some(1), "{said}");
    for (path, rule) in [
        ("docs/ledger/manifest.json", "manifest"),
        ("docs/ledger/decision-index.md", "decisions"),
        ("docs/adr/decision-coverage.md", "decisions"),
        ("docs/ledger/implementation-slices.md", "records"),
        ("docs/ledger/implementation-slices.md", "blockers"),
        ("docs/ledger/implementation-slices.md", "closing-notes"),
        ("docs/ledger/implementation-slices.md", "reuses"),
        ("docs/reference/mechanisms.md", "reuses"),
        ("docs/ledger/decision-index.md", "reuses"),
    ] {
        assert!(
            said.contains(&format!(
                "{path} is not tracked, and ledger/{rule} reads it"
            )),
            "{path} for ledger/{rule}: {said}"
        );
    }
    assert_eq!(said.matches("#input]").count(), 9, "{said}");
    assert!(!said.contains("ledger/exception-"), "{said}");
    assert!(!said.contains("ledger/rule-decisions"), "{said}");
    assert!(!said.contains("ledger/root-"), "{said}");
}

#[test]
fn an_input_level_of_off_turns_off_the_input_findings_of_that_rule_only() {
    let repo = doc_repo(
        "ledger",
        concat!(
            "\"ledger/records\" = { level = \"error\", levels = { input = \"off\" } }\n",
            "\"ledger/blockers\" = \"error\"\n",
        ),
        "",
    );
    let (code, said) = doc_run(&repo, "ledger", &ledger().0, &["check"]);
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains("ledger/blockers#input"), "{said}");
    assert!(!said.contains("ledger/records"), "{said}");

    let repo = doc_repo(
        "ledger",
        "\"ledger/records\" = { level = \"error\", levels = { input = \"off\" } }\n",
        "",
    );
    let (code, said) = doc_run(&repo, "ledger", &ledger().0, &["check"]);
    assert_eq!(code, Some(0), "{said}");
}

#[test]
fn a_ledger_rule_without_the_option_it_cannot_run_without_is_a_configuration_error() {
    for (rule, named) in [
        ("closing-notes", "field"),
        ("reuses", "catalog"),
        ("record-status", "options has no parity"),
    ] {
        let repo = doc_repo("ledger", &format!("\"ledger/{rule}\" = \"error\"\n"), "");
        let (code, said) = doc_run(&repo, "ledger", &ledger().0, &["check"]);
        assert_eq!(code, Some(2), "{rule}: {said}");
        assert!(said.contains(rule), "{rule}: {said}");
        assert!(said.contains(named), "{rule}: {said}");
    }
}

#[test]
fn agents_rules_and_the_agents_block_fail_without_agent_docs_and_pass_with_them() {
    let rules = concat!(
        "\"agents/rule-registry\" = \"error\"\n",
        "\"agents/instruction-references\" = \"error\"\n",
    );
    let generate = "\"agents/agents-block\" = { options = { command-text = \"gate sync\" } }\n";
    let repo = doc_repo("agents", rules, generate);
    let wasm = &agents().0;

    let (code, said) = doc_run(&repo, "agents", wasm, &["check"]);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("docs/agents/rules has no rule file, and agents/rule-registry reads"),
        "{said}"
    );
    assert!(
        said.contains("AGENTS.md is not tracked, and agents/instruction-references reads it"),
        "{said}"
    );
    // The entry is seeded, and `sync --check` never reports a seed; the rule files are reported.
    let (code, said) = doc_run(&repo, "agents", wasm, &["sync", "--check"]);
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains("gate/sync#agents/agents-block"), "{said}");
    assert!(
        said.contains("docs/agents/rules has no rule file, and agents/agents-block reads"),
        "{said}"
    );
    assert!(!said.contains("AGENTS.md"), "{said}");

    // An entry with the markers and no rule file: only the rule files are missing.
    repo.write(
        "AGENTS.md",
        "# Agents\n\n<!-- gate:begin agents -->\n<!-- gate:end agents -->\n",
    );
    repo.commit("the entry");
    let (code, said) = doc_run(&repo, "agents", wasm, &["check"]);
    assert_eq!(code, Some(1), "{said}");
    assert_eq!(said.matches("#input]").count(), 1, "{said}");
    assert!(said.contains("agents/rule-registry#input"), "{said}");

    // One valid rule file and the block written from it: nothing is reported. `gate sync` reads
    // the committed tree, so the rule file is committed before the block is written.
    repo.write(
        "docs/agents/rules/api.md",
        "---\narea: API\napplies: always\nowner: D-ONE-001\n---\n# Rules\n\n## RULE-API-001 Rule 1\n\nEnforcement: gate · Status: stable\n\n",
    );
    repo.commit("a rule file");
    let (code, said) = doc_run(&repo, "agents", wasm, &["sync"]);
    assert_eq!(code, Some(0), "{said}");
    repo.commit("its block");
    let (code, said) = doc_run(&repo, "agents", wasm, &["check"]);
    assert_eq!(code, Some(0), "{said}");
    let (code, said) = doc_run(&repo, "agents", wasm, &["sync", "--check"]);
    assert_eq!(code, Some(0), "{said}");
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
    let git = component("gate-plugin-git", "git");
    let agents = component("gate-plugin-agents", "agents");

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
    let git = component("gate-plugin-git", "git");
    let agents = component("gate-plugin-agents", "agents");

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
    let git = component("gate-plugin-git", "git");
    let agents = component("gate-plugin-agents", "agents");
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
            "unknown key colour; the keys are label",
        ),
        ("{ label = 5 }", "/label: 5 is not a string"),
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
    let git = component("gate-plugin-git", "git");
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
            && message
                .contains("main is required; set it in the [plugins.git] options in gate.toml")
            && !message.contains("one of"),
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

/// The git plugin built from this tree.
fn git_component() -> &'static (PathBuf, Vec<u8>) {
    component("gate-plugin-git", "git")
}

/// The commit `name` names in `repo`.
fn revision(repo: &Repo, name: &str) -> String {
    let output = isolated(
        Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["rev-parse", name]),
    )
    .output()
    .expect("git");
    text(&output.stdout).trim().to_owned()
}

/// A repository with `git/commit-message` on, where branch `feat/the-thing` has pushed one commit
/// and main has since moved on with a commit of another form, a web edit: the repository, the
/// `--plugin` argument, the pushed commit and main's new commit. The branch is checked out.
fn diverged() -> (Repo, String, String, String) {
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins.git]\nsource = \"acme/rules:git@0.1.0\"\noptions = {{ main = [\"refs/heads/main\"] }}\n\n[rules]\n\"git/commit-message\" = \"error\"\n"
        ),
    );
    repo.commit("chore(repo): configure the gate");
    repo.git(&["checkout", "-q", "-b", "feat/the-thing"]);
    repo.write("a.txt", "a\n");
    let pushed = repo.commit("feature(core): add the thing");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("README.md", "# r\n");
    let web = repo.commit("Update README.md");
    repo.git(&["checkout", "-q", "feat/the-thing"]);
    (
        repo,
        format!("git={}", git_component().0.display()),
        pushed,
        web,
    )
}

/// The commits the `git/commit-message` findings of a push of `base..head` to `target` name.
fn pushed_targets(
    repo: &Repo,
    plugin: &str,
    base: &str,
    head: &str,
    target: &str,
) -> BTreeSet<String> {
    let range = format!("{base}..{head}");
    let output = repo.gate(&[
        "check",
        "--plugin",
        plugin,
        "--push",
        &range,
        "--target-ref",
        target,
        "--format",
        "json",
    ]);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("a JSON report: {}", text(&output.stderr)));
    report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter(|finding| finding["rule"] == "git/commit-message")
        .filter_map(|finding| finding["target"].as_str())
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_push_after_a_merge_of_main_judges_only_the_branch_s_new_commits() {
    let (repo, plugin, pushed, web) = diverged();
    repo.git(&[
        "merge",
        "-q",
        "--no-ff",
        "-m",
        "Merge branch 'main' into feat/the-thing",
        "main",
    ]);
    repo.write("b.txt", "b\n");
    let next = repo.commit("feature(core): the next thing");
    let targets = pushed_targets(&repo, &plugin, &pushed, &next, "refs/heads/feat/the-thing");
    assert!(!targets.contains(&web), "main's web edit is not judged");
    assert_eq!(targets, BTreeSet::from([next]));
}

#[test]
fn a_rebase_and_force_push_judges_only_the_branch_s_rewritten_commits() {
    let (repo, plugin, pushed, web) = diverged();
    repo.git(&["rebase", "-q", "main"]);
    let rebased = revision(&repo, "HEAD");
    assert_ne!(rebased, pushed, "the rebase rewrote the commit");
    let targets = pushed_targets(
        &repo,
        &plugin,
        &pushed,
        &rebased,
        "refs/heads/feat/the-thing",
    );
    assert!(!targets.contains(&web), "main's web edit is not judged");
    assert_eq!(targets, BTreeSet::from([rebased]));
}

/// `diverged()` with the pushed commit, which others already have, accepted by an exception keyed
/// by its id, as the rule's page says, committed on the branch: the repository, the `--plugin`
/// argument, the pushed commit and the commit that adds the exception.
fn accepted_pushed_commit() -> (Repo, String, String, String) {
    let (repo, plugin, pushed, _) = diverged();
    repo.write(
        "gate.exceptions.toml",
        &format!(
            "[[exception]]\nrule = \"git/commit-message\"\ntarget = \"{pushed}\"\nreason = \"pushed before the hooks were on\"\n"
        ),
    );
    let accepted = repo.commit("chore(repo): accept the pushed commit");
    (repo, plugin, pushed, accepted)
}

/// The `gate/exceptions` findings of `gate check` with `arguments`, and its exit code.
fn unmatched_exceptions(repo: &Repo, plugin: &str, arguments: &[&str]) -> (Vec<String>, i32) {
    let mut all = vec!["check", "--plugin", plugin, "--format", "json"];
    all.extend_from_slice(arguments);
    let output = repo.gate(&all);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("a JSON report: {}", text(&output.stderr)));
    let unmatched = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter(|finding| finding["rule"] == "gate/exceptions")
        .map(|finding| finding["message"].as_str().unwrap_or_default().to_owned())
        .collect();
    (unmatched, output.status.code().unwrap_or(-1))
}

#[test]
fn an_exception_for_a_pushed_commit_is_not_due_in_a_run_that_does_not_judge_it() {
    let (repo, plugin, _, accepted) = accepted_pushed_commit();

    // pre-commit: no commit is judged.
    repo.write("c.txt", "c\n");
    repo.git(&["add", "c.txt"]);
    assert_eq!(
        unmatched_exceptions(&repo, &plugin, &["--staged"]),
        (Vec::new(), 0),
        "pre-commit"
    );

    // commit-msg: the new message is judged, not the pushed commit.
    let outside = tempfile::tempdir().expect("temporary directory");
    let message = outside.path().join("COMMIT_EDITMSG");
    std::fs::write(&message, "feat(core): add c\n").expect("message");
    let at = message.display().to_string();
    assert_eq!(
        unmatched_exceptions(&repo, &plugin, &["--commit-message", &at]),
        (Vec::new(), 0),
        "commit-msg"
    );

    // pre-push of the exception's commit alone: the pushed commit is already on the remote.
    let parent = revision(&repo, &format!("{accepted}^"));
    let range = format!("{parent}..{accepted}");
    let push = [
        "--push",
        range.as_str(),
        "--target-ref",
        "refs/heads/feat/the-thing",
    ];
    assert_eq!(
        unmatched_exceptions(&repo, &plugin, &push),
        (Vec::new(), 0),
        "a push that does not hold the commit"
    );
}

#[test]
fn an_exception_for_a_commit_the_push_judges_and_accepts_is_reported() {
    let (repo, plugin, _, _) = diverged();
    repo.write("c.txt", "c\n");
    let good = repo.commit("feat(core): add c");
    repo.write(
        "gate.exceptions.toml",
        &format!(
            "[[exception]]\nrule = \"git/commit-message\"\ntarget = \"{good}\"\nreason = \"never needed\"\n"
        ),
    );
    let accepted = repo.commit("chore(repo): accept c");
    let range = format!("{}..{accepted}", revision(&repo, "main"));
    let push = [
        "--push",
        range.as_str(),
        "--target-ref",
        "refs/heads/feat/the-thing",
    ];
    let (unmatched, code) = unmatched_exceptions(&repo, &plugin, &push);
    assert_eq!(code, 1, "{unmatched:?}");
    assert_eq!(
        unmatched,
        [format!(
            "the exception for git/commit-message at {good} matches no finding"
        )]
    );
}

/// `gate check --staged --timings` of the git plugin: the finding messages, sorted, and whether
/// each git call came from the result cache.
fn timed_git(repo: &Repo, plugin: &str) -> (Vec<String>, Vec<bool>) {
    let output = repo.gate(&[
        "check",
        "--staged",
        "--plugin",
        plugin,
        "--format",
        "json",
        "--timings",
    ]);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("a JSON report: {}", text(&output.stderr)));
    let mut messages: Vec<String> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter_map(|finding| finding["message"].as_str().map(str::to_owned))
        .collect();
    messages.sort();
    let stderr = text(&output.stderr);
    let cached: Vec<bool> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("timings: call git "))
        .map(|call| call.contains(": cached, "))
        .collect();
    assert!(!cached.is_empty(), "--timings names each call: {stderr}");
    (messages, cached)
}

#[test]
fn a_changed_exception_list_is_not_answered_from_the_result_cache() {
    let (repo, plugin, pushed, _) = accepted_pushed_commit();
    let configured = revision(&repo, &format!("{pushed}^"));
    // Only the index changes from run to run, never HEAD, which the call's read set records: a
    // stored result is told apart by its key alone.
    repo.write("c.txt", "c\n");
    repo.git(&["add", "c.txt"]);
    // The run judges the excepted commit: its finding is accepted.
    assert_eq!(timed_git(&repo, &plugin), (Vec::new(), vec![false]));
    assert_eq!(
        timed_git(&repo, &plugin),
        (Vec::new(), vec![true]),
        "an unchanged run is answered from the result cache"
    );
    // The exception now names a good commit: the call runs again, judges that commit and no
    // longer the pushed one, so the exception is stale and the pushed commit has no finding.
    repo.write(
        "gate.exceptions.toml",
        &format!(
            "[[exception]]\nrule = \"git/commit-message\"\ntarget = \"{configured}\"\nreason = \"never needed\"\n"
        ),
    );
    repo.git(&["add", "gate.exceptions.toml"]);
    assert_eq!(
        timed_git(&repo, &plugin),
        (
            vec![format!(
                "the exception for git/commit-message at {configured} matches no finding"
            )],
            vec![false]
        )
    );
}

#[test]
fn an_exception_for_a_commit_the_history_no_longer_holds_is_reported() {
    let (repo, plugin, pushed, _) = accepted_pushed_commit();
    // The branch is rebased onto main: the exception names a commit the history no longer holds,
    // which no run judges again.
    repo.git(&["rebase", "-q", "main"]);
    let rewritten = revision(&repo, "HEAD~1");
    assert_ne!(rewritten, pushed, "the rebase rewrote the commit");
    repo.write("c.txt", "c\n");
    repo.git(&["add", "c.txt"]);
    let (unmatched, code) = unmatched_exceptions(&repo, &plugin, &["--staged"]);
    assert_eq!(code, 1, "{unmatched:?}");
    assert_eq!(
        unmatched,
        [format!(
            "the exception for git/commit-message at {pushed} matches no finding"
        )]
    );
}

#[test]
fn an_excepted_commit_re_judged_outside_its_push_brings_no_finding_its_exception_does_not_cover() {
    let rules = |trailers: &str| {
        format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins.git]\nsource = \"acme/rules:git@0.1.0\"\noptions = {{ main = [\"refs/heads/main\"] }}\n\n[rules]\n\"git/commit-message\" = {{ level = \"error\", options = {{ header-max-length = 40{trailers} }} }}\n"
        )
    };
    let repo = Repo::empty();
    repo.write("gate.toml", &rules(""));
    repo.commit("chore(repo): configure");
    repo.git(&["checkout", "-q", "-b", "feat/the-thing"]);
    repo.write("a.txt", "a\n");
    // Pushed before the hooks were on: its header is too long, and it carries a trailer.
    let pushed = repo.commit(
        "feat(core): add the thing with a long header\n\nThe body says why.\n\nProhibited: yes",
    );
    repo.write(
        "gate.exceptions.toml",
        &format!(
            "[[exception]]\nrule = \"git/commit-message#header-max-length\"\ntarget = \"{pushed}\"\nreason = \"pushed before the hooks were on\"\n"
        ),
    );
    repo.commit("chore(repo): accept it");
    // Later, a pattern the pushed commit's trailer matches.
    repo.write(
        "gate.toml",
        &rules(", prohibited-trailers = [\"^Prohibited\"]"),
    );
    let configured = repo.commit("chore(repo): prohibit a trailer");
    let plugin = format!("git={}", git_component().0.display());
    let commit_messages = |arguments: &[&str]| -> (Vec<String>, i32) {
        let mut all = vec!["check", "--plugin", plugin.as_str(), "--format", "json"];
        all.extend_from_slice(arguments);
        let output = repo.gate(&all);
        let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
            .unwrap_or_else(|_| panic!("a JSON report: {}", text(&output.stderr)));
        let found = report["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .map(|finding| {
                format!(
                    "{}#{}",
                    finding["rule"].as_str().unwrap_or_default(),
                    finding["aspect"].as_str().unwrap_or_default()
                )
            })
            .collect();
        (found, output.status.code().unwrap_or(-1))
    };

    // pre-commit and commit-msg re-judge the excepted commit: its header finding is accepted, and
    // the trailer, which no exception covers, is not this run's to report.
    repo.write("c.txt", "c\n");
    repo.git(&["add", "c.txt"]);
    assert_eq!(
        commit_messages(&["--staged"]),
        (Vec::new(), 0),
        "pre-commit"
    );
    let outside = tempfile::tempdir().expect("temporary directory");
    let message = outside.path().join("COMMIT_EDITMSG");
    std::fs::write(&message, "feat(core): add c\n").expect("message");
    let at = message.display().to_string();
    assert_eq!(
        commit_messages(&["--commit-message", &at]),
        (Vec::new(), 0),
        "commit-msg"
    );

    // The push that brings the commit judges it in full and reports the trailer.
    let range = format!("{}..{configured}", revision(&repo, "main"));
    assert_eq!(
        commit_messages(&[
            "--push",
            &range,
            "--target-ref",
            "refs/heads/feat/the-thing"
        ]),
        (vec!["git/commit-message#prohibited-trailers".to_owned()], 1),
        "the push"
    );
}

impl GitOptions {
    fn new() -> Self {
        let git = git_component();
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
fn branch_name_options_outside_its_forms_name_the_rule_the_path_and_the_forms() {
    let git = GitOptions::new();
    let rule = |options: &str| {
        format!("\"git/branch-name\" = {{ level = \"error\", options = {options} }}")
    };
    for (options, expected) in [
        (
            "{}",
            "[rules]: \"git/branch-name\" options: sets none of template or pattern; set exactly one of them",
        ),
        (
            r#"{ pattern = "^main$", template = ":type/:name" }"#,
            "\"git/branch-name\" options: sets template and pattern; set exactly one of template or pattern",
        ),
        (
            r#"{ pattern = "^main$", subject = true }"#,
            "\"git/branch-name\" options: /subject: true is not a string or a table",
        ),
        (
            r#"{ pattern = "^main$", subject = { name = 3 } }"#,
            "\"git/branch-name\" options: /subject/name: 3 is not a string",
        ),
    ] {
        git.refused("", &rule(options), &[], expected);
        let message = text(&git.check("", &rule(options), &[]).stderr);
        assert!(
            !message.contains("schemas listed in the"),
            "{options}: {message}"
        );
    }
    git.passes("", &rule(r#"{ pattern = "^main$" }"#), &[]);
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
            && said.contains("gate check (commit message): "),
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

#[test]
fn a_project_that_extends_an_adoption_preset_is_told_at_warning_and_passes() {
    let (path, _) = component("gate-plugin-ci", "ci");
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[project]\nname = \"one\"\nextends = [\"ci/adoption\"]\n\n[plugins.ci]\nsource = \"acme/rules:ci@0.1.0\"\n"
        ),
    );
    repo.write(
        ".github/workflows/ci.yml",
        "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
    );
    repo.commit("configure");
    let plugin = format!("ci={}", path.display());
    let output = repo.gate(&["check", "--plugin", &plugin]);
    let said = text(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{said}");
    assert!(
        said.contains("warning[ci/actions-pinned") && !said.contains("error["),
        "{said}"
    );
}

#[test]
fn a_warm_result_cache_reruns_the_rule_for_another_message_or_hook_state() {
    let git = GitOptions::new();
    let repo = &git.repo;
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins.git]\nsource = \"acme/rules:git@0.1.0\"\noptions = {{ main = [\"refs/heads/main\"] }}\n\n[rules]\n\"git/commit-message\" = \"error\"\n\"git/branch-name\" = {{ level = \"error\", options = {{ pattern = \"^feature/.+$\" }} }}\n"
        ),
    );
    repo.commit("configure");
    // The exit code, how many git calls ran rather than came from the result cache, and what
    // the run said.
    let run = |arguments: &[&str]| {
        let mut all = vec!["check", "--plugin", git.plugin.as_str(), "--timings"];
        all.extend_from_slice(arguments);
        let output = repo.gate(&all);
        let stderr = text(&output.stderr);
        let ran = stderr
            .lines()
            .filter(|line| line.starts_with("timings: call git ") && line.contains(": ran "))
            .count();
        (
            output.status.code(),
            ran,
            format!("{}{stderr}", text(&output.stdout)),
        )
    };

    // One message file, outside the tree, whose text changes: the same request each time.
    let outside = tempfile::tempdir().expect("temporary directory");
    let file = outside.path().join("COMMIT_EDITMSG");
    let at = file.display().to_string();
    let message = |body: &str| {
        std::fs::write(&file, body).expect("message");
        run(&["--rule", "git/commit-message", "--commit-message", &at])
    };
    let (code, ran, said) = message("feat(core): add the thing\n");
    assert_eq!((code, ran), (Some(0), 1), "cold: {said}");
    let (code, ran, said) = message("feat(core): add the thing\n");
    assert_eq!((code, ran), (Some(0), 0), "cached: {said}");
    let (code, ran, said) = message("Added the thing\n");
    assert_eq!(
        (code, ran),
        (Some(1), 1),
        "another message runs the rule again: {said}"
    );
    assert!(said.contains("git/commit-message#type-empty"), "{said}");

    // Outside a hook main passes; in a hook it is checked like any branch.
    let branch = |hook: &[&str]| {
        let mut arguments = vec!["--rule", "git/branch-name"];
        arguments.extend_from_slice(hook);
        run(&arguments)
    };
    let (code, ran, said) = branch(&[]);
    assert_eq!((code, ran), (Some(0), 1), "{said}");
    let (code, ran, said) = branch(&[]);
    assert_eq!((code, ran), (Some(0), 0), "warm: {said}");
    let (code, ran, said) = branch(&["--hook"]);
    assert_eq!(
        (code, ran),
        (Some(1), 1),
        "a hook run runs the rule again: {said}"
    );
    assert!(said.contains("git/branch-name"), "{said}");
}

#[test]
fn a_message_past_the_read_set_limit_is_recorded_by_its_digest() {
    let git = GitOptions::new();
    // Under `git commit --verbose` the message file holds the diff below the scissors line; this
    // one is larger than the 64 MiB of strings a call's read set may hold.
    let line = "+ a line of the staged diff\n";
    let message = format!(
        "feat(core): add the thing\n# ------------------------ >8 ------------------------\n{}",
        line.repeat((65 << 20) / line.len() + 1)
    );
    let outside = tempfile::tempdir().expect("temporary directory");
    let file = outside.path().join("COMMIT_EDITMSG");
    std::fs::write(&file, message).expect("message");
    let output = git.check(
        "",
        "\"git/commit-message\" = \"error\"",
        &["--commit-message", &file.display().to_string()],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
}

#[test]
fn a_rules_bad_option_is_a_configuration_error_and_a_missing_plugin_an_environment_error() {
    // The hello plugin: its marker passes the schema and is refused by the rule.
    let repo = Repo::pinned(r#"{ marker = "" }"#);
    let output = repo.gate(&["check"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let message = text(&output.stderr);
    assert!(
        message.starts_with("gate: configuration error:") && message.contains("marker is empty"),
        "{message}"
    );

    // The git plugin: a header-pattern that does not compile.
    let git = GitOptions::new();
    let outside = tempfile::tempdir().expect("temporary directory");
    let file = outside.path().join("COMMIT_EDITMSG");
    std::fs::write(&file, "feat(core): add the thing\n").expect("message");
    let at = file.display().to_string();
    let output = git.check(
        "",
        r#""git/commit-message" = { level = "error", options = { header-pattern = "(" } }"#,
        &["--commit-message", &at],
    );
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let message = text(&output.stderr);
    assert!(
        message.starts_with("gate: configuration error:")
            && message.contains("header-pattern \"(\" is not a valid regular expression"),
        "{message}"
    );

    // A plugin file that is not there stays an environment error.
    let missing = outside.path().join("git.wasm");
    let output = git.repo.gate(&[
        "check",
        "--plugin",
        &format!("git={}", missing.display()),
        "--commit-message",
        &at,
    ]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let message = text(&output.stderr);
    assert!(
        message.starts_with("gate: environment error:") && message.contains("cannot read"),
        "{message}"
    );
}

#[test]
fn a_plugins_own_environment_error_keeps_its_label() {
    // git/no-ci-skip with a main reference the repository does not have: the plugin's
    // environment error (a fetch may fix it), which the runtime and the host keep as one.
    let git = GitOptions::new();
    git.repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins.git]\nsource = \"acme/rules:git@0.1.0\"\noptions = {{ main = [\"refs/heads/trunk\"], activation = \"{}\", forms = [{{ kind = \"direct\", parents = 1, checks = [] }}] }}\n\n[rules]\n{NO_CI_SKIP}\n",
            git.root
        ),
    );
    git.repo.commit("configure");
    let output = git.repo.gate(&["check", "--plugin", &git.plugin]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let message = text(&output.stderr);
    assert!(
        message.starts_with("gate: environment error:")
            && message.contains("none of the main references"),
        "{message}"
    );
}

#[test]
fn a_configured_tsconfig_that_is_not_tracked_is_a_configuration_error() {
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[project]\nname = \"app\"\n\n[plugins]\ntypescript = {{ source = \"acme/rules:typescript@0.1.0\" }}\n\n[languages.typescript]\ntsconfig = \"tsconfig.app.json\"\n\n[rules]\n\"typescript/barrel-imports\" = {{ level = \"error\" }}\n"
        ),
    );
    repo.write(
        "src/accounts/index.ts",
        "export { account } from \"./account\";\n",
    );
    repo.write("src/accounts/account.ts", "export const account = 1;\n");
    repo.write("src/main.ts", "import { account } from \"./accounts\";\n");
    repo.commit("init");
    let plugin = format!("typescript={}", typescript().0.display());
    let output = repo.gate(&["check", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let message = text(&output.stderr);
    assert!(
        message.starts_with("gate: configuration error:")
            && message.contains("tsconfig names tsconfig.app.json, which is not tracked"),
        "{message}"
    );
}

/// A root project `app` that runs `typescript/barrel-imports`, and a nested project `cdk` in
/// `infra/cdk` with a `gate.toml` of its own whose `[rules]` are `nested_rules`: each has a barrel
/// folder and a file that imports past it, and a root file imports past the nested project's
/// barrel. With `exceptions`, the root's `gate.exceptions.toml`. Committed.
fn nested_barrel_repo(nested_rules: &str, exceptions: Option<&str>) -> Repo {
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[project]\nname = \"app\"\n\n[plugins]\ntypescript = {{ source = \"acme/rules:typescript@0.1.0\" }}\n\n[rules]\n\"typescript/barrel-imports\" = {{ level = \"error\" }}\n"
        ),
    );
    repo.write("tsconfig.json", "{}\n");
    repo.write("src/lib/index.ts", "export { shown } from \"./shown\";\n");
    repo.write("src/lib/shown.ts", "export const shown = 1;\n");
    repo.write("src/lib/deep.ts", "export const deep = 2;\n");
    repo.write("src/main.ts", "import { deep } from \"./lib/deep\";\n");
    repo.write(
        "src/infra.ts",
        "import { inner } from \"../infra/cdk/lib/inner\";\n",
    );
    repo.write(
        "infra/cdk/gate.toml",
        &format!("[project]\nname = \"cdk\"\n{nested_rules}"),
    );
    repo.write("infra/cdk/tsconfig.json", "{}\n");
    repo.write(
        "infra/cdk/lib/index.ts",
        "export { stack } from \"./stack\";\n",
    );
    repo.write("infra/cdk/lib/stack.ts", "export const stack = 1;\n");
    repo.write("infra/cdk/lib/inner.ts", "export const inner = 2;\n");
    repo.write(
        "infra/cdk/bin/app.ts",
        "import { inner } from \"../lib/inner\";\n",
    );
    if let Some(exceptions) = exceptions {
        repo.write("gate.exceptions.toml", exceptions);
    }
    repo.commit("a root project and a nested project, each with a barrel");
    repo
}

/// A finding of a JSON report as (project, rule, path, message), `-` for a field it lacks.
type Reported = (String, String, String, String);

/// What `gate check --timings` with the built typescript plugin reports in `repo`: each finding,
/// sorted, and whether each typescript call came from the result cache.
fn barrel_check(repo: &Repo) -> (Vec<Reported>, Vec<bool>) {
    let output = repo.gate(&[
        "check",
        "--plugin",
        &typescript_plugin(),
        "--format",
        "json",
        "--timings",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    let report: serde_json::Value =
        serde_json::from_str(&text(&output.stdout)).expect("a JSON report");
    let field = |value: &serde_json::Value| value.as_str().unwrap_or("-").to_owned();
    let mut found: Vec<_> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| {
            (
                field(&finding["project"]),
                field(&finding["rule"]),
                field(&finding["location"]["path"]),
                field(&finding["message"]),
            )
        })
        .collect();
    found.sort();
    let stderr = text(&output.stderr);
    let cached: Vec<bool> = stderr
        .lines()
        .filter(|line| line.starts_with("timings: call typescript"))
        .map(|call| call.contains(": cached, "))
        .collect();
    assert!(!cached.is_empty(), "--timings names each call: {stderr}");
    (found, cached)
}

/// A `typescript/barrel-imports` deep-path finding of `project` at `path`, which imports
/// `specifier` (resolving to `target`) past the barrel of `folder`.
fn deep(project: &str, path: &str, specifier: &str, target: &str, folder: &str) -> Reported {
    (
        project.to_owned(),
        "typescript/barrel-imports".to_owned(),
        path.to_owned(),
        format!(
            "{path} imports {specifier} ({target}) past the barrel {folder}/index.ts of {folder}"
        ),
    )
}

#[test]
fn a_root_projects_rule_reports_no_file_a_nested_project_owns() {
    // The rule is on in the root only: the nested project's file is judged by no one, and the
    // root's own findings stay, the one past the nested project's barrel included. A run answered
    // from the result cache reports the same.
    let root = [
        deep(
            "app",
            "src/infra.ts",
            "../infra/cdk/lib/inner",
            "infra/cdk/lib/inner.ts",
            "infra/cdk/lib",
        ),
        deep(
            "app",
            "src/main.ts",
            "./lib/deep",
            "src/lib/deep.ts",
            "src/lib",
        ),
    ];
    let repo = nested_barrel_repo("", None);
    let (cold, cached) = barrel_check(&repo);
    assert_eq!(cold, root);
    assert!(cached.iter().all(|cached| !cached), "{cached:?}");
    let (warm, cached) = barrel_check(&repo);
    assert_eq!(warm, cold);
    assert!(cached.iter().all(|cached| *cached), "{cached:?}");

    // On in both, each finding is reported once, under the project that owns its file.
    let repo = nested_barrel_repo(
        "\n[rules]\n\"typescript/barrel-imports\" = \"error\"\n",
        None,
    );
    // The nested project's call addresses its paths from its own root, as its message shows.
    let mut both = root.to_vec();
    both.push((
        "cdk".to_owned(),
        "typescript/barrel-imports".to_owned(),
        "infra/cdk/bin/app.ts".to_owned(),
        "bin/app.ts imports ../lib/inner (lib/inner.ts) past the barrel lib/index.ts of lib"
            .to_owned(),
    ));
    assert_eq!(barrel_check(&repo).0, both);
}

#[test]
fn a_root_rules_exception_on_a_nested_projects_file_matches_no_finding() {
    // The root accepted the finding its rule gave on the nested project's file; the rule no longer
    // reports there, so the exception is reported as matching nothing.
    let repo = nested_barrel_repo(
        "",
        Some(
            "[[exception]]\nrule = \"typescript/barrel-imports\"\ntarget = \"infra/cdk/bin/app.ts\"\nreason = \"accepted before the nested project judged its own files\"\n",
        ),
    );
    let (found, _) = barrel_check(&repo);
    let unmatched: Vec<_> = found
        .iter()
        .filter(|(_, rule, _, _)| rule == "gate/exceptions")
        .map(|(_, _, path, message)| (path.as_str(), message.as_str()))
        .collect();
    assert_eq!(
        unmatched,
        [(
            "gate.exceptions.toml",
            "the exception for typescript/barrel-imports at infra/cdk/bin/app.ts matches no finding"
        )]
    );
    assert!(
        found
            .iter()
            .all(|(_, _, path, _)| !path.starts_with("infra/")),
        "{found:?}"
    );
}

/// `gate check` in a repository with one tracked TypeScript file and `[plugins.<name>]` declared,
/// the built component of `package` (library `library`) given as `--plugin <name>=<path>`, and
/// `[rules]` `rules`.
fn bad_option_check(package: &str, library: &str, name: &str, rules: &str) -> Output {
    let (path, _) = component(package, library);
    let repo = Repo::empty();
    repo.write("src/app/app.service.ts", "export class AppService {}\n");
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins.{name}]\nsource = \"acme/rules:{name}@0.1.0\"\n\n[rules]\n{rules}\n"
        ),
    );
    repo.commit("configure");
    let plugin = format!("{name}={}", path.display());
    repo.gate(&["check", "--plugin", &plugin])
}

#[test]
fn the_moved_nestjs_string_enums_id_is_an_unknown_rule() {
    // 0.3.0 moved the rule to typescript/string-enums and keeps no alias of the old id.
    let output = bad_option_check(
        "gate-plugin-nestjs",
        "nestjs",
        "nestjs",
        r#""nestjs/string-enums" = "error""#,
    );
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let message = text(&output.stderr);
    assert!(
        message.starts_with("gate: configuration error:")
            && message.contains("plugin nestjs has no rule string-enums"),
        "{message}"
    );
}

#[test]
fn the_built_cdk_component_reports_a_container_without_a_health_check_and_an_unchosen_destroy() {
    let (path, _) = component("gate-plugin-cdk", "cdk");
    let repo = Repo::empty();
    repo.write(
        "tsconfig.json",
        "{ \"compilerOptions\": { \"strict\": true } }\n",
    );
    repo.write(
        "lib/stack/service/service.stack.ts",
        "import { RemovalPolicy, Stack } from \"aws-cdk-lib\";\nimport { FargateTaskDefinition } from \"aws-cdk-lib/aws-ecs\";\n\nexport class ServiceStack extends Stack {\n  build(task: FargateTaskDefinition): void {\n    task.addContainer(\"Web\", { memoryLimitMiB: 512 });\n    this.node.addMetadata(\"policy\", RemovalPolicy.DESTROY);\n  }\n}\n",
    );
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[project]\nname = \"infrastructure\"\n\n[plugins.cdk]\nsource = \"acme/rules:cdk@0.1.0\"\n\n[rules]\n\"cdk/container-health\" = \"error\"\n\"cdk/removal-policy\" = \"error\"\n"
        ),
    );
    repo.commit("configure");
    let plugin = format!("cdk={}", path.display());
    let output = repo.gate(&["check", "--plugin", &plugin]);
    let said = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{said}{}",
        text(&output.stderr)
    );
    assert!(
        said.contains("error[cdk/container-health#missing]")
            && said.contains("defines the container Web without a healthCheck")
            && said.contains("error[cdk/removal-policy#destroy]"),
        "{said}"
    );
}

#[test]
fn a_nestjs_option_its_rule_refuses_is_a_configuration_error() {
    // A pattern that is no regular expression: no schema can refuse it, the rule does.
    let output = bad_option_check(
        "gate-plugin-nestjs",
        "nestjs",
        "nestjs",
        r#""nestjs/eslint-directives" = { level = "error", options = { rule-id-pattern = "(" } }"#,
    );
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let message = text(&output.stderr);
    assert!(
        message.starts_with("gate: configuration error:")
            && message.contains("rule-id-pattern \"(\" is no regular expression"),
        "{message}"
    );
}

/// The frontend plugin's component, built once.
fn frontend() -> &'static (PathBuf, Vec<u8>) {
    component("gate-plugin-frontend", "frontend")
}

/// `gate check` with the built frontend component and its `recommended` preset over `files`, the
/// pack `pack` (none when `None`) and a tsconfig with the one alias.
fn frontend_check(pack: Option<&str>, files: &[(&str, &str)]) -> Output {
    let (path, _) = frontend();
    let repo = Repo::empty();
    repo.write(
        "tsconfig.json",
        r#"{ "compilerOptions": { "paths": { "@/*": ["./src/*"] } } }"#,
    );
    for (file, contents) in files {
        repo.write(file, contents);
    }
    let options = pack.map_or_else(String::new, |pack| {
        format!("options = {{ pack = \"{pack}\" }}\n")
    });
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[project]\nextends = [\"frontend/recommended\"]\n\n[plugins.frontend]\nsource = \"acme/rules:frontend@0.3.0\"\n{options}"
        ),
    );
    repo.commit("configure");
    let plugin = format!("frontend={}", path.display());
    repo.gate(&["check", "--plugin", &plugin])
}

#[test]
fn a_frontend_project_without_a_pack_is_a_configuration_error_that_names_the_packs() {
    let output = frontend_check(None, &[("src/main.tsx", "export {};\n")]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let message = text(&output.stderr);
    assert!(
        message.starts_with("gate: configuration error:")
            && message.contains("has no pack")
            && message.contains("\"react-vite\", \"nextjs\" or \"react-library\""),
        "{message}"
    );
}

#[test]
fn an_unknown_pack_names_the_three_packs_and_an_unknown_preset_names_the_presets() {
    let output = frontend_check(Some("next"), &[("src/main.tsx", "export {};\n")]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let message = text(&output.stderr);
    assert!(
        message.starts_with("gate: configuration error:")
            && message.contains(
                "/pack: \"next\" is not one of \"react-vite\", \"nextjs\" or \"react-library\""
            )
            && !message.contains("anyOf"),
        "{message}"
    );

    let (path, _) = frontend();
    let repo = Repo::empty();
    repo.write("src/main.tsx", "export {};\n");
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[project]\nextends = [\"frontend/strict\"]\n\n[plugins.frontend]\nsource = \"acme/rules:frontend@0.3.0\"\noptions = {{ pack = \"nextjs\" }}\n"
        ),
    );
    repo.commit("configure");
    let plugin = format!("frontend={}", path.display());
    assert_config_error(
        &repo,
        &["check", "--plugin", &plugin],
        "gate.toml:5 [project] extends: plugin frontend has no preset strict; its presets are frontend/adoption and frontend/recommended",
    );
}

#[test]
fn list_and_explain_show_the_rules_a_preset_turns_on_with_the_preset_as_their_origin() {
    let (path, _) = frontend();
    let repo = Repo::empty();
    repo.write("src/main.tsx", "export {};\n");
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[project]\nextends = [\"frontend/adoption\"]\n\n[plugins.frontend]\nsource = \"acme/rules:frontend@0.3.0\"\noptions = {{ pack = \"nextjs\" }}\n\n[rules]\n\"frontend/layout\" = \"error\"\n"
        ),
    );
    repo.commit("configure");
    let plugin = format!("frontend={}", path.display());
    let run = |arguments: &[&str]| {
        let output = repo.gate(arguments);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{arguments:?}: {}",
            text(&output.stderr)
        );
        text(&output.stdout)
    };
    let row = |said: &str, key: &str, value: &str, origin: &str| {
        said.lines().any(|line| {
            let cells: Vec<&str> = line.split("  ").filter(|cell| !cell.is_empty()).collect();
            cells.iter().any(|cell| cell.trim() == key)
                && cells.iter().any(|cell| cell.trim() == value)
                && line.ends_with(origin)
        })
    };
    let extends = "gate.toml:5 [project] extends";
    let preset = "level warning (preset frontend/adoption)";

    // A rule only the preset turns on, with the extends entry as its origin.
    let said = run(&["explain", "frontend/file-names", "--plugin", &plugin]);
    assert!(row(&said, "frontend/file-names", preset, extends), "{said}");
    assert!(!said.contains("configures it"), "{said}");
    // A rule the preset and [rules] both set: both rows, the preset's first.
    let said = run(&["explain", "frontend/layout", "--plugin", &plugin]);
    assert!(row(&said, "frontend/layout", preset, extends), "{said}");
    assert!(
        row(
            &said,
            "frontend/layout",
            "level error",
            "gate.toml:12 [rules]"
        ),
        "{said}"
    );
    assert!(
        said.find(preset) < said.find("level error"),
        "the preset's row comes before the entry that wins over it: {said}"
    );
    // A rule of no plugin the configuration declares.
    let said = run(&["explain", "frontend/nothing", "--plugin", &plugin]);
    assert!(
        said.contains("no gate.toml or preset configures it"),
        "{said}"
    );

    let said = run(&["list", "--origin", "--plugin", &plugin]);
    assert!(row(&said, "frontend/hook-names", preset, extends), "{said}");
    let said = run(&["list", "--format", "json", "--plugin", &plugin]);
    let value: serde_json::Value = serde_json::from_str(&said).expect("JSON");
    let rows = value["project-settings"]
        .as_object()
        .and_then(|projects| projects.values().next())
        .and_then(|project| project["settings"].as_array())
        .expect("the project's settings");
    assert!(
        rows.iter().any(|row| row["key"] == "frontend/file-names"
            && row["preset"] == "frontend/adoption"
            && row["origin"]["line"] == 5),
        "{said}"
    );
    assert!(value["presets-error"].is_null(), "{said}");

    // Without the build, the pin is missing: the list says why the presets' rules are not shown.
    let said = run(&["list"]);
    let last = said.lines().last().unwrap_or_default();
    assert!(
        last.starts_with("the rules the presets turn on are not shown: ")
            && last.contains("gate lock")
            && !said.contains("frontend/file-names"),
        "{said}"
    );
}

#[test]
fn the_frontend_component_passes_each_standard_tree_and_reports_a_departure() {
    let trees: [(&str, &[(&str, &str)]); 3] = [
        (
            "react-vite",
            &[
                (
                    "vite.config.ts",
                    "import { tanstackRouter } from \"@tanstack/router-plugin/vite\";\nexport default { plugins: [tanstackRouter({ routesDirectory: \"./src/app/routes\", generatedRouteTree: \"./src/app/route-tree.gen.ts\" })], resolve: { alias: { \"@\": \"/src\" } } };\n",
                ),
                ("src/main.tsx", "export {};\n"),
                ("src/app/routes/__root.tsx", "export const Route = 1;\n"),
            ],
        ),
        (
            "nextjs",
            &[
                ("pages/README.md", "Keeps Next.js from reading src/pages.\n"),
                (
                    "app/layout.tsx",
                    "export default function Layout() { return null; }\n",
                ),
                (
                    "app/page.tsx",
                    "export default function Page() { return null; }\n",
                ),
            ],
        ),
        (
            "react-library",
            &[
                ("src/index.ts", "export {};\n"),
                (
                    "src/ui/button/button.tsx",
                    "export function Button() { return null; }\n",
                ),
            ],
        ),
    ];

    for (pack, files) in trees {
        let output = frontend_check(Some(pack), files);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{pack}: {}{}",
            text(&output.stdout),
            text(&output.stderr)
        );

        let mut departed = files.to_vec();
        departed.push((
            "src/components/card.tsx",
            "export function Card() { return null; }\n",
        ));
        let output = frontend_check(Some(pack), &departed);
        assert_eq!(
            output.status.code(),
            Some(1),
            "{pack}: {}",
            text(&output.stderr)
        );
        // A library's src/ holds segments, so the folder departs as a segment, not as a layer.
        let aspect = if pack == "react-library" {
            "shared"
        } else {
            "layer"
        };
        let stdout = text(&output.stdout);
        assert!(
            stdout.contains(&format!("error[frontend/layout#{aspect}] src/components")),
            "{pack}: {stdout}"
        );
    }
}

#[test]
fn a_ledger_option_its_rule_refuses_is_a_configuration_error() {
    // A pattern that is no regular expression: no schema can refuse it, the rule does.
    let output = bad_option_check(
        "gate-plugin-ledger",
        "ledger",
        "ledger",
        r#""ledger/reuses" = { level = "error", options = { catalog = "docs/reference/mechanisms.md", generic-reasons = "(" } }"#,
    );
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let message = text(&output.stderr);
    assert!(
        message.starts_with("gate: configuration error:")
            && message.contains("option generic-reasons is not a valid pattern"),
        "{message}"
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
fn a_path_at_a_commit_the_clone_lacks_reads_none_until_a_fetch_brings_it() {
    let other = Repo::empty();
    other.write("apps/one/x.txt", "tip\n");
    let tip = other.commit("tip");
    let repo = probe_repo_for(
        "apps/one",
        &format!(
            r#""probe/git" = {{ level = "error", options = {{ read_at = [["{tip}", "x.txt"]] }} }}"#
        ),
    );
    repo.write("apps/one/a.txt", "one\n");
    repo.commit("first");
    let answer = |text: &str| format!("read-at {tip}:x.txt: {text}");

    let (messages, ran, _) = probe_timings(&repo);
    assert_eq!(ran, ["one"]);
    assert_eq!(messages, [answer("none")]);

    // The fetch brings the commit: the call runs again, and the none is not reused from the
    // stored git answers.
    let source = other.path().to_str().expect("a UTF-8 path");
    repo.git(&["fetch", "-q", source, "main"]);
    let (messages, ran, _) = probe_timings(&repo);
    assert_eq!(ran, ["one"]);
    assert_eq!(messages, [answer("tip")]);

    let (_, ran, _) = probe_timings(&repo);
    assert!(ran.is_empty(), "{ran:?}");
}

#[test]
fn read_at_answers_and_contents_are_stored_and_read_back_without_asking_git() {
    let present = r#"["HEAD~1", "a.txt"]"#;
    let absent = r#"["HEAD~1", "absent.txt"]"#;
    let rule = |first: &str, second: &str| {
        format!(
            r#""probe/git" = {{ level = "error", options = {{ read_at = [{first}, {second}] }} }}"#
        )
    };
    let repo = probe_repo(&rule(present, absent));
    repo.write("apps/one/a.txt", "first\n");
    repo.commit("first");
    repo.write("apps/one/a.txt", "second\n");
    repo.commit("second");
    let config = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    let expected = [
        "read-at HEAD~1:a.txt: first",
        "read-at HEAD~1:absent.txt: none",
    ];
    let answers = |stderr: &str| timing(stderr, "timings: git answers: ");

    let (messages, _, stderr) = probe_timings(&repo);
    assert_eq!(messages, expected);
    assert_eq!(
        answers(&stderr).as_deref(),
        Some("2 asked, 0 from the cache")
    );

    // Other options: the call runs again, and asks git nothing.
    repo.write(
        "gate.toml",
        &config.replace(&rule(present, absent), &rule(absent, present)),
    );
    let (messages, ran, stderr) = probe_timings(&repo);
    assert_eq!(ran, ["one"]);
    assert_eq!(messages, expected);
    assert_eq!(
        answers(&stderr).as_deref(),
        Some("0 asked, 2 from the cache")
    );

    // Without the stored content, the stored answer still reads the file, from git by its id.
    std::fs::remove_dir_all(repo.cache.path().join("git").join("blobs")).expect("stored blobs");
    std::fs::remove_dir_all(repo.cache.path().join("results")).expect("stored results");
    let (messages, ran, stderr) = probe_timings(&repo);
    assert_eq!(ran, ["one"]);
    assert_eq!(messages, expected);
    assert_eq!(
        answers(&stderr).as_deref(),
        Some("0 asked, 2 from the cache")
    );
}

/// A repository whose `apps/one/a.txt` is `first` at `HEAD~1` and `second` at `HEAD`, served to
/// partial clones; the probe reads the file at `HEAD~1`, which a `blob:none` clone lacks.
fn partial_origin() -> Repo {
    let origin = probe_repo(
        r#""probe/git" = { level = "error", options = { read_at = [["HEAD~1", "a.txt"]] } }"#,
    );
    origin.write("apps/one/a.txt", "first\n");
    origin.commit("first");
    origin.write("apps/one/a.txt", "second\n");
    origin.commit("second");
    origin.git(&["config", "uploadpack.allowFilter", "true"]);
    origin
}

/// A `blob:none` clone of `origin`, with its own cache.
fn partial_clone(origin: &Repo) -> Repo {
    let clone = Repo {
        directory: tempfile::tempdir().expect("temporary directory"),
        cache: tempfile::tempdir().expect("temporary cache"),
    };
    let url = format!("file://{}", origin.path().display());
    clone.git(&["clone", "-q", "--filter=blob:none", &url, "."]);
    clone
}

/// Points the clone's promisor remote at `origin`, or at nothing when `origin` is `None`.
fn reach(clone: &Repo, origin: Option<&Repo>) {
    let url = origin.map_or_else(
        || "file:///nonexistent/origin.git".to_owned(),
        |origin| format!("file://{}", origin.path().display()),
    );
    clone.git(&["remote", "set-url", "origin", &url]);
}

/// `gate check` with the built probe in `directory`, the cache `cache` and the environment `env`.
fn probe_output_in(directory: &Path, cache: &Path, env: &[(&str, &str)]) -> Output {
    let plugin = format!("probe={}", probe().0.display());
    isolated(
        Command::new(env!("CARGO_BIN_EXE_gate"))
            .args([
                "check",
                "--plugin",
                &plugin,
                "--format",
                "json",
                "--timings",
            ])
            .current_dir(directory)
            .env("GATE_CACHE_DIR", cache)
            .envs(env.iter().copied()),
    )
    .output()
    .expect("gate")
}

/// Asserts that a check failed with a message that contains `advice`, and that no stored
/// `read-at` answer in `cache` says "absent".
fn failed_storing_no_absent(output: &Output, advice: &str, cache: &Path) {
    let said = format!("{}{}", text(&output.stdout), text(&output.stderr));
    assert_ne!(output.status.code(), Some(0), "{said}");
    assert!(said.contains(advice), "{said}");
    assert!(!said.contains("a.txt: none"), "{said}");
    let stored = stored_read_at(cache);
    assert!(
        stored.iter().all(|line| !line.contains(r#""blob":null"#)),
        "{stored:?}"
    );
}

/// The stored `read-at` answers in `cache`, one JSON line each.
fn stored_read_at(cache: &Path) -> Vec<String> {
    let mut lines = Vec::new();
    let mut directories = vec![cache.join("git")];
    while let Some(directory) = directories.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.map(|entry| entry.expect("a directory entry").path()) {
            if entry.is_dir() {
                directories.push(entry);
            } else if let Ok(content) = std::fs::read_to_string(&entry) {
                lines.extend(
                    content
                        .lines()
                        .filter(|line| line.contains(r#""kind":"read-at""#))
                        .map(str::to_owned),
                );
            }
        }
    }
    lines
}

#[test]
fn a_partial_clone_that_cannot_fetch_a_file_stores_no_absent_answer_and_reads_it_once_it_can() {
    let origin = partial_origin();
    let clone = partial_clone(&origin);

    // The remote is unreachable: git cannot fetch the file its tree names. The call fails and
    // says what to do; no "absent" is stored. Git's reader stops when the fetch fails, or, without
    // lazy fetches, reads the file as missing.
    reach(&clone, None);
    let cache = clone.cache.path();
    let output = probe_output_in(clone.path(), cache, &[]);
    failed_storing_no_absent(&output, "run git fetch", cache);
    let output = probe_output_in(clone.path(), cache, &[("GIT_NO_LAZY_FETCH", "1")]);
    failed_storing_no_absent(&output, "run git fetch", cache);

    // The remote is back: the file is read, and its answer is stored.
    reach(&clone, Some(&origin));
    let (messages, ran, _) = probe_timings(&clone);
    assert_eq!(ran, ["one"]);
    assert_eq!(messages, ["read-at HEAD~1:a.txt: first"]);
    let stored = stored_read_at(clone.cache.path());
    assert!(
        stored.iter().any(|line| line.contains(r#""blob":""#)),
        "{stored:?}"
    );
}

#[test]
fn a_stored_file_whose_content_neither_the_cache_nor_git_has_is_an_error_not_absent() {
    let origin = partial_origin();
    let first = partial_clone(&origin);
    let second = partial_clone(&origin);

    // The first clone reads the file and stores its answer.
    let (messages, _, _) = probe_timings(&first);
    assert_eq!(messages, ["read-at HEAD~1:a.txt: first"]);

    // The second clone shares the cache without the stored content, and cannot fetch the file:
    // the question is asked of git again, as if no answer were stored, and fails as it would
    // without the cache, saying what to do.
    let cache = first.cache.path();
    std::fs::remove_dir_all(cache.join("git").join("blobs")).expect("stored blobs");
    std::fs::remove_dir_all(cache.join("results")).expect("stored results");
    reach(&second, None);
    for env in [&[][..], &[("GIT_NO_LAZY_FETCH", "1")][..]] {
        let output = probe_output_in(second.path(), cache, env);
        failed_storing_no_absent(&output, "run git fetch", cache);
        let said = format!("{}{}", text(&output.stdout), text(&output.stderr));
        assert!(
            !said.contains("gate cache prune"),
            "the question was asked again, not answered from the stored id: {said}"
        );
    }

    // With the remote back, git fetches the file by the stored id.
    reach(&second, Some(&origin));
    let output = probe_output_in(second.path(), cache, &[]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    assert!(
        text(&output.stdout).contains("read-at HEAD~1:a.txt: first"),
        "{}",
        text(&output.stdout)
    );
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

/// [`probe_repo`] with the built probe pinned in `gate.lock` and already in the cache, so the
/// commands that take no `--plugin` (`baseline write`, `baseline prune`) run it.
fn pinned_probe_repo() -> Repo {
    let repo = probe_repo("");
    let (_, bytes) = probe();
    let sum = sha256(bytes);
    repo.write(
        "gate.lock",
        &lock(
            "acme/rules:probe@0.1.0",
            "https://example.invalid/probe.wasm",
            &sum,
        ),
    );
    let blobs = repo.cache.path().join("blobs/sha256");
    std::fs::create_dir_all(&blobs).expect("cache directories");
    std::fs::write(blobs.join(&sum), bytes).expect("cache entry");
    repo
}

/// Sets `probe/read`'s `options` (a TOML inline table) for project `apps/one` and commits.
fn read_with(repo: &Repo, options: &str) {
    repo.write(
        "apps/one/gate.toml",
        &format!(
            "[project]\nname = \"one\"\n\n[rules]\n\"probe/read\" = {{ level = \"error\", options = {options} }}\n"
        ),
    );
    repo.commit(&format!("probe/read with {options}"));
}

/// `gate check --format json` in `repo`: its exit code and its findings.
fn check_json(repo: &Repo) -> (Option<i32>, Vec<serde_json::Value>) {
    let output = repo.gate(&["check", "--format", "json"]);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("json: {}", text(&output.stderr)));
    let findings = report["findings"].as_array().expect("findings").clone();
    (output.status.code(), findings)
}

/// The fingerprint of the one finding `gate check` reports, checked against SARIF's.
fn only_fingerprint(repo: &Repo) -> String {
    let (code, findings) = check_json(repo);
    assert_eq!(code, Some(1), "{findings:?}");
    assert_eq!(findings.len(), 1, "{findings:?}");
    let fingerprint = findings[0]["fingerprint"]
        .as_str()
        .expect("fingerprint")
        .to_owned();
    let output = repo.gate(&["check", "--format", "sarif"]);
    let sarif: serde_json::Value = serde_json::from_str(&text(&output.stdout)).expect("SARIF");
    assert_eq!(
        sarif["runs"][0]["results"][0]["partialFingerprints"]["gate/v1"],
        serde_json::json!(fingerprint)
    );
    fingerprint
}

#[test]
fn a_finding_with_a_key_keeps_its_fingerprint_when_its_message_changes() {
    let repo = pinned_probe_repo();
    read_with(
        &repo,
        r#"{ at = "a.txt", at_message = "first wording", key = "Thing.one" }"#,
    );
    let (_, findings) = check_json(&repo);
    assert_eq!(findings[0]["key"], serde_json::json!("Thing.one"));
    let keyed = only_fingerprint(&repo);
    read_with(
        &repo,
        r#"{ at = "a.txt", at_message = "second wording, naming apps/one/a.txt", key = "Thing.one" }"#,
    );
    assert_eq!(only_fingerprint(&repo), keyed);
    read_with(
        &repo,
        r#"{ at = "a.txt", at_message = "second wording", key = "Thing.two" }"#,
    );
    assert_ne!(
        only_fingerprint(&repo),
        keyed,
        "another key, another finding"
    );

    // Without a key the message is the identity: a new wording is a new finding.
    read_with(&repo, r#"{ at = "a.txt", at_message = "first wording" }"#);
    let first = only_fingerprint(&repo);
    assert_ne!(first, keyed);
    read_with(&repo, r#"{ at = "a.txt", at_message = "second wording" }"#);
    assert_ne!(only_fingerprint(&repo), first);
    // A key never gives the fingerprint of a message of the same text.
    read_with(&repo, r#"{ at = "a.txt", at_message = "Thing.one" }"#);
    assert_ne!(only_fingerprint(&repo), keyed);
}

#[test]
fn a_baseline_entry_with_a_key_accepts_its_finding_after_the_message_changes() {
    let repo = pinned_probe_repo();
    read_with(
        &repo,
        r#"{ at = "a.txt", at_message = "first wording", key = "Thing.one" }"#,
    );
    let output = repo.gate(&["baseline", "write"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let written = std::fs::read_to_string(repo.path().join("gate/baseline.json")).expect("file");
    assert!(written.contains("\"key\": \"Thing.one\""), "{written}");
    repo.commit("the baseline");
    read_with(
        &repo,
        r#"{ at = "a.txt", at_message = "second wording", key = "Thing.one" }"#,
    );
    let (code, findings) = check_json(&repo);
    assert_eq!(code, Some(0), "{findings:?}");
    assert!(findings.is_empty(), "{findings:?}");

    // Without a key, the reworded finding is new and its entry is stale.
    read_with(&repo, r#"{ at = "a.txt", at_message = "first wording" }"#);
    let output = repo.gate(&["baseline", "write"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    repo.commit("the baseline without a key");
    read_with(&repo, r#"{ at = "a.txt", at_message = "second wording" }"#);
    let (code, findings) = check_json(&repo);
    assert_eq!(code, Some(1), "{findings:?}");
    let mut messages: Vec<&str> = findings
        .iter()
        .filter_map(|finding| finding["message"].as_str())
        .collect();
    messages.sort_unstable();
    assert_eq!(
        messages,
        [
            "second wording",
            "the baseline entry for probe/read at apps/one/a.txt matches no finding",
        ]
    );
}

#[test]
fn baseline_prune_removes_only_stale_entries_and_never_adds_one() {
    let repo = pinned_probe_repo();
    read_with(&repo, r#"{ paths = ["x", "y"] }"#);
    let output = repo.gate(&["baseline", "write"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    repo.commit("the baseline");
    let (code, findings) = check_json(&repo);
    assert_eq!(code, Some(0), "{findings:?}");

    // `y` is fixed, `z` is new: check reports the new finding and the stale entry.
    read_with(&repo, r#"{ paths = ["x", "z"] }"#);
    let (code, findings) = check_json(&repo);
    assert_eq!(code, Some(1), "{findings:?}");
    assert_eq!(findings.len(), 2, "{findings:?}");

    let output = repo.gate(&["baseline", "prune"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "removed probe/read at apps/one/probe: y: none\n",
            "gate/baseline.json: re-keyed 0, removed 1, added none; 1 kept\n",
            "1 finding is not in the baseline and stays reported; prune adds none\n",
        )
    );
    let pruned = std::fs::read_to_string(repo.path().join("gate/baseline.json")).expect("file");
    let entries: serde_json::Value = serde_json::from_str(&pruned).expect("baseline");
    let messages: Vec<&str> = entries["findings"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|entry| entry["message"].as_str())
        .collect();
    assert_eq!(messages, ["x: none"]);
    repo.commit("the pruned baseline");

    // The new finding stays reported, and nothing is stale.
    let (code, findings) = check_json(&repo);
    assert_eq!(code, Some(1), "{findings:?}");
    let messages: Vec<&str> = findings
        .iter()
        .filter_map(|finding| finding["message"].as_str())
        .collect();
    assert_eq!(messages, ["z: none"]);

    // Nothing more to prune: the file stays as it is.
    let output = repo.gate(&["baseline", "prune"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "gate/baseline.json: re-keyed 0, removed 0, added none; 1 kept\n",
            "1 finding is not in the baseline and stays reported; prune adds none\n",
        )
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("gate/baseline.json")).expect("file"),
        pruned
    );

    // A baseline edited by hand is a configuration error, and nothing is written.
    let edited = pruned.replacen("\"fingerprint\": \"", "\"fingerprint\": \"0", 1);
    repo.write("gate/baseline.json", &edited);
    repo.commit("a baseline edited by hand");
    let output = repo.gate(&["baseline", "prune"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    assert!(
        text(&output.stderr).contains("the file was edited by hand"),
        "{}",
        text(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("gate/baseline.json")).expect("file"),
        edited
    );
}

#[test]
fn check_counts_the_findings_the_baseline_holds_back() {
    let repo = pinned_probe_repo();
    read_with(&repo, r#"{ paths = ["x", "y"] }"#);
    let output = repo.gate(&["baseline", "write"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    repo.commit("the baseline");

    let output = repo.gate(&["check"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stdout));
    assert!(
        text(&output.stdout).contains(
            " 0 errors, 0 warnings, 2 findings held back by gate/baseline.json; projects"
        ),
        "{}",
        text(&output.stdout)
    );

    // `y` is fixed: one finding stays held back, and its entry is stale.
    read_with(&repo, r#"{ paths = ["x"] }"#);
    let output = repo.gate(&["check", "--format", "json"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stdout));
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout)).expect("json");
    assert_eq!(report["baselined"], 1, "{report}");
    assert!(
        report["summary"].as_str().is_some_and(|summary| summary
            .contains(" 1 error, 0 warnings, 1 finding held back by gate/baseline.json; projects")),
        "{report}"
    );
}

#[test]
fn a_baseline_written_over_two_identical_findings_passes_its_own_check() {
    let repo = pinned_probe_repo();
    read_with(&repo, r#"{ paths = ["x", "x"] }"#);
    let output = repo.gate(&["baseline", "write"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    repo.commit("the baseline");

    let output = repo.gate(&["check"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stdout));
    assert!(
        text(&output.stdout)
            .contains(" 0 errors, 0 warnings, 1 finding held back by gate/baseline.json; projects"),
        "{}",
        text(&output.stdout)
    );
}

/// Takes the guard of `repo`'s `gate/baseline.json` as a first run would, before the run under
/// test starts.
fn hold_baseline_guard(repo: &Repo) -> std::fs::File {
    let file = std::fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(repo.path().join(".git/gate.baseline.guard"))
        .expect("the guard file");
    file.lock().expect("the test takes the guard first");
    file
}

/// Starts `gate` with `arguments` in `repo`; its stderr comes line by line from the receiver.
fn spawn_gate(
    repo: &Repo,
    arguments: &[&str],
) -> (std::process::Child, std::sync::mpsc::Receiver<String>) {
    use std::io::BufRead as _;

    let mut child = isolated(
        Command::new(env!("CARGO_BIN_EXE_gate"))
            .args(arguments)
            .current_dir(repo.path())
            .env("GATE_CACHE_DIR", repo.cache.path()),
    )
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .expect("gate");
    let stderr = child.stderr.take().expect("stderr");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stderr)
            .lines()
            .map_while(Result::ok)
        {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    (child, receiver)
}

/// Whether the run said that it waits for the guard before its stderr ended. Order comes from the
/// run's own output; the minute only turns a hang into a failure.
fn waited(lines: &std::sync::mpsc::Receiver<String>) -> bool {
    loop {
        match lines.recv_timeout(Duration::from_secs(60)) {
            Ok(line) if line.starts_with("gate: waiting for another gate run") => return true,
            Ok(_) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return false,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("gate neither said it waits nor ended within a minute")
            }
        }
    }
}

#[test]
fn a_baseline_prune_waits_for_a_baseline_written_meanwhile_and_prunes_that() {
    let repo = pinned_probe_repo();
    read_with(&repo, r#"{ paths = ["x", "y"] }"#);
    let output = repo.gate(&["baseline", "write"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    repo.commit("the baseline");
    let committed = std::fs::read_to_string(repo.path().join("gate/baseline.json")).expect("file");
    // `y` is fixed and `z` is new. A first run writes the baseline of that tree: `x` and `z`.
    read_with(&repo, r#"{ paths = ["x", "z"] }"#);
    let output = repo.gate(&["baseline", "write"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let written = std::fs::read_to_string(repo.path().join("gate/baseline.json")).expect("file");
    assert_ne!(written, committed);
    repo.write("gate/baseline.json", &committed);

    // The first run holds the guard; a prune starts meanwhile, from the committed `x` and `y`.
    let first = hold_baseline_guard(&repo);
    let (prune, lines) = spawn_gate(&repo, &["baseline", "prune"]);
    let prune_waited = waited(&lines);
    // The first run writes its baseline, then lets go.
    repo.write("gate/baseline.json", &written);
    drop(first);
    let output = prune.wait_with_output().expect("gate");
    let said: Vec<String> = lines.iter().collect();

    assert!(prune_waited, "prune waited: {said:?}");
    assert_eq!(output.status.code(), Some(0), "{said:?}");
    assert!(
        text(&output.stdout)
            .contains("gate/baseline.json: re-keyed 0, removed 0, added none; 2 kept"),
        "prune pruned the baseline written meanwhile, not the one it read before: {}",
        text(&output.stdout)
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("gate/baseline.json")).expect("file"),
        written,
        "the first run's entry for z is not lost"
    );
}

#[test]
fn a_baseline_prune_keeps_an_entry_written_meanwhile_from_a_tree_its_check_did_not_see() {
    let repo = pinned_probe_repo();
    read_with(&repo, r#"{ paths = ["x", "y"] }"#);
    let output = repo.gate(&["baseline", "write"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    repo.commit("the baseline");
    let committed = std::fs::read_to_string(repo.path().join("gate/baseline.json")).expect("file");
    // What a first run writes once `z` is added: `x` and `z`.
    read_with(&repo, r#"{ paths = ["x", "z"] }"#);
    let output = repo.gate(&["baseline", "write"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let written = std::fs::read_to_string(repo.path().join("gate/baseline.json")).expect("file");
    // The tree the prune checks reports `x` only; the baseline is the committed `x` and `y`.
    read_with(&repo, r#"{ paths = ["x"] }"#);
    repo.write("gate/baseline.json", &committed);

    let first = hold_baseline_guard(&repo);
    let (prune, lines) = spawn_gate(&repo, &["baseline", "prune"]);
    let prune_waited = waited(&lines);
    // Meanwhile `z` is added and the first run writes its baseline, then lets go.
    read_with(&repo, r#"{ paths = ["x", "z"] }"#);
    repo.write("gate/baseline.json", &written);
    drop(first);
    let output = prune.wait_with_output().expect("gate");
    let said: Vec<String> = lines.iter().collect();

    assert!(prune_waited, "prune waited: {said:?}");
    assert_eq!(output.status.code(), Some(0), "{said:?}");
    assert!(
        text(&output.stdout)
            .contains("gate/baseline.json: re-keyed 0, removed 0, added none; 2 kept"),
        "prune judges only the entries its check saw: {}",
        text(&output.stdout)
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("gate/baseline.json")).expect("file"),
        written,
        "the entry for z, which the prune's check never judged, is kept"
    );
}

#[test]
fn a_baseline_that_appears_untracked_while_prune_checks_stops_it() {
    let repo = pinned_probe_repo();
    read_with(&repo, r#"{ paths = ["x"] }"#);

    // No baseline when prune starts; an untracked one is written while prune checks.
    let first = hold_baseline_guard(&repo);
    let (prune, lines) = spawn_gate(&repo, &["baseline", "prune"]);
    let prune_waited = waited(&lines);
    repo.write("gate/baseline.json", "{\n  \"findings\": []\n}\n");
    drop(first);
    let output = prune.wait_with_output().expect("gate");
    let said = lines.iter().collect::<Vec<_>>().join("\n");

    assert!(prune_waited, "prune waited: {said}");
    assert_eq!(output.status.code(), Some(2), "{said}");
    assert!(
        said.contains("run git add gate/baseline.json first, then gate baseline prune"),
        "{said}"
    );
}

#[test]
fn baseline_prune_re_keys_an_entry_written_before_its_plugin_keyed_the_finding() {
    let repo = pinned_probe_repo();
    read_with(
        &repo,
        r#"{ at = "a.txt", at_message = "Thing has no owner" }"#,
    );
    let output = repo.gate(&["baseline", "write"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    repo.commit("a baseline written without keys");

    // The plugin now keys the finding: the old entry matches it no more, so check reports both.
    read_with(
        &repo,
        r#"{ at = "a.txt", at_message = "Thing has no owner", key = "Thing" }"#,
    );
    let (code, findings) = check_json(&repo);
    assert_eq!(code, Some(1), "{findings:?}");
    let mut rules: Vec<&str> = findings
        .iter()
        .filter_map(|finding| finding["rule"].as_str())
        .collect();
    rules.sort_unstable();
    assert_eq!(rules, ["gate/baseline", "probe/read"], "{findings:?}");

    // Prune rewrites the entry with the key instead of removing it.
    let output = repo.gate(&["baseline", "prune"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "re-keyed probe/read at apps/one/a.txt: Thing\n",
            "gate/baseline.json: re-keyed 1, removed 0, added none; 1 kept\n",
        )
    );
    let written = std::fs::read_to_string(repo.path().join("gate/baseline.json")).expect("file");
    assert!(written.contains("\"key\": \"Thing\""), "{written}");
    repo.commit("the re-keyed baseline");
    let (code, findings) = check_json(&repo);
    assert_eq!(code, Some(0), "{findings:?}");
    assert!(findings.is_empty(), "{findings:?}");

    // A new finding stays reported: re-keying accepted nothing new.
    read_with(
        &repo,
        r#"{ at = "a.txt", at_message = "Thing has no owner", key = "Thing", paths = ["n"] }"#,
    );
    let (code, findings) = check_json(&repo);
    assert_eq!(code, Some(1), "{findings:?}");
    let messages: Vec<&str> = findings
        .iter()
        .filter_map(|finding| finding["message"].as_str())
        .collect();
    assert_eq!(messages, ["n: none"]);
    let output = repo.gate(&["baseline", "prune"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "gate/baseline.json: re-keyed 0, removed 0, added none; 1 kept\n",
            "1 finding is not in the baseline and stays reported; prune adds none\n",
        )
    );
}

#[test]
fn baseline_prune_on_an_untracked_baseline_says_to_add_it_first() {
    let repo = pinned_probe_repo();
    read_with(&repo, r#"{ paths = ["x"] }"#);
    let output = repo.gate(&["baseline", "write"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let written = std::fs::read_to_string(repo.path().join("gate/baseline.json")).expect("file");

    let output = repo.gate(&["baseline", "prune"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    let error = text(&output.stderr);
    assert!(
        error.contains("gate/baseline.json exists but git does not track it")
            && error.contains("run git add gate/baseline.json first"),
        "{error}"
    );
    assert!(
        !text(&output.stdout).contains("removed 0"),
        "{}",
        text(&output.stdout)
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("gate/baseline.json")).expect("file"),
        written
    );
}

#[test]
fn a_finding_that_names_its_line_keeps_its_fingerprint_when_an_edit_above_moves_it() {
    let repo = pinned_probe_repo();
    repo.write(
        "apps/one/x.ts",
        "export const a = 1;\nexport class Thing {}\n",
    );
    read_with(&repo, r#"{ at = "x.ts", line_of = "class Thing" }"#);
    let (_, findings) = check_json(&repo);
    assert_eq!(
        findings[0]["message"],
        serde_json::json!("x.ts:2 class Thing")
    );
    let before = only_fingerprint(&repo);
    let output = repo.gate(&["baseline", "write"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    repo.commit("the baseline");

    // An unrelated edit above the finding moves it, and its message, two lines down.
    repo.write(
        "apps/one/x.ts",
        "// a comment above\nexport const a = 1;\n\nexport class Thing {}\n",
    );
    repo.commit("an edit above the finding");
    let (code, findings) = check_json(&repo);
    assert_eq!(code, Some(0), "the baseline still accepts it: {findings:?}");
    assert!(findings.is_empty(), "{findings:?}");

    repo.git(&["rm", "-q", "gate/baseline.json"]);
    repo.commit("no baseline");
    let (_, findings) = check_json(&repo);
    assert_eq!(
        findings[0]["message"],
        serde_json::json!("x.ts:4 class Thing")
    );
    assert_eq!(only_fingerprint(&repo), before);
}

/// Every file below `directory`.
fn files_below(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Sets the modification time of `path` to `days` days ago.
fn used_days_ago(path: &Path, days: u64) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open")
        .set_modified(SystemTime::now() - Duration::from_hours(24 * days))
        .expect("set the time");
}

#[test]
fn a_used_result_survives_older_than_and_a_pruned_one_is_computed_again() {
    let repo = probe_repo(
        r#""probe/git" = { level = "error", options = { read_at = [["HEAD~1", "a.txt"]] } }"#,
    );
    // The lock pins the probe this run loads, so its results are a pinned plugin's.
    let plugin = sha256(&probe().1);
    repo.write(
        "gate.lock",
        &lock(
            "acme/rules:probe@0.1.0",
            "https://example.invalid/probe.wasm",
            &plugin,
        ),
    );
    repo.write("apps/one/a.txt", "first\n");
    repo.commit("first");
    repo.write("apps/one/a.txt", "second\n");
    repo.commit("second");
    let (messages, ran, _) = probe_timings(&repo);
    assert_eq!(ran, ["one"]);
    assert_eq!(messages, ["read-at HEAD~1:a.txt: first"]);
    let cache = repo.cache.path();
    let results = files_below(&cache.join("results"));
    assert_eq!(results.len(), 1, "{results:?}");
    assert!(
        results[0].starts_with(cache.join("results").join(&plugin)),
        "results sit under their plugin: {results:?}"
    );
    let derived: Vec<PathBuf> = results
        .iter()
        .cloned()
        .chain(files_below(&cache.join("git")))
        .collect();
    for path in &derived {
        used_days_ago(path, 40);
    }

    // A warm run uses the stored result, and its time is now again: --older-than keeps it.
    let (again, ran, _) = probe_timings(&repo);
    assert!(ran.is_empty(), "{ran:?}");
    assert_eq!(again, messages);
    let output = repo.gate(&["cache", "prune", "--older-than", "30"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let listing = text(&output.stdout);
    assert!(!listing.contains("results/"), "{listing}");
    assert!(
        listing.contains("last used more than 30 days ago"),
        "the stored content the warm run did not read is old: {listing}"
    );

    // Every derived file pruned: the next run computes the result again, with the same findings.
    let output = repo.gate(&["cache", "prune", "--older-than", "0", "--yes"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(files_below(&cache.join("results")), Vec::<PathBuf>::new());
    assert_eq!(files_below(&cache.join("git")), Vec::<PathBuf>::new());
    let (after, ran, _) = probe_timings(&repo);
    assert_eq!(ran, ["one"]);
    assert_eq!(after, messages);
}

/// `repo` with plugin `other` declared at the root and pinned to a component this core cannot
/// load, committed.
fn with_broken_other_at_root(repo: &Repo) {
    let toml = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    let pinned = std::fs::read_to_string(repo.path().join("gate.lock")).expect("gate.lock");
    let (toml, lock) = with_unloadable_other(repo, &toml, &pinned);
    repo.write("gate.toml", &toml);
    repo.write("gate.lock", &lock);
    repo.commit("a second plugin, which this core cannot load");
}

/// `--plugin typescript=<the built typescript plugin>`.
fn typescript_plugin() -> String {
    format!("typescript={}", typescript().0.display())
}

/// `repo` with project `spare` in `apps/spare`, which alone declares plugin `typescript` (given
/// with [`typescript_plugin`]), with `extra` after its `[plugins]` section; committed.
/// The `[rules]` table of a spare project that turns on a rule of typescript, so that a check of
/// spare calls typescript.
const SPARE_RULE: &str = "\n[rules]\n\"typescript/parse\" = \"error\"\n";

fn with_spare(repo: &Repo, extra: &str) {
    repo.write(
        "apps/spare/gate.toml",
        &format!(
            "[project]\nname = \"spare\"\n\n[plugins]\ntypescript = {{ source = \"acme/rules:typescript@0.1.0\" }}\n{extra}"
        ),
    );
    repo.write("apps/spare/tsconfig.json", "{}\n");
    repo.write("apps/spare/src/index.ts", "export const spare = 1;\n");
    repo.commit("project spare, which alone declares typescript");
}

/// Stages an edit of greeter's source only: a `--staged` check then runs the repository and
/// greeter, not spare.
fn stage_greeter_edit(repo: &Repo) {
    repo.write(
        "apps/hello/src/main.rs",
        "fn main() {\n    // FIXME: greet by name\n    println!(\"hello, you\");\n}\n",
    );
    repo.git(&["add", "-A"]);
}

/// Asserts that `gate <arguments>` in `repo` is a configuration error whose message contains
/// `needle`.
fn assert_config_error(repo: &Repo, arguments: &[&str], needle: &str) {
    let output = repo.gate(arguments);
    let message = text(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "{arguments:?}: {message}{}",
        text(&output.stdout)
    );
    assert!(
        message.starts_with("gate: configuration error:") && message.contains(needle),
        "{arguments:?}: {message}"
    );
}

#[test]
fn a_generate_key_naming_no_generator_fails_a_run_that_does_not_run_its_plugin() {
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    with_spare(&repo, "\n[generate]\n\"typescript/nothing\" = {}\n");
    let plugin = typescript_plugin();

    assert_config_error(
        &repo,
        &["check", "--rule", "hello/*", "--plugin", &plugin],
        "\"typescript/nothing\": plugin typescript has no generator nothing",
    );
    stage_greeter_edit(&repo);
    assert_config_error(
        &repo,
        &["check", "--staged", "--plugin", &plugin],
        "\"typescript/nothing\": plugin typescript has no generator nothing",
    );
}

#[test]
fn a_rule_id_its_plugin_does_not_declare_fails_a_run_that_does_not_run_the_plugin() {
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    with_spare(
        &repo,
        "\n[rules]\n\"typescript/barrel-imprts\" = \"error\"\n",
    );
    let plugin = typescript_plugin();

    assert_config_error(
        &repo,
        &["check", "--rule", "hello/*", "--plugin", &plugin],
        "\"typescript/barrel-imprts\": plugin typescript has no rule barrel-imprts",
    );
}

#[test]
fn a_plugin_declared_with_two_sources_fails_a_run_that_does_not_run_it() {
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    with_broken_other_at_root(&repo);
    let lock = std::fs::read_to_string(repo.path().join("gate.lock")).expect("gate.lock");
    let checksum = sha256(b"a component of another interface");
    repo.write(
        "gate.lock",
        &format!(
            "{lock}\n[plugins.\"acme/rules:other@0.3.0\"]\nurl = \"https://example.invalid/other-3.wasm\"\nsha256 = \"{checksum}\"\n"
        ),
    );
    repo.write(
        "apps/spare/gate.toml",
        "[project]\nname = \"spare\"\n\n[plugins]\nother = { source = \"acme/rules:other@0.3.0\" }\n",
    );
    repo.write("apps/spare/notes.md", "# Spare\n");
    repo.commit("other at another version in spare");

    assert_config_error(
        &repo,
        &["check", "--rule", "hello/*"],
        "plugin other is declared with 2 sources (acme/rules:other@0.2.0, acme/rules:other@0.3.0)",
    );
}

#[test]
fn an_exception_naming_a_rule_its_plugin_does_not_declare_fails_a_run_that_does_not_run_the_plugin()
{
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    repo.write(
        "apps/spare/gate.exceptions.toml",
        "[[exception]]\nrule = \"typescript/barrel-imprts\"\ntarget = \"src/index.ts\"\nreason = \"a test of an exception's rule\"\n",
    );
    with_spare(&repo, "");
    let plugin = typescript_plugin();

    assert_config_error(
        &repo,
        &["check", "--rule", "hello/*", "--plugin", &plugin],
        "[[exception]] rule \"typescript/barrel-imprts\": plugin typescript has no rule barrel-imprts",
    );
}

#[test]
fn a_rule_key_is_checked_against_a_plugin_the_run_does_not_run() {
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    with_spare(&repo, "");
    let plugin = typescript_plugin();
    stage_greeter_edit(&repo);

    assert_config_error(
        &repo,
        &[
            "check",
            "--staged",
            "--rule",
            "typescript/barrel-imprts",
            "--plugin",
            &plugin,
        ],
        "--rule typescript/barrel-imprts names no configured rule",
    );
    let output = repo.gate(&[
        "check",
        "--staged",
        "--rule",
        "typescript/barrel-imports",
        "--plugin",
        &plugin,
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "the key names a rule typescript declares; nothing of it runs in greeter: {}",
        text(&output.stderr)
    );
}

#[test]
fn a_plugin_the_run_does_not_run_and_that_has_no_declaration_is_loaded_for_it() {
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    with_broken_other_at_root(&repo);

    let output = repo.gate(&["check", "--rule", "hello/*"]);
    let message = text(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "other never loaded, so no declaration of it is stored: the run loads it to validate, and \
         it fails: {message}"
    );
    assert!(message.contains("acme/rules:other@0.2.0"), "{message}");
}

#[test]
fn a_plugin_option_its_schema_refuses_fails_sync_and_every_check() {
    let repo = Repo::pinned("{ marker = 5 }");
    let toml = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    repo.write(
        "gate.toml",
        &format!(
            "{}\"typescript/barrel-imports\" = {{ level = \"error\" }}\n",
            toml.replace(
                "[plugins]\n",
                "[plugins]\ntypescript = { source = \"acme/rules:typescript@0.1.0\" }\n",
            )
        ),
    );
    repo.write("apps/hello/tsconfig.json", "{}\n");
    repo.write(
        "apps/hello/src/greeting.ts",
        "export const greeting = \"hello\";\n",
    );
    repo.commit("the typescript plugin, and an option hello's schema refuses");
    let plugin = typescript_plugin();

    for arguments in [
        &[
            "check",
            "--rule",
            "typescript/*",
            "--plugin",
            plugin.as_str(),
        ][..],
        &["check", "--plugin", plugin.as_str()][..],
        &["sync", "--plugin", plugin.as_str()][..],
    ] {
        assert_config_error(&repo, arguments, "hello/marker");
    }
}

#[test]
fn after_gate_sync_a_check_compiles_only_the_plugins_it_runs() {
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    with_spare(&repo, SPARE_RULE);
    let plugin = typescript_plugin();
    let output = repo.gate(&["sync", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let modules = repo.cache.path().join("modules");

    let runs: [(&[&str], usize, &str); 3] = [
        (
            &["check", "--rule", "hello/*"],
            1,
            "--rule leaves typescript out",
        ),
        (
            &["check", "--staged"],
            1,
            "the edit selects the repository and greeter, which do not declare typescript",
        ),
        (
            &["check", "--staged"],
            2,
            "the edit in spare selects spare, whose typescript rule runs",
        ),
    ];
    for (index, (arguments, compiled, why)) in runs.into_iter().enumerate() {
        match index {
            1 => stage_greeter_edit(&repo),
            2 => {
                repo.git(&["reset", "-q"]);
                repo.write("apps/spare/src/index.ts", "export const spare = 2;\n");
                repo.git(&["add", "apps/spare"]);
            }
            _ => {}
        }
        std::fs::remove_dir_all(&modules).expect("the module cache");
        let mut arguments = arguments.to_vec();
        arguments.extend(["--plugin", plugin.as_str()]);
        let output = repo.gate(&arguments);
        assert_ne!(
            output.status.code(),
            Some(2),
            "{arguments:?}: {}",
            text(&output.stderr)
        );
        let found = compiled_modules(repo.cache.path());
        assert_eq!(found.len(), compiled, "{arguments:?}: {why}: {found:?}");
    }
}

/// The probe's `writer` build: plugin `writer`, with the `write` generator and no rule.
fn writer() -> &'static (PathBuf, Vec<u8>) {
    component_with("gate-runtime-probe", "gate_runtime_probe", "writer")
}

/// A repository whose `probe/git` asks `sync-state` at `HEAD~1`, a commit in a configuration
/// format this core does not read, and at `HEAD`, which declares `writer` and generates with it,
/// in sync. `plugins` goes into the root `[plugins]` of `HEAD` and `root` after its sections.
fn sync_state_fallback_repo(plugins: &str, root: &str) -> Repo {
    let repo = probe_repo(
        r#""probe/git" = { level = "error", options = { sync_state = ["HEAD~1", "HEAD"] } }"#,
    );
    let adopted = std::fs::read_to_string(repo.path().join("gate.toml"))
        .expect("gate.toml")
        .replace(
            "[plugins]\n",
            &format!("[plugins]\nwriter = {{ source = \"acme/rules:writer@0.1.0\" }}\n{plugins}"),
        );
    let adopted = format!("{adopted}{root}");

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
        "[project]\nname = \"one\"\n\n[generate]\n\"writer/write\" = { options = { files = { \"gen/a.md\" = \"a\\n\" } } }\n",
    );
    repo.write("apps/one/gen/a.md", "a\n");
    repo.commit("moved to Gate and in sync");
    repo
}

/// The two answers `probe/git` reports in a [`sync_state_fallback_repo`].
const FALLBACK_ANSWERS: [&str; 2] = [
    "sync-state HEAD: produced gen/a.md; drift none",
    "sync-state HEAD~1: produced gen/a.md; drift none",
];

#[test]
fn a_sync_state_fallback_loads_the_generator_plugin_a_rule_filter_left_out() {
    let repo = sync_state_fallback_repo("", "");
    let plugin = format!("writer={}", writer().0.display());
    let (code, messages) = probe_check(&repo, &["--rule", "probe/*", "--plugin", &plugin]);
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages, FALLBACK_ANSWERS,
        "--rule leaves writer out of the run, and the fallback at HEAD~1 still loads it for its \
         generator: without it, nothing would be produced"
    );
}

/// The root `[plugins]` line of the git plugin with `main` and a release subject but no
/// `activation`, which `git/no-ci-skip` reads.
const GIT_WITHOUT_ACTIVATION: &str = "git = { source = \"acme/rules:git@0.1.0\", options = { main = [\"refs/heads/main\"], release-subject = '^chore\\(release\\): (\\d+\\.\\d+\\.\\d+)$' } }\n";
/// The root `[rules]` that enables `git/no-ci-skip`.
const ROOT_NO_CI_SKIP: &str = "\n[rules]\n\"git/no-ci-skip\" = { level = \"error\" }\n";
const NO_ACTIVATION: &str =
    "[plugins.git] options has no activation, which \"git/no-ci-skip\" reads";

#[test]
fn a_rule_need_follows_the_rule_filter() {
    let repo = Repo::pinned(r#"{ marker = "NOT-IN-THE-TREE" }"#);
    let toml = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    repo.write(
        "gate.toml",
        &format!(
            "{}{ROOT_NO_CI_SKIP}",
            toml.replace(
                "[plugins]\n",
                &format!("[plugins]\n{GIT_WITHOUT_ACTIVATION}")
            )
        ),
    );
    repo.commit("git/no-ci-skip without the activation it reads");
    let git = format!("git={}", git_component().0.display());

    assert_config_error(&repo, &["check", "--plugin", &git], NO_ACTIVATION);
    let output = repo.gate(&["check", "--rule", "hello/*", "--plugin", &git]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "git/no-ci-skip does not run, so what it reads is not required: {}",
        text(&output.stderr)
    );
}

#[test]
fn a_sync_state_fallback_checks_rule_needs_with_the_run_s_rule_filter() {
    let repo = sync_state_fallback_repo(GIT_WITHOUT_ACTIVATION, ROOT_NO_CI_SKIP);
    let writer = format!("writer={}", writer().0.display());
    let git = format!("git={}", git_component().0.display());
    let probe = format!("probe={}", probe().0.display());

    assert_config_error(
        &repo,
        &[
            "check", "--plugin", &probe, "--plugin", &writer, "--plugin", &git,
        ],
        NO_ACTIVATION,
    );
    let (code, messages) = probe_check(
        &repo,
        &["--rule", "probe/*", "--plugin", &writer, "--plugin", &git],
    );
    assert_eq!(code, Some(1), "{messages:?}");
    assert_eq!(
        messages, FALLBACK_ANSWERS,
        "--rule leaves git/no-ci-skip out, at the start of the run and when the answer at HEAD~1 \
         checks the run's configuration again"
    );
}

#[test]
fn an_unknown_preset_fails_a_run_that_does_not_run_its_plugin() {
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    repo.write(
        "apps/spare/gate.toml",
        "[project]\nname = \"spare\"\nextends = [\"typescript/recomended\"]\n\n[plugins]\ntypescript = { source = \"acme/rules:typescript@0.1.0\" }\n",
    );
    repo.write("apps/spare/tsconfig.json", "{}\n");
    repo.write("apps/spare/src/index.ts", "export const spare = 1;\n");
    repo.commit("spare extends a preset typescript does not declare");
    let plugin = typescript_plugin();

    for arguments in [
        &["check", "--plugin", plugin.as_str()][..],
        &["check", "--rule", "hello/*", "--plugin", plugin.as_str()][..],
    ] {
        assert_config_error(
            &repo,
            arguments,
            "plugin typescript has no preset recomended",
        );
    }
}

/// The stored declaration of the plugin built as `wasm` in `repo`'s cache: the one entry of its
/// directory, which `gate sync` wrote.
fn stored_declaration(repo: &Repo, wasm: &[u8]) -> PathBuf {
    let directory = repo.cache.path().join("declarations").join(sha256(wasm));
    let entries: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("the plugin's declarations")
        .map(|entry| entry.expect("an entry").path())
        .collect();
    assert_eq!(entries.len(), 1, "{entries:?}");
    entries.into_iter().next().expect("one entry")
}

/// `gate check <arguments>` on an emptied module cache and result cache: the modules it compiled.
/// With no result cached, every call that runs loads its plugin; a plugin with no call is loaded
/// only when the cache holds no declaration of it.
fn compiled_by(repo: &Repo, arguments: &[&str]) -> BTreeSet<String> {
    for cache in ["modules", "results"] {
        let directory = repo.cache.path().join(cache);
        if directory.exists() {
            std::fs::remove_dir_all(&directory).expect("the cache");
        }
    }
    let output = repo.gate(arguments);
    assert_ne!(
        output.status.code(),
        Some(2),
        "{arguments:?}: {}",
        text(&output.stderr)
    );
    compiled_modules(repo.cache.path())
}

#[test]
fn a_stored_declaration_that_is_corrupt_or_names_another_plugin_is_replaced_by_the_run_that_loads_it()
 {
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    with_spare(&repo, "");
    let plugin = typescript_plugin();
    let output = repo.gate(&["sync", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let entry = stored_declaration(&repo, &typescript().1);
    let good = std::fs::read_to_string(&entry).expect("the entry");
    let mut renamed: serde_json::Value = serde_json::from_str(&good).expect("JSON");
    renamed["name"] = serde_json::Value::from("other");
    let arguments = ["check", "--rule", "hello/*", "--plugin", plugin.as_str()];

    for (bad, what) in [
        ("{\"name\":".to_owned(), "an entry that does not parse"),
        (renamed.to_string(), "an entry that names another plugin"),
    ] {
        std::fs::write(&entry, bad).expect("a bad entry");
        assert_eq!(
            compiled_by(&repo, &arguments).len(),
            2,
            "{what}: typescript loads for its declaration"
        );
        assert_eq!(
            std::fs::read_to_string(&entry).expect("the entry"),
            good,
            "{what}: the run that loaded typescript wrote its entry again"
        );
        assert_eq!(
            compiled_by(&repo, &arguments).len(),
            1,
            "{what}: the next run answers from the entry and compiles hello only"
        );
    }
}

#[test]
fn list_with_a_stored_declaration_load_would_not_use_and_no_build_in_the_cache_says_gate_sync_and_fetches_nothing()
 {
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    let output = repo.gate(&["check"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    let entry = stored_declaration(&repo, &hello().1);
    let mut renamed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&entry).expect("the entry")).expect("JSON");
    renamed["name"] = serde_json::Value::from("other");
    std::fs::write(&entry, renamed.to_string()).expect("an entry that names another plugin");
    let blob = repo
        .cache
        .path()
        .join("blobs/sha256")
        .join(sha256(&hello().1));
    std::fs::remove_file(&blob).expect("the build leaves the cache");
    repo.write(
        "apps/hello/gate.toml",
        "[project]\nname = \"greeter\"\nextends = [\"hello/recommended\"]\n",
    );
    repo.commit("greeter extends a preset");

    let output = repo.gate(&["list"]);
    let said = format!("{}{}", text(&output.stdout), text(&output.stderr));
    assert_eq!(output.status.code(), Some(0), "{said}");
    assert!(
        said.contains(&format!(
            "the rules the presets turn on are not shown: plugin hello = {SOURCE} is not in the cache; run gate sync once to fetch it, or pass --plugin hello=<file.wasm>"
        )),
        "{said}"
    );
    assert!(
        !said.contains("example.invalid"),
        "nothing is fetched: {said}"
    );
    assert!(!blob.exists(), "nothing is fetched into the cache");
}

#[test]
fn a_stored_declaration_whose_rule_schema_is_not_json_loads_the_plugin() {
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    with_spare(&repo, "");
    let plugin = typescript_plugin();
    let output = repo.gate(&["sync", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let entry = stored_declaration(&repo, &typescript().1);
    let mut stored: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&entry).expect("the entry")).expect("JSON");
    stored["rules"][0]["options_schema"] = serde_json::Value::from("not JSON");
    std::fs::write(&entry, stored.to_string()).expect("the entry");

    assert_eq!(
        compiled_by(
            &repo,
            &["check", "--rule", "hello/*", "--plugin", plugin.as_str()]
        )
        .len(),
        2,
        "an entry the run cannot use is not an error: typescript loads for its declaration"
    );
}

#[test]
fn after_gate_sync_a_pinned_plugin_the_run_does_not_run_answers_from_its_stored_declaration() {
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    with_spare(&repo, SPARE_RULE);
    let plugin = typescript_plugin();
    let output = repo.gate(&["sync", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));

    let compiled = compiled_by(
        &repo,
        &[
            "check",
            "--rule",
            "typescript/*",
            "--plugin",
            plugin.as_str(),
        ],
    );
    assert_eq!(
        compiled.len(),
        1,
        "hello, the pinned download, is left out by --rule and answers from its entry: only \
         typescript, whose rule runs in spare, compiles: {compiled:?}"
    );
}

/// The compiled modules in `cache`: the files under `modules`, without wasmtime's `.stats`
/// counters and the `.cleanup.wip-*` markers it writes as it goes.
fn compiled_modules(cache: &Path) -> BTreeSet<String> {
    fn walk(directory: &Path, found: &mut BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else if let Some(name) = path.file_name().and_then(|name| name.to_str())
                && !name.contains('.')
            {
                found.insert(name.to_owned());
            }
        }
    }
    let mut found = BTreeSet::new();
    walk(&cache.join("modules"), &mut found);
    found
}

#[test]
fn after_gate_sync_no_check_compiles_a_plugin_again() {
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    let toml = std::fs::read_to_string(repo.path().join("gate.toml")).expect("gate.toml");
    repo.write(
        "gate.toml",
        &toml.replace(
            "[plugins]\n",
            "[plugins]\ntypescript = { source = \"acme/rules:typescript@0.1.0\" }\n",
        ),
    );
    repo.commit("a second plugin, with no rule configured");
    let plugin = format!("typescript={}", typescript().0.display());
    assert_eq!(compiled_modules(repo.cache.path()), BTreeSet::new());

    let output = repo.gate(&["sync", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let after_sync = compiled_modules(repo.cache.path());
    assert_eq!(
        after_sync.len(),
        2,
        "gate sync compiles every declared plugin, generators or not: {after_sync:?}"
    );

    for arguments in [
        &["check", "--rule", "hello/*", "--plugin", plugin.as_str()][..],
        &["check", "--plugin", plugin.as_str()][..],
    ] {
        let output = repo.gate(arguments);
        assert_eq!(
            output.status.code(),
            Some(1),
            "{arguments:?}: {}",
            text(&output.stderr)
        );
        assert_eq!(
            compiled_modules(repo.cache.path()),
            after_sync,
            "{arguments:?} loads cached modules only"
        );
    }
}

/// The built `docs` plugin, from an empty repository to a filled book, in each kind: `gate check`
/// reports every required entry missing, `gate sync` seeds them, `gate check` reports each seed
/// unfilled, and with the marker lines deleted both checks pass; a second `gate sync` writes
/// nothing over the filled pages.
#[test]
fn the_docs_plugin_lays_down_and_checks_the_standard_of_each_kind() {
    const MARKER: &str = "<!-- gate:seed: replace this page's text, then delete this line -->\n";
    let (path, _) = component("gate-plugin-docs", "docs");
    let plugin = format!("docs={}", path.display());
    for (kind, count) in [
        ("service", 9),
        ("frontend", 9),
        ("infrastructure", 9),
        ("library", 10),
        ("monorepo-root", 8),
    ] {
        let repo = Repo::empty();
        repo.write(
            "gate.toml",
            &format!(
                "[gate]\nversion = \"{VERSION}\"\n\n[plugins.docs]\nsource = \"acme/rules:docs@0.1.0\"\noptions = {{ kind = \"{kind}\" }}\n\n[rules]\n\"docs/layout\" = \"error\"\n\n[generate]\n\"docs/layout\" = {{}}\n"
            ),
        );
        repo.commit("configure");

        let output = repo.gate(&["check", "--plugin", &plugin]);
        assert_eq!(
            output.status.code(),
            Some(1),
            "{kind}: {}",
            text(&output.stderr)
        );
        let report = text(&output.stdout);
        assert_eq!(
            report.matches("docs/layout#missing").count(),
            count,
            "{kind}: {report}"
        );

        let output = repo.gate(&["sync", "--plugin", &plugin]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{kind}: {}",
            text(&output.stderr)
        );
        let written: Vec<String> = text(&output.stdout)
            .lines()
            .filter_map(|line| line.strip_prefix("wrote "))
            .map(str::to_owned)
            .collect();
        assert_eq!(written.len(), count, "{kind}: {written:?}");
        let output = repo.gate(&["sync", "--check", "--plugin", &plugin]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{kind}: a seed is never drift: {}",
            text(&output.stdout)
        );
        repo.commit("docs: seed the book");

        let output = repo.gate(&["check", "--plugin", &plugin]);
        assert_eq!(output.status.code(), Some(1), "{kind}");
        let report = text(&output.stdout);
        assert_eq!(
            report.matches("docs/layout#unfilled").count(),
            count,
            "{kind}: {report}"
        );
        assert_eq!(
            report.matches("docs/layout#").count(),
            count,
            "{kind}: {report}"
        );

        for page in &written {
            let full = repo.path().join(page);
            let filled = std::fs::read_to_string(&full)
                .expect("read")
                .replace(MARKER, "");
            std::fs::write(&full, filled).expect("write");
        }
        repo.commit("docs: fill the book");
        let output = repo.gate(&["check", "--plugin", &plugin]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{kind}: {}",
            text(&output.stdout)
        );
        let output = repo.gate(&["sync", "--plugin", &plugin]);
        assert_eq!(output.status.code(), Some(0), "{kind}");
        assert!(
            !text(&output.stdout).contains("wrote "),
            "{kind}: a filled page is never overwritten: {}",
            text(&output.stdout)
        );
    }
}

/// The `delegated` option of the built `docs` plugin, in each kind, with the delegated entries
/// tracked as their plugins would lay them down: after `gate sync`, `gate check` reports no
/// structural finding, only each seed unfilled; with `docs/ledger/` (a monorepo root:
/// `docs/agents/`) removed, it reports the listed entry missing.
#[test]
fn the_docs_index_seeds_and_requires_the_delegated_entries_of_each_kind() {
    let (path, _) = component("gate-plugin-docs", "docs");
    let plugin = format!("docs={}", path.display());
    for (kind, delegated, removed, count) in [
        ("service", r#"["agents", "ledger"]"#, "ledger", 9),
        ("frontend", r#"["agents", "ledger"]"#, "ledger", 9),
        ("infrastructure", r#"["agents", "ledger"]"#, "ledger", 9),
        ("library", r#"["agents", "ledger"]"#, "ledger", 10),
        ("monorepo-root", r#"["agents"]"#, "agents", 8),
    ] {
        let repo = Repo::empty();
        repo.write(
            "gate.toml",
            &format!(
                "[gate]\nversion = \"{VERSION}\"\n\n[plugins.docs]\nsource = \"acme/rules:docs@0.1.0\"\noptions = {{ kind = \"{kind}\", delegated = {delegated} }}\n\n[rules]\n\"docs/layout\" = \"error\"\n\n[generate]\n\"docs/layout\" = {{}}\n"
            ),
        );
        repo.write("docs/agents/rules/general.md", "# General\n");
        if kind != "monorepo-root" {
            repo.write("docs/ledger/manifest.json", "{}\n");
        }
        repo.commit("configure, with the delegated entries in place");

        let output = repo.gate(&["sync", "--plugin", &plugin]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{kind}: {}",
            text(&output.stderr)
        );
        repo.commit("docs: seed the book");
        let output = repo.gate(&["check", "--plugin", &plugin]);
        let report = text(&output.stdout);
        assert_eq!(output.status.code(), Some(1), "{kind}: {report}");
        assert_eq!(
            report.matches("docs/layout#unfilled").count(),
            count,
            "{kind}: {report}"
        );
        assert_eq!(
            report.matches("docs/layout#").count(),
            count,
            "{kind}: no structural finding: {report}"
        );

        repo.git(&["rm", "-r", "-q", &format!("docs/{removed}")]);
        repo.commit("remove a delegated entry");
        let output = repo.gate(&["check", "--plugin", &plugin]);
        let report = text(&output.stdout);
        assert_eq!(output.status.code(), Some(1), "{kind}: {report}");
        assert_eq!(
            report
                .matches(&format!(
                    "error[docs/layout#delegated-missing] docs/{removed}: docs/{removed}/ is listed"
                ))
                .count(),
            1,
            "{kind}: {report}"
        );
        assert_eq!(
            report.matches("docs/layout#").count(),
            count + 1,
            "{kind}: {report}"
        );
    }
}

/// A generator's stored output is keyed by the plugin's options too: with one cache, a sync after
/// `kind` changes from `service` to `monorepo-root` seeds the monorepo root's book, not the
/// service's it stored.
#[test]
fn a_generators_stored_output_is_keyed_by_the_plugins_options() {
    let (path, _) = component("gate-plugin-docs", "docs");
    let plugin = format!("docs={}", path.display());
    let config = |kind: &str| {
        format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins.docs]\nsource = \"acme/rules:docs@0.1.0\"\noptions = {{ kind = \"{kind}\" }}\n\n[generate]\n\"docs/layout\" = {{}}\n"
        )
    };
    let written = |output: &Output| -> Vec<String> {
        text(&output.stdout)
            .lines()
            .filter_map(|line| line.strip_prefix("wrote "))
            .map(str::to_owned)
            .collect()
    };
    let repo = Repo::empty();
    repo.write("gate.toml", &config("service"));
    repo.commit("a service");
    let output = repo.gate(&["sync", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(
        written(&output).contains(&"docs/modules/README.md".to_owned()),
        "{}",
        text(&output.stdout)
    );

    std::fs::remove_dir_all(repo.path().join("docs")).expect("remove the seeds");
    repo.write("gate.toml", &config("monorepo-root"));
    repo.commit("a monorepo root");
    let output = repo.gate(&["sync", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let pages = written(&output);
    assert_eq!(pages.len(), 8, "{pages:?}");
    assert!(
        !pages.contains(&"docs/modules/README.md".to_owned()),
        "{pages:?}"
    );
    let index = std::fs::read_to_string(repo.path().join("docs/README.md")).expect("read");
    assert!(index.contains("\n## Members\n"), "{index}");
}

/// A missing `kind` names where to set it and the five values it takes.
#[test]
fn a_missing_required_plugin_option_says_where_to_set_it_and_its_values() {
    let (path, _) = component("gate-plugin-docs", "docs");
    let plugin = format!("docs={}", path.display());
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins.docs]\nsource = \"acme/rules:docs@0.1.0\"\n\n[generate]\n\"docs/layout\" = {{}}\n"
        ),
    );
    repo.commit("no kind");
    for command in ["sync", "check"] {
        let output = repo.gate(&[command, "--plugin", &plugin]);
        assert_eq!(output.status.code(), Some(2), "{command}");
        let error = text(&output.stderr);
        assert!(
            error.contains(
                "plugin docs options: kind is required; set it in the [plugins.docs] options in gate.toml, one of service, frontend, infrastructure, library, monorepo-root"
            ),
            "{command}: {error}"
        );
    }
}

/// A seed is the project's file once written, so `gate sync` does not produce it: a direct commit
/// that edits a seeded page outside `documentation` is a code change for `git/main-history`, not a
/// generated file in sync.
#[test]
fn main_history_judges_an_edit_of_a_seeded_page_as_the_projects_own_change() {
    let git = component("gate-plugin-git", "git");
    let docs = component("gate-plugin-docs", "docs");
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

[plugins.docs]
source = "acme/rules:docs@0.1.0"
options = {{ kind = "service" }}

[plugins.git]
source = "acme/rules:git@0.1.0"
options = {{ activation = "{start}", main = ["refs/heads/main"], release-subject = '^chore\(release\): (\d+\.\d+\.\d+)$', branch-pattern = "^feature/[a-z0-9-]+$", branch-length = [5, 50], documentation = ["README.md"], dependabot = ["package.json"] }}

[rules]
"git/main-history" = {{ level = "error" }}

[generate]
"docs/layout" = {{}}
"#
        ),
    );
    let mut pins = lock(
        "acme/rules:docs@0.1.0",
        "https://example.invalid/docs.wasm",
        &"c".repeat(64),
    );
    let _ = write!(
        pins,
        "\n[plugins.\"acme/rules:git@0.1.0\"]\nurl = \"https://example.invalid/git.wasm\"\nsha256 = \"{}\"\n",
        "d".repeat(64)
    );
    repo.write("gate.lock", &pins);
    let configured = repo.commit("chore: configure gate");
    repo.write(
        "docs/start-here.md",
        "# Start here\n\nA person's first page.\n",
    );
    let edited = repo.commit("docs: write the first page");

    let docs_plugin = format!("docs={}", docs.0.display());
    let git_plugin = format!("git={}", git.0.display());
    let output = repo.gate(&[
        "check",
        "--plugin",
        &docs_plugin,
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
            "direct commit {} changes code path gate.lock; code reaches main through a branch merge",
            short(&configured)
        ),
        format!(
            "direct commit {} changes code path gate.toml; code reaches main through a branch merge",
            short(&configured)
        ),
        format!(
            "direct commit {} changes code path docs/start-here.md; code reaches main through a branch merge",
            short(&edited)
        ),
    ];
    expected.sort();
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    assert_eq!(messages, expected);
}

/// A workspace that lists `"."` makes the root a project of kind `monorepo-root`: its `docs/` is
/// checked against the root's standard (a `ledger/` there is unknown, with the reason), and a member
/// project checks its own kind independently.
#[test]
fn a_root_listed_as_a_project_checks_its_docs_as_a_monorepo_root_and_a_member_its_own_kind() {
    let (path, _) = component("gate-plugin-docs", "docs");
    let plugin = format!("docs={}", path.display());
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[project]\nname = \"top\"\n\n[workspace]\nprojects = [\".\", \"apps/*\"]\n\n[plugins.docs]\nsource = \"acme/rules:docs@0.1.0\"\noptions = {{ kind = \"monorepo-root\" }}\n\n[rules]\n\"docs/layout\" = \"error\"\n\n[generate]\n\"docs/layout\" = {{}}\n"
        ),
    );
    repo.write(
        "apps/a/gate.toml",
        "[project]\nname = \"a\"\n\n[plugins.docs]\nsource = \"acme/rules:docs@0.1.0\"\noptions = { kind = \"service\" }\n\n[rules]\n\"docs/layout\" = \"error\"\n",
    );
    repo.commit("a root project and a member");
    let output = repo.gate(&["sync", "--plugin", &plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    let written: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("wrote "))
        .map(|line| {
            if line.starts_with("docs/") {
                "root"
            } else {
                line
            }
        })
        .collect();
    assert_eq!(written, vec!["root"; 8], "{stdout}");
    repo.write("docs/ledger/index.md", "# Ledger\n");
    repo.commit("docs: seed the root's book, and a root ledger");

    let output = repo.gate(&["check", "--plugin", &plugin]);
    let report = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{report}{}",
        text(&output.stderr)
    );
    assert_eq!(
        report.matches("docs/layout#unfilled").count(),
        8,
        "{report}"
    );
    assert!(
        report.contains(
            "docs/ledger/ is not an entry of a monorepo root: work is recorded in the ledger of the member project whose code it changes"
        ),
        "{report}"
    );
    assert_eq!(
        report.matches("docs/layout#missing").count(),
        9,
        "the member is a service, with its modules/: {report}"
    );
    assert_eq!(report.matches("docs/layout#").count(), 18, "{report}");
}

/// `agents/no-claude-md` runs once for the whole repository: a nested project's gate.toml that sets
/// it is a configuration error that says where it goes.
#[test]
fn a_nested_project_that_sets_no_claude_md_is_a_configuration_error() {
    let (path, _) = component("gate-plugin-agents", "agents");
    let plugin = format!("agents={}", path.display());
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[workspace]\nprojects = [\"apps/*\"]\n\n[plugins.agents]\nsource = \"acme/rules:agents@0.1.0\"\n"
        ),
    );
    repo.write(
        "apps/one/gate.toml",
        "[project]\nname = \"one\"\n\n[rules]\n\"agents/no-claude-md\" = \"error\"\n",
    );
    repo.commit("a project sets a repository rule");
    let output = repo.gate(&["check", "--plugin", &plugin]);
    let error = text(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{error}");
    assert!(
        error.contains(
            "\"agents/no-claude-md\" is a repository-scope rule and runs once for the whole repository, not for project one; set it in the root gate.toml's [rules]"
        ),
        "{error}"
    );
}

/// `docs/recommended` turns `docs/layout` on at error and `docs/adoption` at warning, with no
/// `[rules]` line: on an empty project the same nine findings, failing the check only at error.
#[test]
fn the_docs_presets_enable_the_layout_rule_at_their_level() {
    let (path, _) = component("gate-plugin-docs", "docs");
    let plugin = format!("docs={}", path.display());
    for (preset, level, code) in [("recommended", "error", 1), ("adoption", "warning", 0)] {
        let repo = Repo::empty();
        repo.write(
            "gate.toml",
            &format!(
                "[gate]\nversion = \"{VERSION}\"\n\n[project]\nname = \"one\"\nextends = [\"docs/{preset}\"]\n\n[plugins.docs]\nsource = \"acme/rules:docs@0.1.0\"\noptions = {{ kind = \"service\" }}\n"
            ),
        );
        repo.commit("configure");
        let output = repo.gate(&["check", "--plugin", &plugin]);
        let report = text(&output.stdout);
        assert_eq!(
            output.status.code(),
            Some(code),
            "{preset}: {report}{}",
            text(&output.stderr)
        );
        assert_eq!(
            report
                .matches(&format!("{level}[docs/layout#missing]"))
                .count(),
            9,
            "{preset}: {report}"
        );
        assert_eq!(
            report.matches("docs/layout#").count(),
            9,
            "{preset}: {report}"
        );
    }
}

/// One `[plugins."<source>"]` entry of a `gate.lock`, for a second plugin pinned beside the first.
fn lock_entry(source: &str, url: &str) -> String {
    format!(
        "\n[plugins.\"{source}\"]\nurl = \"{url}\"\nsha256 = \"{}\"\n",
        "c".repeat(64)
    )
}

/// `gate check` with the built docker and repository plugins, as JSON: the exit code and each
/// finding as `rule#aspect path:line`, sorted.
fn hygiene_check(repo: &Repo) -> (Option<i32>, Vec<String>) {
    hygiene_check_with(repo, &[])
}

/// [`hygiene_check`] with more `arguments` (`--staged`, `--rev <commit>`).
fn hygiene_check_with(repo: &Repo, arguments: &[&str]) -> (Option<i32>, Vec<String>) {
    let docker = format!(
        "docker={}",
        component("gate-plugin-docker", "docker").0.display()
    );
    let repository = format!(
        "repository={}",
        component("gate-plugin-repository", "repository")
            .0
            .display()
    );
    let mut all = vec![
        "check",
        "--plugin",
        &docker,
        "--plugin",
        &repository,
        "--format",
        "json",
    ];
    all.extend_from_slice(arguments);
    let output = repo.gate(&all);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("json: {}", text(&output.stderr)));
    let mut findings: Vec<String> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| {
            format!(
                "{}#{} {}:{}",
                finding["rule"].as_str().unwrap_or(""),
                finding["aspect"].as_str().unwrap_or(""),
                finding["location"]["path"].as_str().unwrap_or(""),
                finding["location"]["span"]["line"].as_u64().unwrap_or(0)
            )
        })
        .collect();
    findings.sort();
    (output.status.code(), findings)
}

#[test]
fn the_docker_and_repository_rules_report_exactly_the_planted_files_and_pass_without_them() {
    let repo = Repo::empty();
    let docker = "acme/rules:docker@0.3.0";
    let repository = "acme/rules:repository@0.3.0";
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[workspace]\nprojects = [\"apps/*\"]\n\n[plugins]\ndocker = {{ source = \"{docker}\" }}\nrepository = {{ source = \"{repository}\" }}\n\n[rules]\n\"repository/tracked-secrets\" = \"error\"\n\"repository/large-files\" = \"error\"\n\n[[profiles]]\nprojects = [\"apps/*\"]\n\n[profiles.rules]\n\"docker/dockerfile\" = \"error\"\n\"docker/compose\" = \"error\"\n"
        ),
    );
    repo.write(
        "gate.lock",
        &(lock(
            docker,
            "https://example.invalid/docker.wasm",
            &"b".repeat(64),
        ) + &lock_entry(repository, "https://example.invalid/repository.wasm")),
    );
    let digest = "0123456789abcdef".repeat(4);
    repo.write("apps/api/gate.toml", "[project]\nname = \"api\"\n");
    repo.write(
        "apps/api/Dockerfile",
        &format!(
            "ARG NODE=node@sha256:{digest}\n\nFROM ${{NODE}} AS build\nWORKDIR /app\nCOPY src ./src\nRUN cp -r src out\n\nFROM ${{NODE}} AS runtime\nARG SOURCE\nARG REVISION\nARG VERSION\nLABEL org.opencontainers.image.source=$SOURCE \\\n      org.opencontainers.image.revision=$REVISION \\\n      org.opencontainers.image.version=$VERSION\nWORKDIR /app\nCOPY --from=build /app/out ./out\nUSER node\nCMD [\"node\", \"out/main.js\"]\n"
        ),
    );
    repo.write("apps/api/Dockerfile.dockerignore", "**\n!src\n");
    repo.write(
        "apps/api/docker-compose.development.yml",
        &format!(
            "services:\n  api-development-database:\n    image: postgres@sha256:{digest}\n    environment:\n      POSTGRES_PASSWORD: ${{POSTGRES_PASSWORD:?set it in the shell}}\n    ports:\n      - \"127.0.0.1::5432\"\n    healthcheck:\n      test: [\"CMD\", \"pg_isready\"]\n  api-development-server:\n    build: .\n    depends_on:\n      api-development-database:\n        condition: service_healthy\n"
        ),
    );
    repo.write("apps/api/.env.example", "DATABASE_URL=\n");
    repo.write("apps/api/src/main.js", "console.log(\"api\");\n");

    // The plants: a private key block in a fixture, and an image of 2 MiB.
    let body: String = (0..20)
        .map(|line| format!("{}\n", "QUJD".repeat(16 + line % 2)))
        .collect::<Vec<_>>()
        .concat();
    let kind = "PRIVATE";
    repo.write(
        "apps/api/test/fixture.pem",
        &format!("-----BEGIN {kind} KEY-----\n{body}-----END {kind} KEY-----\n"),
    );
    let mut image = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    image.resize(2 * 1024 * 1024, 0);
    std::fs::create_dir_all(repo.path().join("docs")).expect("directories");
    std::fs::write(repo.path().join("docs/hero.png"), image).expect("write");
    repo.commit("a service with two planted files");

    let (code, findings) = hygiene_check(&repo);
    assert_eq!(
        findings,
        [
            "repository/large-files#large docs/hero.png:0",
            "repository/tracked-secrets#private-key apps/api/test/fixture.pem:1"
        ]
    );
    assert_eq!(code, Some(1));

    repo.git(&["rm", "-q", "apps/api/test/fixture.pem", "docs/hero.png"]);
    repo.commit("the plants removed");
    let (code, findings) = hygiene_check(&repo);
    assert_eq!(findings, Vec::<String>::new());
    assert_eq!(code, Some(0));
}

#[test]
fn a_monorepo_sets_the_repository_rules_in_the_root_rules_with_large_at_warning() {
    let repo = Repo::empty();
    let docker = "acme/rules:docker@0.3.0";
    let repository = "acme/rules:repository@0.3.0";
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[workspace]\nprojects = [\"apps/*\"]\n\n[plugins]\ndocker = {{ source = \"{docker}\" }}\nrepository = {{ source = \"{repository}\" }}\n\n[rules]\n\"repository/tracked-secrets\" = \"error\"\n\"repository/large-files\" = {{ level = \"error\", levels = {{ large = \"warning\" }} }}\n"
        ),
    );
    repo.write(
        "gate.lock",
        &(lock(
            docker,
            "https://example.invalid/docker.wasm",
            &"b".repeat(64),
        ) + &lock_entry(repository, "https://example.invalid/repository.wasm")),
    );
    repo.write("apps/api/gate.toml", "[project]\nname = \"api\"\n");
    repo.write("apps/api/.env", "MODE=local\n");
    std::fs::create_dir_all(repo.path().join("docs")).expect("directories");
    std::fs::write(
        repo.path().join("docs/large.bin"),
        vec![1u8; 2 * 1024 * 1024],
    )
    .expect("write");
    repo.commit("an environment file and a large file");

    let docker_plugin = format!(
        "docker={}",
        component("gate-plugin-docker", "docker").0.display()
    );
    let repository_plugin = format!(
        "repository={}",
        component("gate-plugin-repository", "repository")
            .0
            .display()
    );
    let output = repo.gate(&[
        "check",
        "--plugin",
        &docker_plugin,
        "--plugin",
        &repository_plugin,
        "--format",
        "json",
    ]);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("json: {}", text(&output.stderr)));
    let mut findings: Vec<String> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| {
            format!(
                "{} {}#{} {}",
                finding["level"].as_str().unwrap_or(""),
                finding["rule"].as_str().unwrap_or(""),
                finding["aspect"].as_str().unwrap_or(""),
                finding["location"]["path"].as_str().unwrap_or("")
            )
        })
        .collect();
    findings.sort();
    assert_eq!(
        findings,
        [
            "error repository/tracked-secrets#env-file apps/api/.env",
            "warning repository/large-files#large docs/large.bin"
        ]
    );
    assert_eq!(output.status.code(), Some(1));
}

/// `gate check --format json` with the built docker and repository plugins: the exit code and each
/// finding as `<rule>#<aspect> <target>`, sorted.
fn hygiene_targets(repo: &Repo) -> (Option<i32>, Vec<String>) {
    let docker = format!(
        "docker={}",
        component("gate-plugin-docker", "docker").0.display()
    );
    let repository = format!(
        "repository={}",
        component("gate-plugin-repository", "repository")
            .0
            .display()
    );
    let output = repo.gate(&[
        "check",
        "--plugin",
        &docker,
        "--plugin",
        &repository,
        "--format",
        "json",
    ]);
    let report: serde_json::Value = serde_json::from_str(&text(&output.stdout))
        .unwrap_or_else(|_| panic!("json: {}", text(&output.stderr)));
    let mut findings: Vec<String> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| {
            format!(
                "{}#{} {}",
                finding["rule"].as_str().unwrap_or(""),
                finding["aspect"].as_str().unwrap_or(""),
                finding["target"].as_str().unwrap_or("")
            )
        })
        .collect();
    findings.sort();
    (output.status.code(), findings)
}

#[test]
fn an_excepted_fixture_key_does_not_cover_a_real_key_added_above_it() {
    let repo = Repo::empty();
    let docker = "acme/rules:docker@0.3.0";
    let repository = "acme/rules:repository@0.3.0";
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins]\ndocker = {{ source = \"{docker}\" }}\nrepository = {{ source = \"{repository}\" }}\n\n[rules]\n\"repository/tracked-secrets\" = \"error\"\n"
        ),
    );
    repo.write(
        "gate.lock",
        &(lock(
            docker,
            "https://example.invalid/docker.wasm",
            &"b".repeat(64),
        ) + &lock_entry(repository, "https://example.invalid/repository.wasm")),
    );
    // Synthetic key blocks, built at run time: 20 lines of 64 base64 characters each.
    let kind = "PRIVATE";
    let block = |letters: &str| {
        let body: String = (0..20)
            .map(|line| format!("{}\n", letters.repeat(16 + line % 2)))
            .collect::<Vec<_>>()
            .concat();
        format!("-----BEGIN {kind} KEY-----\n{body}-----END {kind} KEY-----\n")
    };
    let fixture = block("QUJD");
    repo.write("test/fixture.pem", &fixture);
    repo.commit("a fixture key");

    let (code, findings) = hygiene_targets(&repo);
    assert_eq!(code, Some(1), "{findings:?}");
    let [finding] = findings.as_slice() else {
        panic!("one finding: {findings:?}");
    };
    let target = finding
        .strip_prefix("repository/tracked-secrets#private-key ")
        .expect("a private-key finding");
    assert!(
        target.starts_with("test/fixture.pem:private-key:"),
        "{target}"
    );

    repo.write(
        "gate.exceptions.toml",
        &format!(
            "[[exception]]\nrule = \"repository/tracked-secrets#private-key\"\ntarget = \"{target}\"\nreason = \"a fake key the parser tests read\"\n"
        ),
    );
    repo.commit("the fixture excepted");
    let (code, findings) = hygiene_targets(&repo);
    assert_eq!(findings, Vec::<String>::new());
    assert_eq!(code, Some(0));

    repo.write("test/fixture.pem", &format!("{}\n{fixture}", block("WFla")));
    repo.commit("a real key above the fixture");
    let (code, findings) = hygiene_targets(&repo);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(
        findings[0]
            .starts_with("repository/tracked-secrets#private-key test/fixture.pem:private-key:")
            && !findings[0].ends_with(target),
        "the new key is reported under its own target: {findings:?}"
    );
    assert_eq!(code, Some(1));
}

#[test]
fn a_file_a_filter_stores_as_a_pointer_has_the_pointers_size_in_every_mode() {
    let repo = Repo::empty();
    let docker = "acme/rules:docker@0.3.0";
    let repository = "acme/rules:repository@0.3.0";
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[plugins]\ndocker = {{ source = \"{docker}\" }}\nrepository = {{ source = \"{repository}\" }}\n\n[rules]\n\"repository/large-files\" = \"error\"\n"
        ),
    );
    repo.write(
        "gate.lock",
        &(lock(
            docker,
            "https://example.invalid/docker.wasm",
            &"b".repeat(64),
        ) + &lock_entry(repository, "https://example.invalid/repository.wasm")),
    );
    // A filter as Git LFS installs one: the index holds a short pointer, the checkout the file.
    repo.write(".gitattributes", "*.bin filter=pointer\n");
    repo.git(&[
        "config",
        "filter.pointer.clean",
        "cat >/dev/null; printf 'pointer to a large file\\n'",
    ]);
    repo.git(&[
        "config",
        "filter.pointer.smudge",
        "cat >/dev/null; head -c 2097152 /dev/zero",
    ]);
    repo.git(&["config", "filter.pointer.required", "true"]);
    let large = vec![0u8; 2 * 1024 * 1024];
    std::fs::write(repo.path().join("media.bin"), &large).expect("write");
    // The twin without a filter: git stores all of it.
    std::fs::write(repo.path().join("media.dat"), &large).expect("write");
    repo.write("notes.md", "notes\n");
    repo.commit("a large file behind a filter and one without");
    std::fs::remove_file(repo.path().join("media.bin")).expect("remove");
    repo.git(&["checkout", "--", "media.bin"]);
    assert_eq!(
        std::fs::metadata(repo.path().join("media.bin"))
            .expect("checked out")
            .len(),
        2 * 1024 * 1024,
        "the checkout holds the smudged file"
    );

    let only_the_twin = ["repository/large-files#large media.dat:0"];
    let (code, findings) = hygiene_check(&repo);
    assert_eq!(findings, only_the_twin, "working tree");
    assert_eq!(code, Some(1));
    let (code, findings) = hygiene_check_with(&repo, &["--rev", "HEAD"]);
    assert_eq!(findings, only_the_twin, "commit");
    assert_eq!(code, Some(1));
    repo.write("notes.md", "notes, staged\n");
    repo.git(&["add", "notes.md"]);
    let (code, findings) = hygiene_check_with(&repo, &["--staged"]);
    assert_eq!(findings, only_the_twin, "index");
    assert_eq!(code, Some(1));
}

/// The ci plugin built from this tree.
fn ci_component() -> &'static (PathBuf, Vec<u8>) {
    component("gate-plugin-ci", "ci")
}

/// A commit for the pins of the generated workflow.
const PIN: &str = "0123456789abcdef0123456789abcdef01234567";

/// A root configuration that takes `ci/recommended` and whose `[ci.github]` uses every key.
fn ci_standard_root() -> String {
    format!(
        r#"[gate]
version = "{VERSION}"

[project]
name = "server"
extends = ["ci/recommended"]

[plugins.ci]
source = "acme/rules:ci@0.1.0"

[ci.github]
workflow = ".github/workflows/gate.yml"
name = "Gate"
branches = ["main"]
checkout = "actions/checkout@{PIN} # v4.2.2"
runs-on = "ubuntu-24.04"
aggregate = "gate-passed"
setup = [
  {{ name = "Set up Node", uses = "actions/setup-node@{PIN} # v4.4.0", with = {{ node-version-file = ".nvmrc", cache = "npm" }} }},
]

[ci.github.jobs.lint]
name = "Lint"
timeout-minutes = 20
budget = {{ warn-after = 120, fail-after = 300 }}
fetch-depth = 0
run = ["check --push"]

[ci.github.jobs.unit]
name = "Unit tests"
runs-on = "ubuntu-24.04"
timeout-minutes = 30
projects = ["server"]
working-directory = "apps"
services = {{ postgres = {{ image = "postgres:17" }} }}
steps = [{{ name = "Install", run = "npm ci" }}]
after = [
  {{ name = "Upload the reports", if = "always()", uses = "actions/upload-artifact@{PIN} # v4.6.2", with = {{ path = "reports/" }} }},
]
run = ["run unit --push"]

[ci.github.jobs.reports]
name = "Reports"
timeout-minutes = 10
needs = ["lint", "unit"]
when = "completed"
projects = ["server"]
run = ["check"]

[ci.github.jobs.publish]
name = "Publish"
timeout-minutes = 15
environment = "release"
permissions = {{ contents = "write" }}
run = ["check"]

[steps]
unit = {{ run = "true" }}
"#
    )
}

#[test]
fn the_generated_workflow_meets_the_ci_standard() {
    let ci = format!("ci={}", ci_component().0.display());
    let repo = Repo::empty();
    repo.write("gate.toml", &ci_standard_root());
    repo.write(".nvmrc", "24\n");
    repo.write("apps/index.ts", "export const one = 1;\n");
    repo.commit("configure");
    let output = repo.gate(&["sync", "--plugin", &ci]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let workflow = std::fs::read_to_string(repo.path().join(".github/workflows/gate.yml"))
        .expect("the generated workflow");
    assert!(
        workflow.contains(&format!(
            "\n      - name: Check out the repository\n        uses: actions/checkout@{PIN} # v4.2.2\n"
        )),
        "the release is a comment after the pin: {workflow}"
    );
    assert!(workflow.contains("\npermissions: {}\n"), "{workflow}");
    assert!(
        workflow.contains("\n    needs:\n      - changes\n      - lint\n      - unit\n"),
        "a job with needs: {workflow}"
    );
    repo.commit("sync");

    let output = repo.gate(&["check", "--plugin", &ci]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        text(&output.stdout),
        text(&output.stderr)
    );
    assert!(
        !text(&output.stdout).contains("[ci/"),
        "{}",
        text(&output.stdout)
    );

    // The rules ran: a workflow written by hand next to it is reported, the generated one is not.
    repo.write(
        ".github/workflows/other.yml",
        "on: push\njobs:\n  a:\n    runs-on: ubuntu-latest\n    steps:\n      - run: make\n",
    );
    repo.commit("a workflow by hand");
    let output = repo.gate(&["check", "--plugin", &ci]);
    let reported = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{reported}{}",
        text(&output.stderr)
    );
    for rule in [
        "ci/names",
        "ci/permissions",
        "ci/concurrency",
        "ci/timeout",
        "ci/runner-pinned",
    ] {
        assert!(reported.contains(&format!("[{rule}")), "{rule}: {reported}");
    }
    assert!(
        reported.contains(".github/workflows/other.yml"),
        "{reported}"
    );
    assert!(
        !reported.contains(".github/workflows/gate.yml"),
        "{reported}"
    );
}

#[test]
fn a_cached_workflow_with_an_aggregate_name_meets_the_ci_standard() {
    let ci = format!("ci={}", ci_component().0.display());
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        &ci_standard_root().replace(
            "aggregate = \"gate-passed\"\n",
            "aggregate = \"gate-passed\"\naggregate-name = \"Gate passed\"\ncache = true\n",
        ),
    );
    repo.write(".nvmrc", "24\n");
    repo.write("apps/index.ts", "export const one = 1;\n");
    repo.commit("configure");
    let output = repo.gate(&["sync", "--plugin", &ci]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let workflow = std::fs::read_to_string(repo.path().join(".github/workflows/gate.yml"))
        .expect("the generated workflow");
    assert!(
        workflow.contains("\n        name: Restore the Gate cache\n")
            && workflow.contains("\n    name: Gate passed\n"),
        "{workflow}"
    );
    repo.commit("sync");

    let output = repo.gate(&["check", "--plugin", &ci]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        text(&output.stdout),
        text(&output.stderr)
    );
    assert!(
        !text(&output.stdout).contains("[ci/"),
        "{}",
        text(&output.stdout)
    );
}

#[test]
fn a_monorepo_root_takes_the_ci_standard_through_its_rules() {
    let ci = format!("ci={}", ci_component().0.display());
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        &format!(
            r#"[gate]
version = "{VERSION}"

[workspace]
projects = ["apps/*"]

[plugins.ci]
source = "acme/rules:ci@0.1.0"

[rules]
"ci/*" = "error"

[ci.github]
workflow = ".github/workflows/gate.yml"
checkout = "actions/checkout@{PIN} # v4.2.2"
aggregate = "gate-passed"

[ci.github.jobs.lint]
name = "Lint"
timeout-minutes = 20
run = ["check --push"]
"#
        ),
    );
    repo.write("apps/server/gate.toml", "[project]\nname = \"server\"\n");
    repo.write("apps/server/index.ts", "export const one = 1;\n");
    repo.commit("configure");
    let output = repo.gate(&["sync", "--plugin", &ci]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    repo.commit("sync");

    let output = repo.gate(&["check", "--plugin", &ci]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        text(&output.stdout),
        text(&output.stderr)
    );

    // The wildcard turned the rules on: a workflow written by hand is reported.
    repo.write(
        ".github/workflows/other.yml",
        "name: Other\non: push\njobs:\n  a:\n    name: A\n    runs-on: ubuntu-24.04\n    steps:\n      - name: Make\n        run: make\n",
    );
    repo.commit("a workflow by hand");
    let output = repo.gate(&["check", "--plugin", &ci]);
    let reported = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{reported}{}",
        text(&output.stderr)
    );
    for rule in ["ci/permissions", "ci/concurrency", "ci/timeout"] {
        assert!(reported.contains(&format!("[{rule}")), "{rule}: {reported}");
    }
    assert!(
        !reported.contains(".github/workflows/gate.yml"),
        "{reported}"
    );
}

/// A monorepo whose root is not a project, with project `one` and `[workspace] extends` set to
/// `extends`, the git plugin declared and `rules` in the root `[rules]`, committed.
fn git_workspace(git: &GitOptions, extends: &str, rules: &str) {
    git.repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[workspace]\nprojects = [\"apps/*\"]\nextends = [{extends}]\n\n[plugins.git]\nsource = \"acme/rules:git@0.1.0\"\noptions = {{ main = [\"refs/heads/main\"] }}\n\n[rules]\n{rules}\n"
        ),
    );
    git.repo
        .write("apps/one/gate.toml", "[project]\nname = \"one\"\n");
    git.repo.git(&["add", "-A"]);
    git.repo
        .git(&["commit", "-q", "--allow-empty", "-m", "configure"]);
}

#[test]
fn a_monorepo_root_extends_git_recommended_and_its_own_rules_win() {
    let git = GitOptions::new();
    let outside = tempfile::tempdir().expect("temporary directory");
    let message = outside.path().join("COMMIT_EDITMSG");
    std::fs::write(&message, "ci(a): x\n").expect("message");
    let message = message.display().to_string();
    // A hook run that judges the branch, and one that judges only the message being written.
    let run = |arguments: &[&str]| {
        let mut all = vec!["check", "--plugin", git.plugin.as_str(), "--hook"];
        all.extend_from_slice(arguments);
        let output = git.repo.gate(&all);
        (
            output.status.code(),
            format!("{}{}", text(&output.stdout), text(&output.stderr)),
        )
    };
    let branch = || run(&["--rule", "git/branch-name"]);
    let written = || run(&["--commit-message", &message]);
    let at = |level: &str, aspect: &str| format!("{level}[git/commit-message#{aspect}]");

    // The preset turns on both repository-scope rules at the root, with its aspect levels.
    git_workspace(&git, "\"git/recommended\"", "");
    let (code, said) = branch();
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains("error[git/branch-name"), "{said}");
    let (code, said) = written();
    assert_eq!(code, Some(1), "{said}");
    assert!(
        !said.contains("[git/branch-name")
            && said.contains(&at("warning", "header-min-length"))
            && said.contains(&at("error", "subject-min-length")),
        "{said}"
    );
    let output = git.repo.gate(&["list", "--origin"]);
    let listed = text(&output.stdout);
    assert!(
        listed.lines().any(|line| {
            line.starts_with("  extends")
                && line.contains("git/recommended")
                && line.ends_with("gate.toml:6 [workspace] extends")
        }),
        "{listed}"
    );

    // The root's [rules] wins: a level of the whole rule applies to every aspect, the preset's too.
    git_workspace(
        &git,
        "\"git/recommended\"",
        "\"git/branch-name\" = \"warning\"\n\"git/commit-message\" = \"note\"",
    );
    let (code, said) = branch();
    assert_eq!(code, Some(0), "{said}");
    assert!(said.contains("warning[git/branch-name"), "{said}");
    let (code, said) = written();
    assert_eq!(code, Some(0), "{said}");
    assert!(
        said.contains(&at("note", "header-min-length"))
            && said.contains(&at("note", "subject-min-length")),
        "{said}"
    );
    // A level the root gives an aspect wins over both.
    git_workspace(
        &git,
        "\"git/recommended\"",
        "\"git/commit-message\" = { level = \"note\", levels = { header-min-length = \"error\" } }",
    );
    let (code, said) = written();
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains(&at("error", "header-min-length"))
            && said.contains(&at("note", "subject-min-length")),
        "{said}"
    );
}

#[test]
fn a_preset_of_both_scopes_runs_each_rule_where_its_scope_runs() {
    // probe/both turns on probe/whole (repository scope) and probe/aspects (project scope).
    let extended = |from: &str| {
        let repo = probe_repo("");
        let toml = std::fs::read_to_string(repo.path().join("gate.toml"))
            .expect("gate.toml")
            .replace(from, &format!("{from}extends = [\"probe/both\"]\n"));
        repo.write("gate.toml", &toml);
        repo.commit("extend probe/both");
        probe_check(&repo, &[])
    };
    let (code, messages) = extended("projects = [\"apps/*\"]\n");
    assert_eq!((code, messages), (Some(1), vec!["whole".to_owned()]));
    let (code, messages) = extended("projects = [\"apps/one\"]\n");
    assert_eq!(
        (code, messages),
        (
            Some(1),
            vec!["aspect loud".to_owned(), "aspect quiet".to_owned()]
        )
    );
}

#[test]
fn an_extends_entry_that_turns_on_no_rule_where_it_applies_is_a_configuration_error() {
    // At the root, a preset of project-scope rules only.
    let repo = probe_repo("");
    let toml = std::fs::read_to_string(repo.path().join("gate.toml"))
        .expect("gate.toml")
        .replace(
            "projects = [\"apps/*\"]\n",
            "projects = [\"apps/*\"]\nextends = [\"probe/quiet\"]\n",
        );
    repo.write("gate.toml", &toml);
    repo.commit("the root extends probe/quiet");
    let plugin = format!("probe={}", probe().0.display());
    let output = repo.gate(&["check", "--plugin", &plugin]);
    let said = text(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{said}");
    assert!(
        said.contains(
            "gate.toml:6 [workspace] extends: \"probe/quiet\" turns on no repository-scope rule"
        ) && said.contains("[[profiles]] entry"),
        "{said}"
    );

    // In a member project, a preset of repository-scope rules only.
    let git = GitOptions::new();
    git_workspace(&git, "", "");
    git.repo.write(
        "apps/one/gate.toml",
        "[project]\nname = \"one\"\nextends = [\"git/recommended\"]\n",
    );
    git.repo.commit("the project extends git/recommended");
    let output = git.repo.gate(&["check", "--plugin", &git.plugin]);
    let said = text(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{said}");
    assert!(
        said.contains(
            "apps/one/gate.toml:3 [project] extends: \"git/recommended\" turns on no project-scope rule"
        ) && said.contains("[workspace] extends"),
        "{said}"
    );
}

#[test]
fn at_a_root_that_is_a_project_an_entry_of_repository_rules_points_to_the_roots_project_extends() {
    // A member project's entry, then a profile's for the root project itself: neither reaches
    // the repository's rules, which a root that is a project extends in its [project].
    for (member, profile) in [
        ("extends = [\"git/recommended\"]\n", ""),
        (
            "",
            "\n[[profiles]]\nprojects = [\".\"]\nextends = [\"git/recommended\"]\n",
        ),
    ] {
        let git = GitOptions::new();
        git.repo.write(
            "gate.toml",
            &format!(
                "[gate]\nversion = \"{VERSION}\"\n\n[workspace]\nprojects = [\".\", \"apps/*\"]\n\n[plugins.git]\nsource = \"acme/rules:git@0.1.0\"\noptions = {{ main = [\"refs/heads/main\"] }}\n{profile}"
            ),
        );
        git.repo.write(
            "apps/one/gate.toml",
            &format!("[project]\nname = \"one\"\n{member}"),
        );
        git.repo.commit("extend git/recommended for projects only");
        let output = git.repo.gate(&["check", "--plugin", &git.plugin]);
        let said = text(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{said}");
        assert!(
            said.contains(
                "extends: \"git/recommended\" turns on no project-scope rule; extend it in the root gate.toml's [project] extends"
            ),
            "{said}"
        );
        let place = if member.is_empty() {
            "gate.toml:13 [[profiles]] extends"
        } else {
            "apps/one/gate.toml:3 [project] extends"
        };
        assert!(said.contains(place), "{said}");
    }
}

#[test]
fn a_commit_message_check_runs_only_the_rules_that_read_the_message() {
    let git = GitOptions::new();
    let outside = tempfile::tempdir().expect("temporary directory");
    let file = outside.path().join("COMMIT_EDITMSG");
    std::fs::write(&file, "feat(core): add the thing\n").expect("message");
    let at = file.display().to_string();
    // In a hook, branch-name refuses main; the message is in form.
    let output = git.check(
        "",
        "\"git/commit-message\" = \"error\"\n\"git/branch-name\" = { level = \"error\", options = { pattern = \"^feature/.+$\" } }",
        &["--hook", "--commit-message", &at],
    );
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "branch-name does not read the message, so it does not run: {stdout}{}",
        text(&output.stderr)
    );
    assert!(!stdout.contains("git/branch-name"), "{stdout}");
    assert!(
        stdout.contains("gate check (commit message): 1 rule, "),
        "the summary names only the message, and git/commit-message ran: {stdout}"
    );
    assert!(
        !text(&output.stderr).contains("no selected rule reads the commit message"),
        "{}",
        text(&output.stderr)
    );
}

#[test]
fn a_commit_message_check_whose_selected_rules_read_no_message_says_so() {
    let git = GitOptions::new();
    let outside = tempfile::tempdir().expect("temporary directory");
    let file = outside.path().join("COMMIT_EDITMSG");
    std::fs::write(&file, "feat(core): add the thing\n").expect("message");
    let at = file.display().to_string();
    let output = git.check(
        "",
        "\"git/commit-message\" = \"error\"\n\"git/branch-name\" = { level = \"error\", options = { pattern = \"^feature/.+$\" } }",
        &["--commit-message", &at, "--rule", "git/branch-name"],
    );
    let stdout = text(&output.stdout);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}{stderr}");
    assert!(
        stdout.contains("gate check (commit message): 0 rules, "),
        "{stdout}"
    );
    assert!(
        stderr.contains("gate: warning: no selected rule reads the commit message"),
        "{stderr}"
    );

    // Listing the changed projects runs no rule, so it warns of none.
    let output = git.check(
        "",
        "\"git/commit-message\" = \"error\"\n\"git/branch-name\" = { level = \"error\", options = { pattern = \"^feature/.+$\" } }",
        &[
            "--commit-message",
            &at,
            "--rule",
            "git/branch-name",
            "--list-changed-projects",
        ],
    );
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(
        !stderr.contains("gate: warning: no selected rule"),
        "{stderr}"
    );
}

#[test]
fn a_commit_message_check_with_a_tree_selector_is_a_usage_error() {
    let git = GitOptions::new();
    let outside = tempfile::tempdir().expect("temporary directory");
    let file = outside.path().join("COMMIT_EDITMSG");
    std::fs::write(&file, "feat(core): add the thing\n").expect("message");
    let at = file.display().to_string();
    for selector in [
        &["--staged"][..],
        &["--changed", "--base", "HEAD"][..],
        &["--push", "HEAD..HEAD"][..],
        &["--rev", "HEAD"][..],
    ] {
        let mut arguments = vec!["--commit-message", at.as_str()];
        arguments.extend_from_slice(selector);
        let output = git.check("", "\"git/commit-message\" = \"error\"", &arguments);
        let message = text(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{selector:?}: {message}");
        assert!(
            message.contains("--commit-message checks only the message")
                && message.contains("pre-commit"),
            "{selector:?}: {message}"
        );
    }
}

/// The indexes `docs/layout` seeds for a service.
const SERVICE_INDEXES: [&str; 9] = [
    "docs/README.md",
    "docs/adr/README.md",
    "docs/architecture/README.md",
    "docs/guide/README.md",
    "docs/modules/README.md",
    "docs/operations/README.md",
    "docs/reference/README.md",
    "docs/start-here.md",
    "docs/testing/README.md",
];

/// The files `ledger/layout` seeds at the plugin's defaults.
const LEDGER_SEEDS: [&str; 6] = [
    "docs/ledger/manifest.json",
    "docs/ledger/decision-index.md",
    "docs/ledger/implementation-slices.md",
    "docs/ledger/verification.md",
    "docs/ledger/follow-ups.md",
    "docs/ledger/production-blockers.md",
];

/// The agents rules that read the rule files.
const RULE_FILE_RULES: [&str; 6] = [
    "rule-enforcement",
    "rule-file-size",
    "rule-ids",
    "rule-owner",
    "rule-references",
    "rule-registry",
];

/// The `gate.toml` of an empty service with the docs, ledger and agents plugins, `docs/<preset>`,
/// every ledger and agents rule at `level`, and the three generators.
fn standard_config(preset: &str, level: &str) -> String {
    let mut rules: Vec<String> = [
        "manifest",
        "decisions",
        "records",
        "blockers",
        "exception-owners",
        "exception-decisions",
        "rule-decisions",
    ]
    .iter()
    .map(|rule| format!("\"ledger/{rule}\" = \"{level}\""))
    .collect();
    rules.push(format!(
        "\"ledger/closing-notes\" = {{ level = \"{level}\", options = {{ field = \"`TEMPORARY_DELTA` records\" }} }}"
    ));
    rules.push(format!(
        "\"ledger/reuses\" = {{ level = \"{level}\", options = {{ catalog = \"docs/reference/mechanisms.md\" }} }}"
    ));
    rules.extend(
        RULE_FILE_RULES
            .iter()
            .chain(&["instruction-references", "skills", "no-claude-md"])
            .map(|rule| format!("\"agents/{rule}\" = \"{level}\"")),
    );
    format!(
        "[gate]\nversion = \"{VERSION}\"\n\n[project]\nname = \"one\"\nextends = [\"docs/{preset}\"]\n\n\
         [plugins.docs]\nsource = \"acme/rules:docs@0.1.0\"\noptions = {{ kind = \"service\", delegated = [\"agents\", \"ledger\"] }}\n\n\
         [plugins.ledger]\nsource = \"acme/rules:ledger@0.1.0\"\n\n\
         [plugins.agents]\nsource = \"acme/rules:agents@0.1.0\"\n\n\
         [rules]\n{}\n\n\
         [generate]\n\"docs/layout\" = {{}}\n\"ledger/layout\" = {{}}\n\"agents/agents-block\" = {{}}\n",
        rules.join("\n")
    )
}

/// Runs gate with the docs, ledger and agents components: the exit code and stdout then stderr.
fn with_three_plugins(repo: &Repo, command: &[&str]) -> (Option<i32>, String) {
    let plugins: Vec<String> = [
        ("docs", "gate-plugin-docs", "docs"),
        ("ledger", "gate-plugin-ledger", "ledger"),
        ("agents", "gate-plugin-agents", "agents"),
    ]
    .iter()
    .map(|(name, package, library)| format!("{name}={}", component(package, library).0.display()))
    .collect();
    let mut arguments = command.to_vec();
    for plugin in &plugins {
        arguments.extend(["--plugin", plugin.as_str()]);
    }
    let output = repo.gate(&arguments);
    (
        output.status.code(),
        format!("{}{}", text(&output.stdout), text(&output.stderr)),
    )
}

/// Each finding of a human report as `level[rule#aspect] path`, sorted.
fn finding_heads(report: &str) -> Vec<String> {
    let mut heads: Vec<String> = report
        .lines()
        .filter(|line| line.starts_with("error[") || line.starts_with("warning["))
        .map(|line| {
            line.split_once(": ")
                .map_or(line, |(head, _)| head)
                .to_owned()
        })
        .collect();
    heads.sort();
    heads
}

/// One `gate sync` lays down a service's documentation standard with the docs, ledger and agents
/// generators: the nine indexes, the manifest and the five shared ledger files, `AGENTS.md` and the
/// agents' reading map, so no delegated area is missing.
/// Afterwards `gate check` reports only content findings, which ask the person to write something:
/// each index's `#unfilled`, the rule files' `#input`, the `#input` of the ledger files only the
/// project can write, and `ledger/records`' `#none` for the first implementation and production
/// blocker records. The test lists them, so any structural finding fails it. Under
/// `docs/recommended`, with every rule at error, the check fails; under `docs/adoption`, with every
/// rule at warning, it passes. A second sync writes nothing, and `sync --check` reports no seed.
#[test]
fn one_sync_lays_down_the_whole_standard_and_the_check_asks_only_for_content() {
    // The rule files are the one input no generator writes.
    let no_rule_file = ["error[gate/sync#agents/agents-block] docs/agents/rules"];
    for (preset, level, code) in [("recommended", "error", 1), ("adoption", "warning", 0)] {
        let repo = Repo::empty();
        repo.write("gate.toml", &standard_config(preset, level));
        repo.commit("configure an empty service");

        let (_, said) = with_three_plugins(&repo, &["sync"]);
        let mut written: Vec<&str> = said
            .lines()
            .filter_map(|line| line.strip_prefix("wrote "))
            .collect();
        written.sort_unstable();
        let mut seeds: Vec<&str> = SERVICE_INDEXES
            .iter()
            .chain(&LEDGER_SEEDS)
            .chain(&["AGENTS.md", "docs/agents/context-map.md"])
            .copied()
            .collect();
        seeds.sort_unstable();
        assert_eq!(written, seeds, "{preset}: 9 + 6 + 2 seeds: {said}");
        assert_eq!(finding_heads(&said), no_rule_file, "{preset}: {said}");
        repo.commit("lay down the standard");

        let (status, said) = with_three_plugins(&repo, &["check"]);
        assert_eq!(status, Some(code), "{preset}: {said}");
        let mut expected: Vec<String> = SERVICE_INDEXES
            .iter()
            // Each index's marker line is its third line.
            .map(|index| format!("{level}[docs/layout#unfilled] {index}:3:1"))
            .chain(
                RULE_FILE_RULES
                    .iter()
                    .map(|rule| format!("{level}[agents/{rule}#input] docs/agents/rules")),
            )
            .chain([
                format!("{level}[ledger/decisions#input] docs/adr/decision-coverage.md"),
                format!("{level}[ledger/reuses#input] docs/reference/mechanisms.md"),
                format!(
                    "{level}[ledger/records#record-shape] docs/ledger/implementation-slices.md"
                ),
                format!("{level}[ledger/records#record-shape] docs/ledger/production-blockers.md"),
            ])
            .collect();
        expected.sort();
        assert_eq!(finding_heads(&said), expected, "{preset}: {said}");
        // The agents' delegated area exists: its reading map is seeded.
        assert!(!said.contains("#delegated-missing"), "{preset}: {said}");
        // The two record-shape findings are the `#none` of each entry file: they ask for a record.
        for (path, record) in [
            ("implementation-slices", "implementation record"),
            ("production-blockers", "production blocker record"),
        ] {
            assert!(
                said.contains(&format!(
                    "docs/ledger/{path}.md: docs/ledger/{path}.md must contain at least one {record}"
                )),
                "{preset}: {said}"
            );
        }

        let (_, said) = with_three_plugins(&repo, &["sync"]);
        assert!(
            !said.contains("wrote "),
            "{preset}: the second sync writes nothing: {said}"
        );
        let (_, said) = with_three_plugins(&repo, &["sync", "--check"]);
        assert_eq!(
            finding_heads(&said),
            no_rule_file,
            "{preset}: sync --check reports no seed: {said}"
        );
    }
}

/// A tracked decision index with the block of `ledger/decision-counts` and four rows.
const TRACKED_INDEX: &str = "\
# Decisions

<!-- gate:begin decision-counts -->
- Decision IDs: `0`
<!-- gate:end decision-counts -->

| ID | Status | Title | File |
| --- | --- | --- | --- |
| `D-ONE-001` | `accepted` | One | `decisions/one.md` |
| `D-TWO-001` | `superseded by D-THREE-001` | Two | `decisions/two.md` |
| `D-THREE-001` | `accepted` | Three | `decisions/three.md` |
| `D-THREE-001` | `superseded` | Three again | `decisions/three.md` |
";

/// A tracked implementation ledger with three records.
const TRACKED_IMPLEMENTATION: &str = "\
# Implementation

- `I-ONE-001`: One
  - Status: `completed`
- `I-TWO-001`: Two
  - Status: `in progress`
- `I-THREE-001`: Three
  - Status: `planned`
";

/// A tracked roadmap with the block of `ledger/review-progress`, every count readable.
const TRACKED_ROADMAP: &str = "\
# Roadmap

## Review Progress

<!-- gate:begin review-progress -->
stale
<!-- gate:end review-progress -->

## Feature Inventory

#### F001: Alpha

- Review: `implement`

#### F002: Beta

- Review: `awaiting answer`

#### F003: Gamma

- Review: `unreviewed`

```md
#### F004: Inside a fence
```

## Decision Journal

### First choice

Counts: 2 answers · F001

### Process note

Counts: 0 answers · PROCESS

### Report shape

Counts: 1 answer · REPORT

## Accepted Execution Queue

| Order | Slice | Record |
| --- | --- | --- |
| 1 | first | `I-ONE-001` |
| 2 | second | `I-TWO-001` |
| 3 | third | `I-THREE-001` |
";

/// A tracked manifest that lists the roadmap among its canonical files.
const TRACKED_MANIFEST: &str = r#"{
  "canonicalRoot": "docs/ledger",
  "canonicalFiles": ["decision-index.md", "implementation-slices.md", "plans/roadmap.md"],
  "idFamilies": {},
  "forbiddenReferences": []
}
"#;

/// `ledger/layout` next to `ledger/decision-counts` and `ledger/review-progress` over a tracked
/// ledger: the files the other two write are tracked, so `ledger/layout` seeds none of them, and
/// `gate sync` writes each path once. It seeds only the shared files the project lacks, and
/// `sync --check` passes once that is committed.
#[test]
fn the_ledger_layout_seeds_no_tracked_file_another_ledger_generator_writes() {
    let repo = Repo::empty();
    repo.write(
        "gate.toml",
        &format!(
            "[gate]\nversion = \"{VERSION}\"\n\n[project]\nname = \"one\"\n\n\
             [plugins.ledger]\nsource = \"acme/rules:ledger@0.1.0\"\n\n\
             [generate]\n\"ledger/layout\" = {{}}\n\"ledger/decision-counts\" = {{}}\n\
             \"ledger/review-progress\" = {{ options = {{ path = \"docs/ledger/plans/roadmap.md\", \
             cross-cutting-kinds = [\"RET\", \"REPORT\"], zero-answer-kinds = [\"PROCESS\"] }} }}\n"
        ),
    );
    for (path, contents) in [
        ("docs/ledger/manifest.json", TRACKED_MANIFEST),
        ("docs/ledger/decision-index.md", TRACKED_INDEX),
        (
            "docs/ledger/implementation-slices.md",
            TRACKED_IMPLEMENTATION,
        ),
        ("docs/ledger/plans/roadmap.md", TRACKED_ROADMAP),
    ] {
        repo.write(path, contents);
    }
    repo.commit("a tracked ledger");
    let plugin = format!(
        "ledger={}",
        component("gate-plugin-ledger", "ledger").0.display()
    );

    let output = repo.gate(&["sync", "--plugin", &plugin]);
    let said = format!("{}{}", text(&output.stdout), text(&output.stderr));
    assert_eq!(output.status.code(), Some(0), "{said}");
    assert!(!said.contains("both write"), "{said}");
    let read = |path: &str| std::fs::read_to_string(repo.path().join(path)).expect("written");
    assert!(
        read("docs/ledger/decision-index.md").contains("- Decision IDs: `4`"),
        "decision-counts writes the tracked index"
    );
    assert!(
        !read("docs/ledger/plans/roadmap.md").contains("stale"),
        "review-progress writes the tracked roadmap"
    );
    assert_eq!(read("docs/ledger/manifest.json"), TRACKED_MANIFEST);
    for (path, title) in [
        ("docs/ledger/verification.md", "# Verification\n"),
        ("docs/ledger/follow-ups.md", "# Follow-ups\n"),
        (
            "docs/ledger/production-blockers.md",
            "# Production blockers\n",
        ),
    ] {
        assert_eq!(read(path), title, "{path} is seeded");
    }

    repo.commit("the seeds and the blocks");
    let output = repo.gate(&["sync", "--check", "--plugin", &plugin]);
    let said = format!("{}{}", text(&output.stdout), text(&output.stderr));
    assert_eq!(output.status.code(), Some(0), "{said}");
}

/// [`probe_repo`] with `probe/aspects` and `probe/norms` on for `apps/one`, committed: one call of
/// two rules.
fn two_probe_rules() -> Repo {
    let repo = probe_repo("\"probe/aspects\" = \"error\"\n\"probe/norms\" = \"error\"");
    repo.commit("two probe rules");
    repo
}

/// `gate check` with the built probe, as JSON with `--timings`: the exit code, the finding
/// messages sorted, the summary line, and whether each probe call came from the result cache.
fn skip_check(repo: &Repo, arguments: &[&str]) -> (Option<i32>, Vec<String>, String, Vec<bool>) {
    let plugin = format!("probe={}", probe().0.display());
    let mut all = vec![
        "check",
        "--plugin",
        &plugin,
        "--format",
        "json",
        "--timings",
    ];
    all.extend_from_slice(arguments);
    let output = repo.gate(&all);
    let stderr = text(&output.stderr);
    let report: serde_json::Value =
        serde_json::from_str(&text(&output.stdout)).unwrap_or_else(|_| panic!("json: {stderr}"));
    let mut messages: Vec<String> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter_map(|finding| finding["message"].as_str().map(str::to_owned))
        .collect();
    messages.sort();
    let cached = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("timings: call probe "))
        .map(|call| call.contains(": cached, "))
        .collect();
    (
        output.status.code(),
        messages,
        report["summary"].as_str().expect("summary").to_owned(),
        cached,
    )
}

#[test]
fn a_skip_leaves_its_rules_out_with_and_without_a_rule_filter() {
    let repo = two_probe_rules();
    let both = ["aspect loud", "aspect quiet", "norms: "].map(str::to_owned);
    let (code, messages, summary, _) = skip_check(&repo, &[]);
    assert_eq!((code, messages), (Some(1), both.to_vec()));
    assert!(
        summary.starts_with("gate check (tree): 2 rules, "),
        "{summary}"
    );

    let aspects = ["aspect loud", "aspect quiet"].map(str::to_owned).to_vec();
    for arguments in [
        &["--skip-rule", "probe/norms"][..],
        &["--rule", "probe/*", "--skip-rule", "probe/norms"][..],
        &["--skip-rule", "probe/norms", "--rule", "probe/*"][..],
    ] {
        let (code, messages, summary, _) = skip_check(&repo, arguments);
        assert_eq!((code, &messages), (Some(1), &aspects), "{arguments:?}");
        assert!(
            summary.starts_with("gate check (tree): 1 rule (skipped probe/norms), 2 errors, "),
            "{arguments:?}: {summary}"
        );
    }

    // A family skip lists each rule it left out; the plugin has no call left.
    let (code, messages, summary, cached) = skip_check(&repo, &["--skip-rule", "probe/*"]);
    assert_eq!((code, messages), (Some(0), Vec::new()));
    assert!(
        summary.starts_with("gate check (tree): 0 rules (skipped probe/aspects, probe/norms), "),
        "{summary}"
    );
    assert!(cached.is_empty(), "no probe call: {cached:?}");
}

#[test]
fn a_skip_that_leaves_nothing_out_is_a_configuration_error() {
    let repo = two_probe_rules();
    let plugin = format!("probe={}", probe().0.display());
    for (arguments, expected) in [
        (
            &["--skip-rule", "probe/whole"][..],
            "--skip-rule probe/whole matches no enabled rule: it is off in every scope, so it leaves nothing out; remove --skip-rule probe/whole, or turn the rule on",
        ),
        (
            &["--skip-rule", "probe/nothing"][..],
            "--skip-rule probe/nothing names no configured rule: no [rules] entry names it and plugin probe declares no rule nothing",
        ),
        (
            &["--rule", "probe/aspects", "--skip-rule", "probe/norms"][..],
            "--skip-rule probe/norms matches no rule --rule probe/aspects leaves to run, so it leaves nothing out; remove --skip-rule probe/norms, or name its rules with --rule",
        ),
        (
            &["--rule", "probe/aspects", "--skip-rule", "probe/*"][..],
            "--rule probe/aspects: --skip-rule leaves none of its rules to run, so the run checks nothing it names; remove --rule probe/aspects, or the --skip-rule that matches it",
        ),
    ] {
        let mut all = vec!["check", "--plugin", plugin.as_str()];
        all.extend_from_slice(arguments);
        let output = repo.gate(&all);
        let message = text(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}: {message}");
        assert_eq!(text(&output.stdout), "", "{arguments:?}: nothing ran");
        assert!(
            message.starts_with("gate: configuration error: ") && message.contains(expected),
            "{arguments:?}: {message}"
        );
    }
}

#[test]
fn a_selection_a_skip_changes_is_its_own_entry_of_the_result_cache() {
    let repo = two_probe_rules();
    let both = ["aspect loud", "aspect quiet", "norms: "]
        .map(str::to_owned)
        .to_vec();
    let aspects = ["aspect loud", "aspect quiet"].map(str::to_owned).to_vec();
    let full = || {
        let (_, messages, _, cached) = skip_check(&repo, &[]);
        (messages, cached)
    };
    let skipped = || {
        let (_, messages, _, cached) = skip_check(&repo, &["--skip-rule", "probe/norms"]);
        (messages, cached)
    };
    assert_eq!(full(), (both.clone(), vec![false]));
    assert_eq!(
        skipped(),
        (aspects.clone(), vec![false]),
        "the call of one rule is not answered by the entry of two"
    );
    assert_eq!(
        full(),
        (both, vec![true]),
        "the full run is answered by its own entry, with both rules' findings"
    );
    assert_eq!(skipped(), (aspects, vec![true]));
}

#[test]
fn a_plugin_whose_every_rule_is_skipped_is_not_loaded_and_its_pin_is_still_checked() {
    let repo = Repo::pinned(r#"{ marker = "FIXME" }"#);
    // The first run stores what hello declares; a later run that skips it needs nothing else.
    let output = repo.gate(&["check"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    let arguments = ["check", "--skip-rule", "hello/*"];
    assert_eq!(
        compiled_by(&repo, &arguments),
        BTreeSet::new(),
        "a skipped plugin is not compiled"
    );
    let output = repo.gate(&arguments);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(
        text(&output.stdout)
            .starts_with("gate check (tree): 0 rules (skipped hello/marker), 0 errors, "),
        "{}",
        text(&output.stdout)
    );

    // gate.lock pins another version of hello: the run fails on the pin, skip or not.
    let (_, bytes) = hello();
    repo.write(
        "gate.lock",
        &lock(
            "acme/rules:hello@0.0.9",
            "https://example.invalid/hello.wasm",
            &sha256(bytes),
        ),
    );
    repo.commit("pin another hello");
    for arguments in [
        &["check"][..],
        &["check", "--skip-rule", "hello/*"][..],
        &["check", "--skip-rule", "hello/marker"][..],
    ] {
        let output = repo.gate(arguments);
        let message = text(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}: {message}");
        assert!(
            message.contains("gate.lock does not match the configuration"),
            "{arguments:?}: {message}"
        );
    }
}

#[test]
fn a_pre_commit_hook_that_skips_main_history_does_not_run_it() {
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
            r#"[gate]
version = "{VERSION}"

[plugins.git]
source = "acme/rules:git@0.1.0"
options = {{ activation = "{}", main = ["refs/heads/main"], {RELEASE_SUBJECT}, branch-pattern = "^feature/[a-z0-9-]+$", branch-length = [5, 50], documentation = ["docs/**", "*.md"], dependabot = ["package.json"] }}

[rules]
"git/main-history" = {{ level = "error" }}
{NO_CI_SKIP}

[hooks]
pre-commit = ["check --staged --skip-rule git/main-history"]
"#,
            git.root
        ),
    );
    // A direct commit of code on main: git/main-history reports it in every run.
    repo.commit("chore(core): configure gate");
    let output = repo.gate(&["sync", "--plugin", &git.plugin]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let hook = std::fs::read_to_string(repo.path().join(".githooks/pre-commit")).expect("hook");
    assert!(
        hook.contains("bin/gate check --staged --skip-rule git/main-history --hook\n"),
        "{hook}"
    );

    repo.write("work.txt", "work\n");
    repo.git(&["add", "-A"]);
    let unskipped = repo.gate(&["check", "--staged", "--plugin", &git.plugin]);
    assert_eq!(
        unskipped.status.code(),
        Some(1),
        "{}",
        text(&unskipped.stderr)
    );
    assert!(
        text(&unskipped.stdout).contains("error[git/main-history#direct]"),
        "without the skip the staged run reports the direct commit: {}",
        text(&unskipped.stdout)
    );

    // `git commit` runs the generated hook, which leaves git/main-history out.
    let output = isolated(Command::new("git").arg("-C").arg(repo.path()).args([
        "commit",
        "-q",
        "-m",
        "feat(core): add the work",
    ]))
    .env("GATE_CACHE_DIR", repo.cache.path())
    .output()
    .expect("git");
    let said = format!("{}{}", text(&output.stdout), text(&output.stderr));
    assert!(output.status.success(), "the hook passes: {said}");
    assert!(
        !said.contains("[git/main-history")
            && said.contains(
                "gate check (staged): 1 rule (skipped git/main-history), 0 errors, 0 warnings"
            ),
        "{said}"
    );
}

#[test]
fn a_skip_of_a_rule_one_project_enables_by_its_file_a_profile_or_a_preset_leaves_it_out() {
    for (how, profile, two, rule) in [
        (
            "a profile",
            "\"probe/aspects\" = \"error\"",
            "[project]\nname = \"two\"\n",
            "probe/aspects",
        ),
        (
            "the project's gate.toml",
            "",
            "[project]\nname = \"two\"\n\n[rules]\n\"probe/norms\" = \"error\"\n",
            "probe/norms",
        ),
        (
            "a preset",
            "",
            "[project]\nname = \"two\"\nextends = [\"probe/lenient\"]\n",
            "probe/aspects",
        ),
    ] {
        let repo = probe_repo(profile);
        repo.write("apps/two/gate.toml", two);
        repo.commit(how);
        let (code, messages, _, _) = skip_check(&repo, &[]);
        assert_eq!(code, Some(1), "{how}: the rule runs without the skip");
        assert!(!messages.is_empty(), "{how}");

        let (code, messages, summary, _) = skip_check(&repo, &["--skip-rule", rule]);
        assert_eq!((code, messages), (Some(0), Vec::new()), "{how}");
        assert!(
            summary.starts_with(&format!("gate check (tree): 0 rules (skipped {rule}), ")),
            "{how}: {summary}"
        );
    }
}

//! The command line on synthetic repositories: expected text, formats and exit codes.

use std::fmt::Write as _;
use std::path::Path;
use std::process::{Command, Output};

/// The version of the core under test.
const VERSION: &str = env!("CARGO_PKG_VERSION");

struct Repo {
    directory: tempfile::TempDir,
    cache: tempfile::TempDir,
}

impl Repo {
    fn new() -> Self {
        let repo = Self {
            directory: tempfile::tempdir().expect("temporary directory"),
            cache: tempfile::tempdir().expect("temporary cache"),
        };
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

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    fn gate(&self, arguments: &[&str]) -> Output {
        self.gate_with(arguments, &[])
    }

    fn gate_with(&self, arguments: &[&str], environment: &[(&str, &str)]) -> Output {
        isolated(
            Command::new(env!("CARGO_BIN_EXE_gate"))
                .args(arguments)
                .current_dir(self.path())
                .env("GATE_CACHE_DIR", self.cache.path()),
        )
        .envs(environment.iter().copied())
        .output()
        .expect("gate")
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.path().join(path)).expect("read")
    }
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
        .env_remove("GATE_HEAVY_WRAPPER")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf-8")
}

/// A pnpm monorepo with no plugins: `server` and `web` depend on `shared`; `tools/gen` is a project
/// by its gate.toml alone.
fn monorepo() -> Repo {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        concat!(
            "[gate]\nversion = \"",
            env!("CARGO_PKG_VERSION"),
            "\"\n\n[workspace]\nprojects = \"pnpm\"\nignore = [\"samples/**\"]\n"
        ),
    )
    .write(
        "pnpm-workspace.yaml",
        "packages:\n  - apps/*\n  - packages/*\n",
    )
    .write("package.json", "{\"name\": \"root\", \"private\": true}\n")
    .write(
        "apps/server/package.json",
        "{\"name\": \"@acme/server\", \"dependencies\": {\"shared\": \"workspace:*\"}}\n",
    )
    .write("apps/server/gate.toml", "[project]\nname = \"server\"\n")
    .write("apps/server/src/main.ts", "export const main = 1;\n")
    .write(
        "apps/web/package.json",
        "{\"name\": \"web\", \"dependencies\": {\"shared\": \"workspace:*\"}}\n",
    )
    .write("apps/web/index.ts", "export {};\n")
    .write("packages/shared/package.json", "{\"name\": \"shared\"}\n")
    .write("packages/shared/index.ts", "export const shared = 1;\n")
    .write("tools/gen/gate.toml", "[project]\nname = \"gen\"\n")
    .write("tools/gen/run.sh", "echo gen\n")
    .write("samples/demo/package.json", "{\"name\": \"demo\"}\n")
    .write("docs/guide.md", "# Guide\n");
    repo.commit("initial");
    repo
}

/// The monorepo with plugins, a profile, presets, rules and an exception in `server`.
fn configured() -> Repo {
    let repo = monorepo();
    repo.write(
        "apps/server/gate.toml",
        r#"[project]
name = "server"
extends = ["layers/recommended"]

[plugins]
layers = { source = "ElsiKora/Gate:layers@0.1.0", options = { layout = "modules" } }

[languages.typescript]
tsconfig = "tsconfig.json"

[rules]
"layers/import-direction" = { level = "error", norm = "RULE-IMP-001", options = { roots = ["src"] } }
"layers/field-inventory" = { options-file = "gate/fields.json", levels = { hidden = "warning" }, norms = { hidden = "RULE-FLD-002" } }
"#,
    )
    .write("apps/server/gate/fields.json", "{\"hidden\": [\"password\"]}\n")
    .write(
        "apps/server/gate.exceptions.toml",
        r#"[[exception]]
rule = "layers/import-direction"
target = "src/main.ts:main"
reason = "historic import, removed with the next module split"
owner = "T-9"
"#,
    )
    .write(
        "gate.toml",
        concat!(r#"[gate]
version = ""#, env!("CARGO_PKG_VERSION"), r#""

[workspace]
projects = "pnpm"
ignore = ["samples/**"]

[plugins.git]
source = "ElsiKora/Gate:git@0.1.0"
options = { main = "main" }

[rules]
"git/commit-subject" = { level = "error", norm = "RULE-GIT-001" }

[[profiles]]
projects = ["apps/*"]

[profiles.plugins]
ledger = { source = "ElsiKora/Gate:ledger@0.1.0" }

[profiles.rules]
"ledger/*" = "off"
"#),
    )
    .write(
        "gate.lock",
        &pinned(&[
            "ElsiKora/Gate:git@0.1.0",
            "ElsiKora/Gate:layers@0.1.0",
            "ElsiKora/Gate:ledger@0.1.0",
        ]),
    );
    repo.commit("configure");
    repo
}

/// A `gate.lock` for the core under test and `sources`, with synthetic URLs and checksums.
fn pinned(sources: &[&str]) -> String {
    let mut text = String::from(concat!(
        "[core]\nversion = \"",
        env!("CARGO_PKG_VERSION"),
        "\"\n"
    ));
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-musl"] {
        let _ = write!(
            text,
            "\n[core.targets.{target}]\nurl = \"https://example.invalid/gate-{target}.tar.gz\"\nsha256 = \"{}\"\n",
            "a".repeat(64)
        );
    }
    for source in sources {
        let _ = write!(
            text,
            "\n[plugins.\"{source}\"]\nurl = \"https://example.invalid/plugin.wasm\"\nsha256 = \"{}\"\n",
            "b".repeat(64)
        );
    }
    text
}

#[test]
fn check_on_a_monorepo_with_no_plugins_prints_an_empty_report() {
    let repo = monorepo();
    let output = repo.gate(&["check"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "gate check (tree): 0 rules, 0 errors, 0 warnings; projects: ran gen, server, shared, web; skipped none\n"
    );
}

#[test]
fn check_staged_runs_only_what_the_staged_change_touches() {
    let repo = monorepo();
    repo.write("packages/shared/index.ts", "export const shared = 2;\n");
    repo.git(&["add", "packages/shared/index.ts"]);
    repo.write("tools/gen/run.sh", "echo unstaged\n");

    let output = repo.gate(&["check", "--staged"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "ran server: depends on shared\n\
         ran shared: changed packages/shared/index.ts\n\
         ran web: depends on shared\n\
         skipped gen: no change\n\
         gate check (staged): 0 rules, 0 errors, 0 warnings; projects: ran server, shared, web; skipped gen\n"
    );
}

#[test]
fn check_changed_since_a_base() {
    let repo = monorepo();
    repo.git(&["checkout", "-q", "-b", "feature"]);
    repo.write("apps/web/index.ts", "export const web = 1;\n");
    repo.commit("web");

    let output = repo.gate(&["check", "--changed", "--base", "main"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "ran web: changed apps/web/index.ts\n\
         skipped gen: no change\n\
         skipped server: no change\n\
         skipped shared: no change\n\
         gate check (changed since main): 0 rules, 0 errors, 0 warnings; projects: ran web; skipped gen, server, shared\n"
    );
}

#[test]
fn check_json_is_one_document() {
    let repo = monorepo();
    let output = repo.gate(&["check", "--format", "json", "--rev", "HEAD"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let report: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("json");
    assert_eq!(report["mode"], "rev HEAD");
    assert_eq!(report["findings"], serde_json::json!([]));
    assert_eq!(report["projects"]["ran"].as_array().map(Vec::len), Some(4));
}

#[test]
fn list_prints_projects_and_the_settings_of_each_scope() {
    let repo = configured();
    let output = repo.gate(&["list"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        concat!(
            "projects (4):\n",
            "  gen     tools/gen        (gate.toml)\n",
            "  server  apps/server      (gate.toml, pnpm)  depends on shared\n",
            "  shared  packages/shared  (pnpm)\n",
            "  web     apps/web         (pnpm)             depends on shared\n",
            "repository (gate.toml):\n",
            "  plugin  git                 ElsiKora/Gate:git@0.1.0 (options)\n",
            "  rule    git/commit-subject  level error\n",
            "  rule    git/commit-subject  norm RULE-GIT-001\n",
            "project gen (tools/gen):\n",
            "  plugin  git  ElsiKora/Gate:git@0.1.0 (options)\n",
            "project server (apps/server):\n",
            "  plugin    git                            ElsiKora/Gate:git@0.1.0 (options)\n",
            "  plugin    layers                         ElsiKora/Gate:layers@0.1.0 (options)\n",
            "  plugin    ledger                         ElsiKora/Gate:ledger@0.1.0\n",
            "  language  typescript                     {\"tsconfig\":\"tsconfig.json\"}\n",
            "  extends   layers/recommended\n",
            "  rule      ledger/*                       level off\n",
            "  rule      layers/field-inventory         options-file apps/server/gate/fields.json\n",
            "  rule      layers/field-inventory#hidden  level warning\n",
            "  rule      layers/field-inventory#hidden  norm RULE-FLD-002\n",
            "  rule      layers/import-direction        level error\n",
            "  rule      layers/import-direction        norm RULE-IMP-001\n",
            "  rule      layers/import-direction        options\n",
            "project shared (packages/shared):\n",
            "  plugin  git  ElsiKora/Gate:git@0.1.0 (options)\n",
            "project web (apps/web):\n",
            "  plugin  git       ElsiKora/Gate:git@0.1.0 (options)\n",
            "  plugin  ledger    ElsiKora/Gate:ledger@0.1.0\n",
            "  rule    ledger/*  level off\n",
        )
    );
}

#[test]
fn list_origin_prints_where_each_setting_came_from() {
    let repo = configured();
    let output = repo.gate(&["list", "--origin"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        concat!(
            "projects (4):\n",
            "  gen     tools/gen        (gate.toml)\n",
            "  server  apps/server      (gate.toml, pnpm)  depends on shared\n",
            "  shared  packages/shared  (pnpm)\n",
            "  web     apps/web         (pnpm)             depends on shared\n",
            "repository (gate.toml):\n",
            "  plugin  git                 ElsiKora/Gate:git@0.1.0 (options)  gate.toml:8 [plugins]\n",
            "  rule    git/commit-subject  level error                        gate.toml:13 [rules]\n",
            "  rule    git/commit-subject  norm RULE-GIT-001                  gate.toml:13 [rules]\n",
            "project gen (tools/gen):\n",
            "  plugin  git  ElsiKora/Gate:git@0.1.0 (options)  gate.toml:8 [plugins]\n",
            "project server (apps/server):\n",
            "  plugin    git                            ElsiKora/Gate:git@0.1.0 (options)          gate.toml:8 [plugins]\n",
            "  plugin    layers                         ElsiKora/Gate:layers@0.1.0 (options)       apps/server/gate.toml:6 [plugins]\n",
            "  plugin    ledger                         ElsiKora/Gate:ledger@0.1.0                 gate.toml:19 [[profiles]] plugins\n",
            "  language  typescript                     {\"tsconfig\":\"tsconfig.json\"}               apps/server/gate.toml:8 [languages]\n",
            "  extends   layers/recommended                                                        apps/server/gate.toml:3 [project] extends\n",
            "  rule      ledger/*                       level off                                  gate.toml:22 [[profiles]] rules\n",
            "  rule      layers/field-inventory         options-file apps/server/gate/fields.json  apps/server/gate.toml:13 [rules]\n",
            "  rule      layers/field-inventory#hidden  level warning                              apps/server/gate.toml:13 [rules]\n",
            "  rule      layers/field-inventory#hidden  norm RULE-FLD-002                          apps/server/gate.toml:13 [rules]\n",
            "  rule      layers/import-direction        level error                                apps/server/gate.toml:12 [rules]\n",
            "  rule      layers/import-direction        norm RULE-IMP-001                          apps/server/gate.toml:12 [rules]\n",
            "  rule      layers/import-direction        options                                    apps/server/gate.toml:12 [rules]\n",
            "project shared (packages/shared):\n",
            "  plugin  git  ElsiKora/Gate:git@0.1.0 (options)  gate.toml:8 [plugins]\n",
            "project web (apps/web):\n",
            "  plugin  git       ElsiKora/Gate:git@0.1.0 (options)  gate.toml:8 [plugins]\n",
            "  plugin  ledger    ElsiKora/Gate:ledger@0.1.0         gate.toml:19 [[profiles]] plugins\n",
            "  rule    ledger/*  level off                          gate.toml:22 [[profiles]] rules\n",
        )
    );
}

#[test]
fn explain_prints_what_each_scope_says_about_a_rule_and_where() {
    let repo = configured();
    let output = repo.gate(&["explain", "layers/import-direction"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        concat!(
            "layers/import-direction\n",
            "  project server (apps/server):\n",
            "    plugin  layers                   ElsiKora/Gate:layers@0.1.0 (options)  apps/server/gate.toml:6 [plugins]\n",
            "    rule    layers/import-direction  level error                           apps/server/gate.toml:12 [rules]\n",
            "    rule    layers/import-direction  norm RULE-IMP-001                     apps/server/gate.toml:12 [rules]\n",
            "    rule    layers/import-direction  options                               apps/server/gate.toml:12 [rules]\n",
            "  exceptions:\n",
            "    layers/import-direction at src/main.ts:main: historic import, removed with the next module split (owner T-9)  apps/server/gate.exceptions.toml:1\n",
        )
    );
}

#[test]
fn plugin_overrides_are_parsed_checked_and_refusable() {
    let repo = configured();
    repo.write("build/layers.wasm", "\0asm");

    let output = repo.gate(&[
        "check",
        "--plugin",
        "layers=build/layers.wasm",
        "--no-overrides",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("--plugin layers=build/layers.wasm is refused by --no-overrides"),
        "{}",
        stderr(&output)
    );

    let output = repo.gate(&["check", "--plugin", "angular=build/layers.wasm"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("no gate.toml declares plugin angular"),
        "{}",
        stderr(&output)
    );

    let output = repo.gate(&["check", "--plugin", "layers=build/missing.wasm"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("cannot read the file"),
        "{}",
        stderr(&output)
    );

    let output = repo.gate(&["check", "--plugin", "layers"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "a value without = is a usage error"
    );
}

#[test]
fn baseline_write_stores_todays_findings_and_check_reads_it() {
    let repo = monorepo();
    let output = repo.gate(&["baseline", "write"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "wrote gate/baseline.json: 0 findings\ngate reads tracked files only; run git add gate/baseline.json\n"
    );
    let written = std::fs::read_to_string(repo.path().join("gate/baseline.json")).expect("file");
    assert_eq!(written, "{\n  \"findings\": []\n}\n");

    repo.commit("baseline");
    let output = repo.gate(&["baseline", "write"]);
    assert_eq!(stdout(&output), "wrote gate/baseline.json: 0 findings\n");
    let output = repo.gate(&["check"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
}

#[test]
fn a_configuration_error_exits_two_and_names_file_and_line() {
    let repo = monorepo();
    repo.write(
        "apps/server/gate.toml",
        "[project]\nname = \"server\"\nowner = \"me\"\n",
    );
    repo.commit("break");
    let output = repo.gate(&["check"]);
    assert_eq!(output.status.code(), Some(2));
    let message = stderr(&output);
    assert!(
        message.starts_with("gate: configuration error: apps/server/gate.toml:3:1: unknown field `owner`, expected `name` or `extends`"),
        "{message}"
    );
}

#[test]
fn a_run_over_budget_warns_and_with_budget_fails_errors() {
    let repo = monorepo();
    let output = repo.gate(&["check", "--budget", "0.000001"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.starts_with("warning[gate/budget]: the run took "),
        "{text}"
    );
    assert!(text.contains("0 errors, 1 warning;"), "{text}");

    let output = repo.gate(&["check", "--budget", "0.000001", "--budget-fails"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stdout(&output).starts_with("error[gate/budget]: the run took "));
}

#[test]
fn a_core_other_than_the_pinned_one_refuses_to_run() {
    let repo = monorepo();
    repo.write(
        "gate.toml",
        "[gate]\nversion = \"9.9.9\"\n\n[workspace]\nprojects = \"pnpm\"\nignore = [\"samples/**\"]\n",
    );
    repo.commit("ask for another core");
    for arguments in [
        &["check"][..],
        &["list"],
        &["explain", "git/commit-subject"],
    ] {
        let output = repo.gate(arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(
            stderr(&output).contains(concat!(
                "this is gate ",
                env!("CARGO_PKG_VERSION"),
                ", and gate.toml asks for gate 9.9.9"
            )),
            "{arguments:?}: {}",
            stderr(&output)
        );
    }
}

const ROOT: &str = concat!(
    "[gate]\nversion = \"",
    env!("CARGO_PKG_VERSION"),
    "\"\n\n[workspace]\nprojects = \"pnpm\"\nignore = [\"samples/**\"]\n"
);

/// The monorepo with `extra` appended to the root gate.toml, committed.
fn with_root(extra: &str) -> Repo {
    let repo = monorepo();
    repo.write("gate.toml", &format!("{ROOT}{extra}"));
    repo.commit("configure");
    repo
}

/// `bin/gate`, the launcher the hooks call, running the binary under test.
fn launcher(repo: &Repo) {
    repo.write(
        "bin/gate",
        &format!(
            "#!/bin/sh\nexec '{}' \"$@\"\n",
            env!("CARGO_BIN_EXE_gate").replace('\'', "'\\''")
        ),
    );
    executable(repo, "bin/gate");
}

fn executable(repo: &Repo, name: &str) {
    let path = repo.path().join(name);
    let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&path, permissions).expect("chmod");
}

const HOOKS: &str = r#"
[hooks]
budget = 30
pre-commit = ["check --staged"]
commit-msg = ["check --commit-message"]
pre-push = ["check --push"]
"#;

const MARKER: &str =
    "# Generated by gate sync from gate.toml [hooks]; edit gate.toml, then run gate sync.";

#[test]
fn sync_writes_the_hooks_byte_stable_and_sets_the_hooks_path() {
    let repo = with_root(HOOKS);
    launcher(&repo);

    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("wrote .githooks/pre-commit\n"), "{text}");
    assert!(
        text.contains("set core.hooksPath to .githooks (unset)\n"),
        "{text}"
    );
    assert!(
        text.ends_with("gate sync: 3 generated, 3 written, 0 removed\n"),
        "{text}"
    );

    assert_eq!(
        repo.read(".githooks/pre-commit"),
        format!("#!/bin/sh\n{MARKER}\nset -e\nbin/gate check --staged --budget 30 --hook\n")
    );
    assert_eq!(
        repo.read(".githooks/commit-msg"),
        format!(
            "#!/bin/sh\n{MARKER}\nset -e\nbin/gate check --commit-message \"$1\" --budget 30 --hook\n"
        )
    );
    assert_eq!(
        repo.read(".githooks/pre-push"),
        format!(
            "#!/bin/sh\n{MARKER}\nset -e\nrefs=$(cat)\n\
             while read -r local_ref local_sha remote_ref remote_sha; do\n\
             \x20 [ -n \"$local_sha\" ] || continue\n\
             \x20 [ \"$local_sha\" != 0000000000000000000000000000000000000000 ] || continue\n\
             \x20 bin/gate check --push \"$remote_sha..$local_sha\" --target-ref \"$remote_ref\" --budget 30 --hook </dev/null\n\
             done <<EOF\n$refs\nEOF\n"
        )
    );
    let hooks_path = Command::new("git")
        .args([
            "-C",
            &repo.path().to_string_lossy(),
            "config",
            "core.hooksPath",
        ])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git");
    assert_eq!(stdout(&hooks_path), ".githooks\n");

    let before: Vec<String> = ["pre-commit", "commit-msg", "pre-push"]
        .iter()
        .map(|hook| repo.read(&format!(".githooks/{hook}")))
        .collect();
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).ends_with("gate sync: 3 generated, 0 written, 0 removed\n"),
        "{}",
        stdout(&output)
    );
    let after: Vec<String> = ["pre-commit", "commit-msg", "pre-push"]
        .iter()
        .map(|hook| repo.read(&format!(".githooks/{hook}")))
        .collect();
    assert_eq!(before, after);

    // The commit runs pre-commit and commit-msg through bin/gate; both pass on this tree.
    repo.commit("hooks");
    let output = repo.gate(&["sync", "--check"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert_eq!(
        stdout(&output),
        "gate sync --check: 3 generated files, 0 out of date\n"
    );
}

#[test]
fn sync_check_reports_a_hand_edited_a_missing_and_a_stale_hook() {
    let repo = with_root(HOOKS);
    launcher(&repo);
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    repo.commit("hooks");

    let edited = format!("{}echo skipped\n", repo.read(".githooks/pre-commit"));
    repo.write(".githooks/pre-commit", &edited);
    let output = repo.gate(&["sync", "--check"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.starts_with(
            "error[gate/sync#hooks] .githooks/pre-commit: .githooks/pre-commit differs from what gate sync writes"
        ),
        "{text}"
    );
    assert!(
        text.ends_with("gate sync --check: 3 generated files, 1 out of date\n"),
        "{text}"
    );

    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).contains("wrote .githooks/pre-commit\n"));
    let output = repo.gate(&["sync", "--check"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));

    repo.write(
        "gate.toml",
        &format!("{ROOT}\n[hooks]\npre-commit = [\"check --staged\"]\npre-push = [\"check --push\"]\npost-merge = [\"check\"]\n"),
    );
    repo.commit("hooks changed");
    let output = repo.gate(&["sync", "--check"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("error[gate/sync#hooks] .githooks/commit-msg: .githooks/commit-msg was generated by gate sync and gate.toml no longer names it"), "{text}");
    assert!(
        text.contains(
            "error[gate/sync#hooks] .githooks/post-merge: .githooks/post-merge is missing"
        ),
        "{text}"
    );
    assert!(
        text.contains("error[gate/sync#hooks] .githooks/pre-commit: .githooks/pre-commit differs"),
        "{text}"
    );
    assert!(
        text.ends_with("gate sync --check: 3 generated files, 4 out of date\n"),
        "{text}"
    );

    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("removed .githooks/commit-msg (no longer generated)\n"),
        "{}",
        stdout(&output)
    );
    assert!(!repo.path().join(".githooks/commit-msg").exists());
}

/// Two projects, `api` and `site`, by their gate.toml alone, and a workflow whose heavy job runs
/// only when one of them changed.
fn two_projects() -> Repo {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        concat!(
            r#"[gate]
version = ""#,
            env!("CARGO_PKG_VERSION"),
            r#""

[ci.github]
workflow = ".github/workflows/gate.yml"
checkout = "actions/checkout@0123456789abcdef0123456789abcdef01234567"

[ci.github.jobs.check]
name = "Check"
timeout-minutes = 10
run = ["check --push"]

[ci.github.jobs.heavy]
name = "Heavy"
projects = ["api", "site"]
timeout-minutes = 30
run = ["run test --push"]
"#
        ),
    )
    .write("apps/api/gate.toml", "[project]\nname = \"api\"\n")
    .write("apps/api/main.ts", "export const api = 1;\n")
    .write("apps/site/gate.toml", "[project]\nname = \"site\"\n")
    .write("apps/site/index.ts", "export const site = 1;\n")
    .write("docs/guide.md", "# Guide\n");
    repo.commit("initial");
    repo
}

#[test]
fn the_push_range_jobs_pass_the_remote_ref_with_the_range() {
    let repo = two_projects();
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let workflow = repo.read(".github/workflows/gate.yml");
    let changes = &workflow[workflow.find("\n  changes:\n").expect("changes job")..];
    let changes = &changes[..changes.find("\n  check:\n").unwrap_or(changes.len())];
    let check = &workflow[workflow.find("\n  check:\n").expect("check job")..];
    let check = &check[..check.find("\n  heavy:\n").unwrap_or(check.len())];
    for job in [changes, check] {
        assert!(job.contains("--target-ref \"$GATE_TARGET_REF\""), "{job}");
        assert!(
            job.contains("\n          GATE_TARGET_REF: ${{ github.base_ref && format('refs/heads/{0}', github.base_ref) || github.ref }}"),
            "the pull request's base branch, or the pushed ref: {job}"
        );
    }
}

#[test]
fn the_workflow_skips_the_heavy_job_when_only_docs_changed() {
    let repo = two_projects();
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let workflow = repo.read(".github/workflows/gate.yml");

    assert!(
        workflow.starts_with("# Generated by gate sync from gate.toml [ci.github]"),
        "{workflow}"
    );
    assert!(
        workflow.contains(
            "bin/gate check --list-changed-projects --format github-output --push \"$GATE_RANGE\" --target-ref \"$GATE_TARGET_REF\" >> \"$GITHUB_OUTPUT\""
        ),
        "{workflow}"
    );
    assert!(
        workflow.contains("projects: ${{ steps.changes.outputs.projects }}"),
        "{workflow}"
    );
    let heavy = &workflow[workflow.find("\n  heavy:\n").expect("heavy job")..];
    assert!(heavy.contains("\n    needs:\n      - changes\n"), "{heavy}");
    assert!(
        heavy.contains(
            "\n    if: ${{ !cancelled() && (needs.changes.result != 'success' || contains(fromJSON(needs.changes.outputs.projects), 'api') || contains(fromJSON(needs.changes.outputs.projects), 'site')) }}\n"
        ),
        "a heavy job runs when its projects changed, and when nobody knows what changed: {heavy}"
    );
    let changes = &workflow[workflow.find("\n  changes:\n").expect("changes job")..];
    let changes = &changes[..changes.find("\n  check:\n").unwrap_or(changes.len())];
    assert!(
        changes.contains("\n        with:\n          fetch-depth: 0\n"),
        "{changes}"
    );
    assert!(
        changes.contains(
            "\n      - name: Check the generated files\n        run: bin/gate sync --check\n"
        ),
        "a hook or workflow edited by hand fails the changes job: {changes}"
    );
    let check = &workflow[workflow.find("\n  check:\n").expect("check job")..];
    let check = &check[..check.find("\n  heavy:\n").unwrap_or(check.len())];
    assert!(!check.contains("needs:"), "{check}");
    assert!(!check.contains("if:"), "{check}");
    assert!(
        check.contains("\n        run: bin/gate check --push \"$GATE_RANGE\" --target-ref \"$GATE_TARGET_REF\" --no-overrides\n"),
        "the range and the target ref follow --push, the other flags after them: {check}"
    );
    assert!(
        check.contains("\n          fetch-depth: 0\n"),
        "a pushed range needs the history: {check}"
    );
    repo.commit("workflow");

    // The check job's run line, as the runner runs it, with the range the changes job compares.
    repo.write("docs/guide.md", "# Guide\n\nPushed.\n");
    repo.commit("pushed");
    let base = stdout(
        &isolated(Command::new("git").args(["rev-parse", "HEAD~1"]))
            .current_dir(repo.path())
            .output()
            .expect("git"),
    );
    let run = check
        .lines()
        .find_map(|line| line.trim().strip_prefix("run: "))
        .expect("the check job's run line");
    launcher(&repo);
    let output = isolated(Command::new("sh").args(["-c", run]))
        .current_dir(repo.path())
        .env("GATE_RANGE", format!("{}..HEAD", base.trim()))
        .env("GATE_TARGET_REF", "refs/heads/main")
        .output()
        .expect("sh");
    assert_eq!(output.status.code(), Some(0), "{run}: {}", stderr(&output));

    repo.write("docs/guide.md", "# Guide\n\nMore.\n");
    repo.commit("docs");
    let output = repo.gate(&[
        "check",
        "--list-changed-projects",
        "--format",
        "github-output",
        "--push",
        "HEAD~1..HEAD",
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "projects=[\".\"]\nrepository=true\n",
        "the root project owns docs/; neither api nor site is listed, so the heavy job's if: is false"
    );

    repo.write("apps/site/index.ts", "export const site = 2;\n");
    repo.commit("site");
    let output = repo.gate(&[
        "check",
        "--list-changed-projects",
        "--format",
        "github-output",
        "--push",
        "HEAD~1..HEAD",
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "projects=[\"site\"]\nrepository=true\n");
}

const STEPS: &str = r#"
[steps]
first = { run = "echo first" }
broken = { run = "exit 3" }
last = { run = "echo last" }

[suites]
all = ["first", "broken", "last"]
stop = { steps = ["first", "broken", "last"], fail-fast = true }
"#;

#[test]
fn a_suite_runs_every_step_prints_each_exit_code_and_fails() {
    let repo = with_root(STEPS);
    let output = repo.gate(&["run", "all"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let lines: Vec<String> = stdout(&output)
        .lines()
        .filter(|line| line.starts_with("repository ") || line.starts_with("gate run "))
        .map(|line| match line.rfind(" (") {
            Some(at) if line.starts_with("repository ") => line[..at].to_owned(),
            _ => line.to_owned(),
        })
        .collect();
    assert_eq!(
        lines,
        [
            "repository first: exit 0",
            "repository broken: exit 3",
            "repository last: exit 0",
            "gate run all (tree): 3 steps, 1 failed; projects: ran gen, server, shared, web; skipped none",
        ],
        "{}",
        stdout(&output)
    );
}

#[test]
fn fail_fast_stops_the_suite_at_the_first_failure() {
    let repo = with_root(STEPS);
    let output = repo.gate(&["run", "stop"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("repository broken: exit 3 ("), "{text}");
    assert!(
        text.contains("repository last: not run (fail-fast) ("),
        "{text}"
    );
    assert!(!stderr(&output).contains("gate run: repository last: echo last"));
}

#[test]
fn a_step_past_its_timeout_is_stopped() {
    let repo = with_root("\n[steps]\nslow = { run = \"sleep 30\", timeout = 1 }\n");
    let started = std::time::Instant::now();
    let output = repo.gate(&["run", "slow"]);
    assert!(started.elapsed() < std::time::Duration::from_secs(15));
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("repository slow: timed out after 1 s ("),
        "{}",
        stdout(&output)
    );
}

const WRAPPED: &str =
    "\n[steps]\nbuild = { run = \"echo built\", heavy = true }\nlight = { run = \"echo light\" }\n";

fn wrapper(repo: &Repo, name: &str) {
    repo.write(
        name,
        "#!/bin/sh\necho \"$0 $*\" >> wrapped.log\nexec \"$@\"\n",
    );
    let path = repo.path().join(name);
    let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&path, permissions).expect("chmod");
}

#[test]
fn heavy_steps_take_the_wrapper_from_configuration_before_the_environment() {
    let repo = monorepo();
    wrapper(&repo, "queue.sh");
    wrapper(&repo, "other.sh");
    repo.write(
        "gate.toml",
        &format!("{ROOT}\n[run]\nheavy-wrapper = \"./queue.sh\"\n{WRAPPED}"),
    );
    repo.commit("wrapped");

    let output = repo.gate_with(&["run", "build"], &[("GATE_HEAVY_WRAPPER", "./other.sh")]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(repo.read("wrapped.log"), "./queue.sh echo built\n");

    let output = repo.gate(&["run", "light"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(repo.read("wrapped.log"), "./queue.sh echo built\n");
}

#[test]
fn heavy_steps_take_the_wrapper_from_the_environment() {
    let repo = monorepo();
    wrapper(&repo, "other.sh");
    repo.write("gate.toml", &format!("{ROOT}{WRAPPED}"));
    repo.commit("wrapped");

    let output = repo.gate_with(&["run", "build"], &[("GATE_HEAVY_WRAPPER", "./other.sh")]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(repo.read("wrapped.log"), "./other.sh echo built\n");

    let output = repo.gate(&["run", "build"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        repo.read("wrapped.log"),
        "./other.sh echo built\n",
        "no wrapper, no queue"
    );
}

#[test]
fn a_staged_files_step_receives_exactly_the_staged_matching_paths() {
    let repo = monorepo();
    repo.write(
        "gate.toml",
        &format!(
            "{ROOT}\n[steps]\nlint = {{ run = \"sh record.sh\", files = \"staged\", glob = \"**/*.ts\" }}\n"
        ),
    )
    .write("record.sh", "printf '%s\\n' \"$@\" > received.txt\n")
    .write("packages/shared/gone.ts", "export {};\n");
    repo.commit("lint step");

    repo.write("apps/server/src/main.ts", "export const main = 2;\n")
        .write("packages/shared/a b.ts", "export const ab = 1;\n")
        .write("docs/guide.md", "# Guide, staged\n");
    repo.git(&[
        "add",
        "apps/server/src/main.ts",
        "packages/shared/a b.ts",
        "docs/guide.md",
    ]);
    repo.git(&["rm", "-q", "packages/shared/gone.ts"]);
    repo.write("apps/web/index.ts", "export const unstaged = 1;\n");

    let output = repo.gate(&["run", "lint"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        repo.read("received.txt"),
        "apps/server/src/main.ts\npackages/shared/a b.ts\n"
    );
    assert!(
        stderr(&output).contains("gate run: repository lint: sh record.sh apps/server/src/main.ts 'packages/shared/a b.ts'"),
        "{}",
        stderr(&output)
    );

    repo.git(&["reset", "-q"]);
    repo.git(&["checkout", "-q", "--", "packages/shared/gone.ts"]);
    let output = repo.gate(&["run", "lint"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("repository lint: skipped: no staged files matching **/*.ts ("),
        "{}",
        stdout(&output)
    );
}

#[test]
fn rule_filters_that_name_no_configured_rule_are_configuration_errors() {
    let repo = monorepo();
    let output = repo.gate(&["check", "--rule", "layers/import-direction"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stderr(&output),
        "gate: configuration error: --rule layers/import-direction names no configured rule: no gate.toml declares plugin layers; gate list prints the configured rules\n"
    );

    let output = repo.gate(&["check", "--rule", "gate/sync"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("the core's own rules always run"),
        "{}",
        stderr(&output)
    );

    let output = repo.gate(&["check", "--rule", "layers/import-direction#hidden"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("without #<aspect>"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_core_rule_norm_is_printed_with_its_finding() {
    let repo = with_root(&format!(
        "{HOOKS}\n[rules]\n\"gate/sync\" = {{ norm = \"DEC-1\", norms = {{ hooks = \"DEC-2\" }} }}\n"
    ));
    launcher(&repo);
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    repo.commit("hooks");

    let edited = format!("{}echo skipped\n", repo.read(".githooks/pre-commit"));
    repo.write(".githooks/pre-commit", &edited);
    let output = repo.gate(&["sync", "--check"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.starts_with("error[gate/sync#hooks] .githooks/pre-commit: "),
        "{text}"
    );
    assert!(text.contains("\n  norm: DEC-2\n"), "{text}");
}

#[test]
fn every_generated_checkout_keeps_no_credentials() {
    let repo = with_root(&format!(
        "\n[ci.github]\nworkflow = \".github/workflows/gate.yml\"\ncheckout = \"actions/checkout@{ACTION}\"\n\n[ci.github.jobs.lint]\nname = \"Lint\"\ntimeout-minutes = 10\nrun = [\"check\"]\n\n[ci.github.jobs.shallow]\nname = \"Shallow\"\ntimeout-minutes = 10\nfetch-depth = 5\nrun = [\"check\"]\n"
    ));
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let workflow = repo.read(".github/workflows/gate.yml");

    // Each checkout, with or without a fetch depth, says persist-credentials: false.
    let checkouts = workflow
        .matches(&format!(
            "      - name: Check out the repository\n        uses: actions/checkout@{ACTION}\n"
        ))
        .count();
    assert_eq!(checkouts, 3, "changes, lint and shallow: {workflow}");
    assert_eq!(
        workflow
            .matches("\n          persist-credentials: false\n")
            .count(),
        checkouts,
        "{workflow}"
    );
    assert!(
        job(&workflow, "shallow").contains(&format!(
            "      - name: Check out the repository\n        uses: actions/checkout@{ACTION}\n        with:\n          fetch-depth: 5\n          persist-credentials: false\n"
        )),
        "{}",
        job(&workflow, "shallow")
    );
    repo.commit("workflow");

    // A workflow an earlier core wrote, whose checkouts kept the credentials, is drift.
    let earlier = workflow
        .replace("        with:\n          persist-credentials: false\n", "")
        .replace("          persist-credentials: false\n", "");
    assert_ne!(earlier, workflow);
    repo.write(".github/workflows/gate.yml", &earlier);
    repo.commit("a workflow from an earlier core");
    let output = repo.gate(&["sync", "--check"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("[gate/sync#ci] .github/workflows/gate.yml"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn the_workflow_cancels_superseded_pull_request_runs_and_never_main() {
    let repo = with_root(&format!(
        "\n[ci.github]\nworkflow = \".github/workflows/gate.yml\"\ncheckout = \"actions/checkout@{ACTION}\"\n\n[ci.github.jobs.lint]\nname = \"Lint\"\ntimeout-minutes = 10\nrun = [\"check --push\"]\n"
    ));
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let workflow = repo.read(".github/workflows/gate.yml");

    // It runs on pushes and pull requests, and its runs share one group per workflow file and ref;
    // a newer run cancels the one in progress for a pull request only.
    assert!(
        workflow.contains("\non:\n  push:\n    branches:\n      - main\n  pull_request:\n    branches:\n      - main\n"),
        "{workflow}"
    );
    let concurrency = concat!(
        "\nconcurrency:\n",
        "  group: .github/workflows/gate.yml@${{ github.ref }}\n",
        "  cancel-in-progress: ${{ github.event_name == 'pull_request' }}\n",
        "jobs:\n",
    );
    assert_eq!(workflow.matches(concurrency).count(), 1, "{workflow}");
    assert_eq!(workflow.matches("concurrency:").count(), 1, "{workflow}");
    repo.commit("workflow");

    // A workflow written before the group, by 0.2.0, is drift.
    let earlier = workflow.replace(concurrency, "\njobs:\n");
    assert_ne!(earlier, workflow);
    repo.write(".github/workflows/gate.yml", &earlier);
    repo.commit("a workflow from an earlier core");
    let output = repo.gate(&["sync", "--check"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("[gate/sync#ci] .github/workflows/gate.yml"),
        "{}",
        stdout(&output)
    );
}

const ACTION: &str = "0123456789abcdef0123456789abcdef01234567";

/// The text of one job of a workflow, from its id line to the next job.
fn job(workflow: &str, id: &str) -> String {
    let header = format!("  {id}:");
    workflow
        .lines()
        .skip_while(|line| *line != header)
        .enumerate()
        .take_while(|(index, line)| *index == 0 || line.starts_with("   "))
        .fold(String::new(), |mut text, (_, line)| {
            text.push_str(line);
            text.push('\n');
            text
        })
}

#[test]
fn ci_jobs_take_branch_globs_working_directories_after_steps_and_fetch_depth() {
    let repo = with_root(&format!(
        r#"
[ci.github]
workflow = ".github/workflows/gate.yml"
branches = ["**"]
checkout = "actions/checkout@{ACTION}"

[ci.github.jobs.static]
name = "Static"
timeout-minutes = 10
working-directory = "apps/server"
steps = [
  {{ name = "Node", uses = "actions/setup-node@{ACTION}" }},
  {{ name = "Install", run = "npm ci" }},
  {{ name = "Tools", run = "make", working-directory = "tools/gen" }},
]
after = [
  {{ name = "Upload", uses = "actions/upload-artifact@{ACTION}", with = {{ path = "reports/" }}, if = "always() && hashFiles('reports/**') != ''" }},
  {{ name = "Summary", run = "cat summary.txt", if = "always()" }},
]
run = ["run static"]

[ci.github.jobs.history]
name = "History"
timeout-minutes = 10
run = ["run history"]

[ci.github.jobs.revision]
name = "Revision"
timeout-minutes = 10
run = ["check --rev HEAD"]

[ci.github.jobs.shallow]
name = "Shallow"
timeout-minutes = 10
fetch-depth = 5
run = ["check --budget 3"]

[steps]
static = {{ run = "echo static" }}
policy = {{ run = "bin/gate check --rev HEAD" }}

[suites]
history = ["policy"]
"#
    ));
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let workflow = repo.read(".github/workflows/gate.yml");

    assert!(
        workflow.contains("  push:\n    branches:\n      - \"**\"\n  pull_request:\n    branches:\n      - \"**\"\n"),
        "{workflow}"
    );

    let static_job = &job(&workflow, "static");
    assert!(
        static_job.contains(&format!(
            "\n    steps:\n      - name: Check out the repository\n        uses: actions/checkout@{ACTION}\n        with:\n          persist-credentials: false\n      - name: Node\n        uses: actions/setup-node@{ACTION}\n      - name: Install\n        run: npm ci\n        working-directory: apps/server\n      - name: Tools\n        run: make\n        working-directory: tools/gen\n      - name: gate run static\n        run: bin/gate run static\n        env:\n          GATE_BRANCH: ${{{{ github.head_ref || github.ref_name }}}}\n          GATE_GITHUB_TOKEN: ${{{{ secrets.GATE_GITHUB_TOKEN || github.token }}}}\n      - name: Upload\n        if: always() && hashFiles('reports/**') != ''\n        uses: actions/upload-artifact@{ACTION}\n        with:\n          path: reports/\n      - name: Summary\n        if: always()\n        run: cat summary.txt\n        working-directory: apps/server\n"
        )),
        "no fetch-depth without history; the job's directory for its run steps, not for actions or gate commands; after steps after the gate commands: {static_job}"
    );

    for id in ["history", "revision"] {
        assert!(
            job(&workflow, id).contains(&format!(
                "      - name: Check out the repository\n        uses: actions/checkout@{ACTION}\n        with:\n          fetch-depth: 0\n"
            )),
            "{id}: {}",
            job(&workflow, id)
        );
    }
    let shallow = &job(&workflow, "shallow");
    assert!(
        shallow.contains("\n          fetch-depth: 5\n"),
        "{shallow}"
    );
    assert!(
        shallow
            .contains("\n        run: bin/gate check --budget 3 --no-overrides --budget-fails\n"),
        "an entry's own --budget fails the job too: {shallow}"
    );
}

#[test]
fn ci_steps_refuse_if_before_the_gate_commands_and_a_directory_on_an_action() {
    for (extra, message) in [
        (
            "steps = [{ name = \"Early\", run = \"true\", if = \"always()\" }]\n",
            "jobs.lint: steps 1: if belongs to an after step",
        ),
        (
            &*format!(
                "steps = [{{ uses = \"actions/setup-node@{ACTION}\", working-directory = \"apps\" }}]\n"
            ),
            "jobs.lint: steps 1: working-directory belongs to a run step",
        ),
    ] {
        let repo = with_root(&format!(
            "\n[ci.github]\nworkflow = \".github/workflows/gate.yml\"\ncheckout = \"actions/checkout@{ACTION}\"\n\n[ci.github.jobs.lint]\nname = \"Lint\"\ntimeout-minutes = 10\nrun = [\"check\"]\n{extra}"
        ));
        let output = repo.gate(&["sync"]);
        assert_eq!(output.status.code(), Some(2), "{extra}");
        assert!(stderr(&output).contains(message), "{}", stderr(&output));
    }

    for (hooks, message) in [
        (
            "pre-push = [\"check --push main..topic\"]",
            "write --push without a value; the hook or the job supplies it",
        ),
        (
            "pre-push = [\"check --push=main..topic\"]",
            "write --push without a value; the hook or the job supplies it",
        ),
        (
            "commit-msg = [\"check --commit-message=message.txt\"]",
            "write --commit-message without a value; the hook or the job supplies it",
        ),
        (
            "pre-push = [\"check --push --target-ref refs/heads/main\"]",
            "remove --target-ref; the hook or the job passes it with --push",
        ),
        (
            "pre-commit = [\"check --staged --hook\"]",
            "remove --hook; the generated hooks pass it to every check, and a job is not a hook",
        ),
    ] {
        let repo = with_root(&format!("\n[hooks]\n{hooks}\n"));
        let output = repo.gate(&["sync"]);
        assert_eq!(output.status.code(), Some(2), "{hooks}");
        assert!(
            stderr(&output).contains(message),
            "{hooks}: {}",
            stderr(&output)
        );
    }

    for run in [
        "check --commit-message",
        "check --commit-message=message.txt",
        "check --push=main..topic",
        "check --hook",
    ] {
        let repo = with_root(&format!(
            "\n[ci.github]\nworkflow = \".github/workflows/gate.yml\"\ncheckout = \"actions/checkout@{ACTION}\"\n\n[ci.github.jobs.lint]\nname = \"Lint\"\ntimeout-minutes = 10\nrun = [\"{run}\"]\n"
        ));
        let output = repo.gate(&["sync"]);
        assert_eq!(output.status.code(), Some(2), "{run}");
        let expected = if run == "check --commit-message" {
            "jobs.lint: \"check --commit-message\": a job has no commit message file; --commit-message belongs in a [hooks] commit-msg entry"
        } else if run == "check --hook" {
            "jobs.lint: \"check --hook\": remove --hook; the generated hooks pass it to every check, and a job is not a hook"
        } else {
            "without a value; the hook or the job supplies it"
        };
        assert!(
            stderr(&output).contains(expected),
            "{run}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn target_ref_needs_push_and_a_full_ref_name() {
    let repo = with_root("\n# --target-ref without --push\n");
    let output = repo.gate(&["check", "--target-ref", "refs/heads/main"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("--push <RANGE>"),
        "{}",
        stderr(&output)
    );
    let output = repo.gate(&["run", "lint", "--target-ref", "refs/heads/main"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("--push <RANGE>"),
        "{}",
        stderr(&output)
    );

    let output = repo.gate(&["check", "--push", "HEAD~1..HEAD", "--target-ref", "main"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains(
            "\"main\" is not a full ref; pass the full remote ref, such as refs/heads/main"
        ),
        "{}",
        stderr(&output)
    );

    // A ref needs a name after refs/.
    for bare in ["refs/", "refs/heads/"] {
        let output = repo.gate(&["check", "--push", "HEAD~1..HEAD", "--target-ref", bare]);
        assert_eq!(output.status.code(), Some(2), "{bare}");
        assert!(
            stderr(&output).contains(&format!("{bare:?} is not a full ref")),
            "{bare}: {}",
            stderr(&output)
        );
    }

    // clap drops `requires = "push"` beside an argument that conflicts with --push: check and run
    // refuse it themselves, with the same message.
    let refused: [&[&str]; 3] = [
        &["check", "--staged", "--target-ref", "refs/heads/main"],
        &["check", "--rev", "HEAD", "--target-ref", "refs/heads/main"],
        &["run", "lint", "--staged", "--target-ref", "refs/heads/main"],
    ];
    for arguments in refused {
        let output = repo.gate(arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(
            stderr(&output).contains(
                "--target-ref names the remote ref a --push range updates; pass it with --push"
            ),
            "{arguments:?}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn a_push_flag_anywhere_in_an_entry_gets_the_range_and_only_in_pre_push() {
    let repo = with_root("\n[hooks]\npre-push = [\"check --push --fail-fast\"]\n");
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        repo.read(".githooks/pre-push").contains(
            "  bin/gate check --push \"$remote_sha..$local_sha\" --target-ref \"$remote_ref\" --fail-fast --hook </dev/null\n"
        ),
        "{}",
        repo.read(".githooks/pre-push")
    );

    let repo = with_root("\n[hooks]\npre-commit = [\"check --push --fail-fast\"]\n");
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains(
            "[hooks] pre-commit: \"--push\" checks the pushed refs, which only pre-push receives"
        ),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_configuration_whose_hooks_or_workflow_sync_refuses_does_not_load() {
    let action = "0123456789abcdef0123456789abcdef01234567";
    let cases = [
        (
            "\n[hooks]\npre-commit = [\"check --push\"]\n".to_owned(),
            "[hooks] pre-commit: \"--push\" checks the pushed refs, which only pre-push receives",
        ),
        (
            format!(
                "\n[ci.github]\nworkflow = \".github/workflows/gate.yml\"\ncheckout = \"actions/checkout@{action}\"\n\n[ci.github.jobs.lint]\nname = \"Lint\"\ntimeout-minutes = 10\nrun = [\"check\"]\nprojects = [\"nowhere\"]\n"
            ),
            "[ci.github] jobs.lint: projects names \"nowhere\", which is not a project",
        ),
    ];
    for (section, message) in &cases {
        let repo = with_root(section);
        // Not only gate sync: every command that loads the configuration refuses it, so a
        // configuration that loads always generates its own files.
        for arguments in [&["check"][..], &["list"][..], &["sync", "--check"][..]] {
            let output = repo.gate(arguments);
            assert_eq!(
                output.status.code(),
                Some(2),
                "{arguments:?}: {}",
                stderr(&output)
            );
            assert!(
                stderr(&output).contains(message),
                "{arguments:?}: {}",
                stderr(&output)
            );
        }
    }
}

#[test]
fn every_pushed_ref_is_checked_even_when_a_call_reads_its_input() {
    let repo = with_root("\n[hooks]\npre-push = [\"check --push\"]\n");
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    // A launcher that records its arguments and swallows its input, as a step reading stdin would.
    repo.write(
        "bin/gate",
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> calls.log\ncat > /dev/null\n",
    );
    executable(&repo, "bin/gate");
    let one = "a".repeat(40);
    let two = "b".repeat(40);
    let base = "c".repeat(40);
    let refs = format!(
        "refs/heads/one {one} refs/heads/one {base}\nrefs/heads/two {two} refs/heads/two {base}\n"
    );
    let mut child = isolated(&mut Command::new("sh"))
        .arg(".githooks/pre-push")
        .current_dir(repo.path())
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("hook");
    std::io::Write::write_all(child.stdin.as_mut().expect("stdin"), refs.as_bytes()).expect("refs");
    drop(child.stdin.take());
    assert!(child.wait().expect("hook").success());
    assert_eq!(
        repo.read("calls.log"),
        format!(
            "check --push {base}..{one} --target-ref refs/heads/one --hook\ncheck --push {base}..{two} --target-ref refs/heads/two --hook\n"
        ),
        "each pushed ref, with the remote ref it goes to"
    );
}

#[test]
fn a_step_reads_nothing_from_the_input_of_gate() {
    let repo = with_root("\n[steps]\nslurp = { run = \"cat\" }\n");
    let mut child = isolated(&mut Command::new(env!("CARGO_BIN_EXE_gate")))
        .args(["run", "slurp"])
        .current_dir(repo.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("gate");
    std::io::Write::write_all(
        child.stdin.as_mut().expect("stdin"),
        b"refs that belong to the hook\n",
    )
    .expect("input");
    let output = child.wait_with_output().expect("gate");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        !stdout(&output).contains("refs that belong"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn a_changed_files_step_on_the_first_push_of_a_branch_gets_every_tracked_path() {
    let repo = with_root(
        "\n[steps]\nlint = { run = \"echo linted\", files = \"changed\", glob = \"**/*.ts\" }\n",
    );
    let output = repo.gate(&[
        "run",
        "lint",
        "--push",
        &format!("{}..HEAD", "0".repeat(40)),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains(
            "linted apps/server/src/main.ts apps/web/index.ts packages/shared/index.ts\n"
        ),
        "a zero base selects everything, so every tracked .ts file is linted: {text}"
    );
}

/// Whether the process `pid` still runs.
fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid.trim()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Whether the process `pid` is gone within ten seconds: a killed grandchild stays visible to
/// `kill -0` until init reaps it, which a loaded machine can delay.
fn gone(pid: &str) -> bool {
    (0..500).any(|_| {
        let gone = !alive(pid);
        if !gone {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        gone
    })
}

/// Waits up to ten seconds for `path` to hold a line.
fn wait_for(path: &Path) -> String {
    for _ in 0..500 {
        if let Ok(text) = std::fs::read_to_string(path)
            && text.ends_with('\n')
        {
            return text;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("{} was not written", path.display());
}

#[test]
fn a_timed_out_step_loses_children_that_ignore_term() {
    let repo = with_root(
        "\n[steps]\nstubborn = { run = \"(trap '' TERM; sleep 30) & echo $! > child.pid; wait\", timeout = 1 }\n",
    );
    let output = repo.gate(&["run", "stubborn"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("timed out after 1 s"),
        "{}",
        stdout(&output)
    );
    let pid = repo.read("child.pid");
    assert!(gone(&pid), "the child that ignored TERM got KILL");
}

#[test]
fn an_interrupted_run_stops_the_running_step_and_exits_130() {
    let repo = with_root("\n[steps]\nlong = { run = \"sleep 30 & echo $! > child.pid; wait\" }\n");
    let child = isolated(&mut Command::new(env!("CARGO_BIN_EXE_gate")))
        .args(["run", "long"])
        .current_dir(repo.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("gate");
    let pid = wait_for(&repo.path().join("child.pid"));
    let status = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("kill");
    assert!(status.success());
    let output = child.wait_with_output().expect("gate");
    assert_eq!(output.status.code(), Some(130), "{}", stderr(&output));
    assert!(gone(&pid), "the step's background child is gone");
}

const LAUNCHER_TEXT: &str = include_str!("../src/launcher.sh");

/// A repository doctor finds ready: the launcher of this core, a tracked lock pinning the core
/// for both release targets, a writable cache.
fn ready(extra: &str) -> (Repo, tempfile::TempDir) {
    let repo = with_root(&format!("\n# doctor\n{extra}"));
    repo.write("bin/gate", LAUNCHER_TEXT);
    executable(&repo, "bin/gate");
    repo.write("gate.lock", &pinned(&[]));
    repo.commit("lock");
    (repo, tempfile::tempdir().expect("cache"))
}

fn doctor(repo: &Repo, cache: &str) -> (Option<i32>, String) {
    let output = repo.gate_with(&["doctor"], &[("GATE_CACHE_DIR", cache)]);
    (output.status.code(), stdout(&output))
}

#[test]
fn doctor_says_ready_when_every_check_holds() {
    let (repo, cache) = ready("");
    let (code, text) = doctor(&repo, &cache.path().to_string_lossy());
    assert_eq!(code, Some(0), "{text}");
    assert!(
        text.contains(concat!(
            "ok   launcher: bin/gate is the launcher of gate ",
            env!("CARGO_PKG_VERSION"),
            "\n"
        )),
        "{text}"
    );
    assert!(
        text.contains(concat!(
            "ok   lock: gate.lock pins gate ",
            env!("CARGO_PKG_VERSION"),
            " and 0 plugins\n"
        )),
        "{text}"
    );
    assert!(
        text.contains("ok   plugins: gate.lock pins no plugin\n"),
        "{text}"
    );
    assert!(text.ends_with("gate doctor: ready\n"), "{text}");
}

/// What a doctor case does to a ready repository.
type Breaks = Box<dyn Fn(&Repo)>;

#[test]
fn doctor_names_each_failing_check_and_what_to_do() {
    let other = if cfg!(target_os = "macos") {
        "x86_64-unknown-linux-musl"
    } else {
        "aarch64-apple-darwin"
    };
    let cases: Vec<(&str, Breaks, &str)> = vec![
        (
            "",
            Box::new(|repo: &Repo| {
                std::fs::remove_file(repo.path().join("bin/gate")).expect("remove");
            }),
            "fix  launcher: bin/gate is missing; run gate init\n",
        ),
        (
            "",
            Box::new(|repo: &Repo| {
                repo.write("bin/gate", "#!/bin/sh\nexit 0\n");
            }),
            concat!(
                "fix  launcher: bin/gate differs from the launcher gate ",
                env!("CARGO_PKG_VERSION"),
                " writes; run gate init, then commit it\n"
            ),
        ),
        (
            "",
            Box::new(|repo: &Repo| {
                std::fs::remove_file(repo.path().join("bin/gate")).expect("remove");
                repo.write("bin/gate", LAUNCHER_TEXT);
            }),
            "fix  launcher: bin/gate is not executable",
        ),
        (
            "",
            Box::new(|repo: &Repo| {
                repo.git(&["rm", "-q", "gate.lock"]);
                repo.commit("no lock");
            }),
            "fix  lock: gate.lock is not tracked, and bin/gate reads the core from it; run gate lock, then commit gate.lock\n",
        ),
        (
            "",
            Box::new(|repo: &Repo| {
                repo.write("gate.lock", "");
                repo.commit("empty lock");
            }),
            concat!(
                "fix  lock: gate.lock does not match the configuration: it pins no core, and gate.toml asks for ",
                env!("CARGO_PKG_VERSION"),
                " ([gate] version); run gate lock and commit gate.lock\n"
            ),
        ),
        (
            "",
            Box::new(move |repo: &Repo| {
                repo.write(
                    "gate.lock",
                    &format!(
                        "[core]\nversion = \"{VERSION}\"\n\n[core.targets.{other}]\nurl = \"https://example.invalid/gate.tar.gz\"\nsha256 = \"{}\"\n",
                        "a".repeat(64)
                    ),
                );
                repo.commit("other machine");
            }),
            "fix  lock: gate.lock does not match the configuration: it has no core archive for",
        ),
        (
            "\n[hooks]\npre-commit = [\"check --staged\"]\n",
            Box::new(|_: &Repo| {}),
            "fix  hooks: core.hooksPath is not set, so the hooks in .githooks do not run; run gate sync\n",
        ),
    ];
    for (extra, break_it, expected) in cases {
        let (repo, cache) = ready(extra);
        break_it(&repo);
        let (code, text) = doctor(&repo, &cache.path().to_string_lossy());
        assert_eq!(code, Some(1), "{expected}: {text}");
        assert!(text.contains(expected), "{expected}: {text}");
        assert!(
            text.ends_with("gate doctor: fix the lines marked fix\n"),
            "{text}"
        );
    }

    let (repo, _) = ready("");
    let (code, text) = doctor(&repo, "relative/cache");
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("fix  cache: no cache directory"), "{text}");

    let repo = configured();
    repo.write("bin/gate", LAUNCHER_TEXT);
    executable(&repo, "bin/gate");
    let cache = tempfile::tempdir().expect("cache");
    let (code, text) = doctor(&repo, &cache.path().to_string_lossy());
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains("fix  plugins: not in the cache: ElsiKora/Gate:git@0.1.0, ElsiKora/Gate:layers@0.1.0, ElsiKora/Gate:ledger@0.1.0; run gate check once with network access\n"),
        "{text}"
    );
}

#[test]
fn doctor_reports_a_changed_plugin_and_leaves_the_cache_as_it_is() {
    let repo = configured();
    repo.write("bin/gate", LAUNCHER_TEXT);
    executable(&repo, "bin/gate");
    let cache = tempfile::tempdir().expect("cache");
    let blob = cache.path().join("blobs/sha256").join("b".repeat(64));
    std::fs::create_dir_all(blob.parent().expect("parent")).expect("blobs");
    std::fs::write(&blob, b"not the plugin").expect("plant");
    let (code, text) = doctor(&repo, &cache.path().to_string_lossy());
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains("fix  plugins: in the cache but not matching the sha256 gate.lock pins: ElsiKora/Gate:git@0.1.0, ElsiKora/Gate:layers@0.1.0, ElsiKora/Gate:ledger@0.1.0; run gate check once with network access\n"),
        "{text}"
    );
    assert_eq!(
        std::fs::read(&blob).expect("still there"),
        b"not the plugin"
    );
}

#[test]
fn a_workflow_moved_anywhere_leaves_no_stale_copy() {
    let ci = |path: &str| {
        format!(
            "\n# {path}\n[ci.github]\nworkflow = \"{path}\"\ncheckout = \"actions/checkout@{ACTION}\"\n\n[ci.github.jobs.lint]\nname = \"Lint\"\ntimeout-minutes = 10\nrun = [\"check\"]\n"
        )
    };
    let repo = with_root(&ci("ci/gate.yml"));
    assert_eq!(repo.gate(&["sync"]).status.code(), Some(0));
    repo.commit("workflow in ci/");

    repo.write(
        "gate.toml",
        &format!("{ROOT}{}", ci(".github/workflows/gate.yml")),
    );
    repo.commit("moved");
    let output = repo.gate(&["sync", "--check"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout(&output).contains("error[gate/sync#ci] ci/gate.yml: ci/gate.yml was generated by gate sync and gate.toml no longer names it"),
        "{}",
        stdout(&output)
    );
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(!repo.path().join("ci/gate.yml").exists());
}

/// `two_projects` with `aggregate = "<id>"` in `[ci.github]`.
fn two_projects_with_aggregate(id: &str) -> Repo {
    let repo = two_projects();
    let configuration = repo.read("gate.toml").replace(
        "checkout = \"actions/checkout@0123456789abcdef0123456789abcdef01234567\"\n",
        &format!(
            "checkout = \"actions/checkout@0123456789abcdef0123456789abcdef01234567\"\naggregate = \"{id}\"\n"
        ),
    );
    repo.write("gate.toml", &configuration);
    repo.commit("an aggregate job");
    repo
}

/// The aggregate job `two_projects_with_aggregate("gate")` adds after its other jobs.
const AGGREGATE_JOB: &str = r#"  gate:
    name: gate
    runs-on: ubuntu-24.04
    timeout-minutes: 5
    needs:
      - changes
      - check
      - heavy
    if: ${{ always() }}
    permissions: {}
    steps:
      - name: Require every job to succeed or be skipped
        run: failed=$(printf '%s' "$NEEDS" | jq -r 'to_entries[] | select(.value.result != "success" and .value.result != "skipped") | "\(.key) is \(.value.result)"'); if [ -n "$failed" ]; then printf 'jobs that neither succeeded nor were skipped:\n%s\n' "$failed"; exit 1; fi
        env:
          NEEDS: ${{ toJSON(needs) }}
"#;

#[test]
fn an_aggregate_job_needs_every_job_and_leaves_the_rest_of_the_workflow_as_it_was() {
    let without = two_projects();
    let output = without.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let plain = without.read(".github/workflows/gate.yml");
    assert!(!plain.contains("always()"), "{plain}");

    let with = two_projects_with_aggregate("gate");
    let output = with.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let workflow = with.read(".github/workflows/gate.yml");
    assert_eq!(workflow, format!("{plain}{AGGREGATE_JOB}"));
}

#[test]
fn sync_check_reports_an_edited_aggregate_job() {
    let repo = two_projects_with_aggregate("gate");
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    repo.commit("synced");
    let output = repo.gate(&["sync", "--check"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let generated = repo.read(".github/workflows/gate.yml");

    for (edit, from, to) in [
        (
            "needs",
            "      - heavy\n    if: ${{ always() }}\n",
            "    if: ${{ always() }}\n",
        ),
        (
            "if",
            "    if: ${{ always() }}\n",
            "    if: ${{ success() }}\n",
        ),
    ] {
        assert!(generated.contains(from), "{edit}: {generated}");
        repo.write(".github/workflows/gate.yml", &generated.replace(from, to));
        let output = repo.gate(&["sync", "--check"]);
        assert_eq!(output.status.code(), Some(1), "{edit}: {}", stdout(&output));
        assert!(
            stdout(&output).contains(
                "error[gate/sync#ci] .github/workflows/gate.yml: .github/workflows/gate.yml differs from what gate sync writes"
            ),
            "{edit}: {}",
            stdout(&output)
        );
    }
}

#[test]
fn an_aggregate_id_that_another_job_has_is_a_configuration_error() {
    for (id, message) in [
        (
            "heavy",
            "[ci.github] aggregate = \"heavy\": the id is a job of [ci.github.jobs]; the aggregate is a job of its own, so choose an id no job has",
        ),
        (
            "changes",
            "[ci.github] aggregate = \"changes\": the id is the generated job that lists the changed projects; choose another id",
        ),
        ("All Jobs", "\"All Jobs\" is not a valid job id"),
        (
            "1-required",
            "\"1-required\" is not a valid job id; start with a lowercase letter",
        ),
    ] {
        let repo = two_projects_with_aggregate(id);
        for command in [&["sync"][..], &["sync", "--check"], &["check"]] {
            let output = repo.gate(command);
            assert_eq!(output.status.code(), Some(2), "{id} {command:?}");
            assert!(
                stderr(&output).contains("configuration error")
                    && stderr(&output).contains(message),
                "{id} {command:?}: {}",
                stderr(&output)
            );
        }
        assert!(
            !repo.path().join(".github/workflows/gate.yml").exists(),
            "{id}"
        );
    }

    // A job of [ci.github.jobs] follows the same rule: GitHub refuses the whole workflow otherwise.
    let repo = two_projects();
    let configuration = repo
        .read("gate.toml")
        .replace("[ci.github.jobs.check]", "[ci.github.jobs.1-check]");
    repo.write("gate.toml", &configuration);
    repo.commit("a job id that starts with a digit");
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(2), "{}", stdout(&output));
    assert!(
        stderr(&output)
            .contains("\"1-check\" is not a valid job id; start with a lowercase letter"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_step_gets_the_arguments_after_two_dashes_and_with_stdin_the_input_of_gate() {
    let repo = with_root(
        "\n[steps]\nfirst = { run = \"printf 'first %s %s\\\\n' \\\"$1\\\" \\\"$2\\\"; cat\" }\nsecond = { run = \"printf 'second %s\\\\n' \\\"$1\\\"; cat\" }\n\n[suites]\nboth = [\"first\", \"second\"]\n",
    );
    let mut child = isolated(&mut Command::new(env!("CARGO_BIN_EXE_gate")))
        .args(["run", "both", "--stdin", "--", "message file", "two"])
        .current_dir(repo.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("gate");
    std::io::Write::write_all(child.stdin.as_mut().expect("stdin"), b"refs/heads/main\n")
        .expect("input");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("gate");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains(
            "first message file two\nrefs/heads/main\nsecond message file\nrefs/heads/main\n"
        ),
        "each step gets the arguments as $1, $2 and its own copy of the input: {text}"
    );
}

#[test]
fn a_run_entry_of_a_hook_passes_the_hook_arguments_and_in_pre_push_the_refs() {
    let repo = with_root(
        r#"
[hooks]
commit-msg = ["run message"]
pre-push = ["run refs", "run refs --push"]

[steps]
message = { run = "cp \"$1\" seen-message.txt" }
refs = { run = "printf '%s\\n' \"$1\" >> seen-remote.txt; cat >> seen-refs.txt" }
"#,
    );
    launcher(&repo);
    let output = repo.gate(&["sync"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let pre_push = repo.read(".githooks/pre-push");
    assert!(
        pre_push.contains(
            "{ [ -z \"$refs\" ] || printf '%s\\n' \"$refs\"; } | bin/gate run refs --stdin -- \"$@\"\n"
        ),
        "{pre_push}"
    );
    assert_eq!(
        repo.read(".githooks/commit-msg"),
        format!("#!/bin/sh\n{MARKER}\nset -e\nbin/gate run message -- \"$@\"\n")
    );

    repo.write("message.txt", "feat: a message\n");
    let status = isolated(&mut Command::new("sh"))
        .args([".githooks/commit-msg", "message.txt"])
        .current_dir(repo.path())
        .env("GATE_CACHE_DIR", repo.cache.path())
        .status()
        .expect("commit-msg");
    assert!(status.success());
    assert_eq!(repo.read("seen-message.txt"), "feat: a message\n");

    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo.path())
        .output()
        .expect("rev-parse");
    let head = String::from_utf8_lossy(&head.stdout).trim().to_owned();
    let refs = format!(
        "refs/heads/main {head} refs/heads/main {}\n",
        "0".repeat(40)
    );
    let mut child = isolated(&mut Command::new("sh"))
        .args([
            ".githooks/pre-push",
            "origin",
            "https://example.invalid/repo.git",
        ])
        .current_dir(repo.path())
        .env("GATE_CACHE_DIR", repo.cache.path())
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("pre-push");
    std::io::Write::write_all(child.stdin.as_mut().expect("stdin"), refs.as_bytes()).expect("refs");
    drop(child.stdin.take());
    assert!(child.wait().expect("pre-push").success());
    assert_eq!(
        repo.read("seen-remote.txt"),
        "origin\norigin\n",
        "both run entries get the remote's name as $1"
    );
    assert_eq!(
        repo.read("seen-refs.txt"),
        format!("{refs}{refs}"),
        "both run entries get the pushed refs on their standard input"
    );
}

#[test]
fn a_step_gets_no_token_gate_reads_releases_with_unless_its_env_sets_one() {
    let repo = with_root(
        "\n[steps]\nplain = { run = \"echo \\\"tokens ${GATE_GITHUB_TOKEN-none} ${GITHUB_TOKEN-none}\\\"\" }\nown = { run = \"echo \\\"own $GITHUB_TOKEN\\\"\", env = { GITHUB_TOKEN = \"set-by-the-step\" } }\n\n[suites]\nboth = [\"plain\", \"own\"]\n",
    );
    let output = repo.gate_with(
        &["run", "both"],
        &[
            ("GATE_GITHUB_TOKEN", "secret-one"),
            ("GITHUB_TOKEN", "secret-two"),
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("tokens none none\n"), "{text}");
    assert!(text.contains("own set-by-the-step\n"), "{text}");
    assert!(!text.contains("secret-"), "{text}");
}

#[test]
fn a_ci_job_without_timeout_minutes_or_with_an_empty_release_is_a_configuration_error() {
    for (job, message) in [
        (
            "name = \"Lint\"\nrun = [\"check\"]\n",
            "[ci.github] jobs.lint: timeout-minutes is not set; give the job's time limit in minutes",
        ),
        (
            "name = \"Lint\"\ntimeout-minutes = 0\nrun = [\"check\"]\n",
            "[ci.github] jobs.lint: timeout-minutes is 0",
        ),
        (
            &*format!(
                "name = \"Lint\"\ntimeout-minutes = 10\nrun = [\"check\"]\nsteps = [{{ name = \"Node\", uses = \"actions/setup-node@{ACTION} # \" }}]\n"
            ),
            "the release after \" # \" is empty or spans lines",
        ),
    ] {
        let repo = with_root(&format!(
            "\n[ci.github]\nworkflow = \".github/workflows/gate.yml\"\ncheckout = \"actions/checkout@{ACTION} # v4.2.2\"\n\n[ci.github.jobs.lint]\n{job}"
        ));
        let output = repo.gate(&["sync"]);
        assert_eq!(output.status.code(), Some(2), "{job}");
        assert!(stderr(&output).contains(message), "{}", stderr(&output));
    }
}

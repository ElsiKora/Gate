//! The workflow `gate sync` writes, with a plugin runtime that says which rules read history: a
//! job checks out the whole history exactly when one of its gate commands runs such a rule. And
//! what `gate sync` never writes or removes, whatever a generator returns: `.git`, the core's own
//! files, the hooks directory, and anything behind a symbolic link.

use gate_host::config::PluginItem;
use gate_host::sync::{GeneratedFile, Generators, Output, check, owned, plan, write};
use gate_host::{ErrorKind, Result, RuleFilter, Source, Workspace};

use crate::fixture::{ROOT, Repo, monorepo};

/// A runtime whose only rule that reads history is `history/commits`.
struct OneHistoryRule;

impl Generators for OneHistoryRule {
    fn generate(
        &self,
        _project: Option<&str>,
        _generator: &PluginItem,
        _options: Option<&serde_json::Value>,
    ) -> Result<Vec<GeneratedFile>> {
        Ok(Vec::new())
    }

    fn reads_history(&self, filter: &RuleFilter) -> bool {
        filter.allows("history/commits")
    }
}

#[test]
fn a_job_checks_out_the_history_when_a_command_runs_a_rule_that_reads_it() {
    let repo = monorepo();
    repo.write(
        "gate.toml",
        &format!(
            r#"{ROOT}
[ci.github]
workflow = ".github/workflows/gate.yml"
checkout = "actions/checkout@0123456789abcdef0123456789abcdef01234567"

[ci.github.jobs.all]
name = "All rules"
timeout-minutes = 10
run = ["check"]

[ci.github.jobs.named]
name = "The history rule"
timeout-minutes = 10
run = ["check --rule history/commits"]

[ci.github.jobs.other]
name = "Another rule"
timeout-minutes = 10
run = ["check --rule layers/imports"]

[ci.github.jobs.suite]
name = "A suite that calls it"
timeout-minutes = 10
run = ["run policy"]

[steps]
policy = {{ run = "bin/gate check --rule=history/commits" }}
"#
        ),
    );
    repo.commit("ci");
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    let outputs = plan(&workspace, &OneHistoryRule).expect("plan");
    let workflow = String::from_utf8(
        outputs
            .iter()
            .find(|output| output.item == "ci")
            .expect("workflow")
            .contents
            .clone(),
    )
    .expect("utf-8");

    let full = "      - name: Check out the repository\n        uses: actions/checkout@0123456789abcdef0123456789abcdef01234567\n        with:\n          fetch-depth: 0\n          persist-credentials: false\n";
    for (id, deep) in [
        ("all", true),
        ("named", true),
        ("other", false),
        ("suite", true),
    ] {
        let header = format!("  {id}:");
        let job = workflow
            .lines()
            .skip_while(|line| *line != header)
            .enumerate()
            .take_while(|(index, line)| *index == 0 || line.starts_with("   "))
            .fold(String::new(), |mut job, (_, line)| {
                job.push_str(line);
                job.push('\n');
                job
            });
        assert_eq!(job.contains(full), deep, "{id}: {job}");
    }

    // A repository secret of that name reads private plugin releases, else the job's token; and
    // the branch under test, which a pull request's detached checkout does not name. Both only on
    // the bin/gate steps: not on the workflow, actions or other steps.
    let gate_steps = workflow.matches("        run: bin/gate ").count();
    assert_eq!(gate_steps, 6, "{workflow}");
    for line in [
        "          GATE_GITHUB_TOKEN: ${{ secrets.GATE_GITHUB_TOKEN || github.token }}\n",
        "          GATE_BRANCH: ${{ github.head_ref || github.ref_name }}\n",
    ] {
        assert_eq!(
            workflow.matches(line).count(),
            gate_steps,
            "{line}{workflow}"
        );
    }
    assert!(
        !workflow.contains("\nenv:"),
        "no workflow-level env: {workflow}"
    );
}

/// A runtime whose one generator returns the given paths.
struct Writes(Vec<&'static str>);

impl Generators for Writes {
    fn generate(
        &self,
        _project: Option<&str>,
        _generator: &PluginItem,
        _options: Option<&serde_json::Value>,
    ) -> Result<Vec<GeneratedFile>> {
        Ok(self
            .0
            .iter()
            .map(|path| GeneratedFile {
                path: (*path).to_owned(),
                contents: b"generated\n".to_vec(),
                seed: false,
            })
            .collect())
    }

    fn reads_history(&self, _filter: &RuleFilter) -> bool {
        false
    }
}

fn with_generator() -> Repo {
    let repo = monorepo();
    repo.write(
        "gate.toml",
        &format!(
            "{ROOT}\n[plugins]\ndemo = {{ source = \"ElsiKora/Gate:demo@0.1.0\" }}\n\n[generate]\n\"demo/files\" = {{}}\n"
        ),
    );
    repo.commit("generate");
    repo
}

fn output(path: &str) -> Output {
    Output {
        item: "demo/files".to_owned(),
        path: path.to_owned(),
        contents: b"generated\n".to_vec(),
        executable: false,
        seed: false,
    }
}

const PROTECTED: &[(&str, &str)] = &[
    (".git/hooks/pre-commit", "is inside .git"),
    ("docs/.GIT/config", "is inside .git"),
    ("gate.toml", "is a configuration file of gate"),
    ("apps/api/Gate.Lock", "is a configuration file of gate"),
    (
        "apps/api/gate.exceptions.toml",
        "is a configuration file of gate",
    ),
    ("bin/gate", "is the launcher"),
    ("BIN/Gate", "is the launcher"),
    (".githooks/pre-commit", "is in .githooks"),
    (".GitHooks/notes", "is in .githooks"),
];

#[test]
fn a_generator_output_on_a_protected_path_is_refused_by_the_plan() {
    let repo = with_generator();
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    assert_eq!(
        plan(&workspace, &Writes(vec!["docs/generated.md"]))
            .expect("plan")
            .len(),
        1
    );
    for (path, reason) in PROTECTED {
        let error = plan(&workspace, &Writes(vec![path])).expect_err(path);
        assert!(
            error
                .message()
                .starts_with(&format!("demo/files writes {path}, which {reason}")),
            "{path}: {}",
            error.message()
        );
    }
}

#[test]
fn the_writer_refuses_a_protected_path_and_changes_nothing() {
    let repo = with_generator();
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    let before = std::fs::read(repo.path().join("gate.toml")).expect("gate.toml");
    for (path, reason) in PROTECTED {
        let error = write(&workspace, &[output(path)], &[]).expect_err(path);
        assert!(
            error.message().contains(&format!(
                "gate sync refuses to write or remove {path}: it {reason}"
            )),
            "{path}: {}",
            error.message()
        );
        if *path != "gate.toml" {
            assert!(!repo.path().join(path).exists(), "{path} was written");
        }
    }
    assert_eq!(
        std::fs::read(repo.path().join("gate.toml")).expect("gate.toml"),
        before
    );
}

#[cfg(unix)]
#[test]
fn the_writer_refuses_to_write_through_a_symbolic_link() {
    let repo = with_generator();
    let outside = tempfile::tempdir().expect("outside");
    std::os::unix::fs::symlink(outside.path(), repo.path().join("generated")).expect("link");
    std::fs::write(outside.path().join("kept.md"), b"kept\n").expect("kept");
    std::os::unix::fs::symlink(outside.path().join("kept.md"), repo.path().join("notes.md"))
        .expect("file link");
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");

    let error =
        write(&workspace, &[output("generated/deep/file.md")], &[]).expect_err("parent link");
    assert!(
        error.message().contains(
            "gate sync refuses to write or remove generated/deep/file.md: generated is a symbolic link"
        ),
        "{}",
        error.message()
    );
    assert!(!outside.path().join("deep").exists());

    let error = write(&workspace, &[output("notes.md")], &[]).expect_err("file link");
    assert!(
        error
            .message()
            .contains("cannot write notes.md: it is a symbolic link"),
        "{}",
        error.message()
    );
    assert_eq!(
        std::fs::read(outside.path().join("kept.md")).expect("kept"),
        b"kept\n"
    );
}

#[cfg(unix)]
#[test]
fn the_writer_refuses_to_remove_through_a_symbolic_link() {
    let repo = monorepo();
    repo.write(
        "gate.toml",
        &format!("{ROOT}\n[hooks]\npre-commit = [\"check --staged\"]\n"),
    );
    repo.commit("hooks");
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    let outputs = plan(&workspace, &Writes(Vec::new())).expect("plan");
    write(&workspace, &outputs, &[]).expect("write the hook");
    // The hook calls a launcher this repository does not have; it is only a file here.
    repo.git(&["config", "--unset", "core.hooksPath"]);
    repo.add().commit("the hook");

    // The hook is no longer configured, and its directory now points outside the repository.
    repo.write("gate.toml", ROOT);
    repo.commit("no hooks");
    let outside = tempfile::tempdir().expect("outside");
    let hooks = repo.path().join(".githooks");
    std::fs::rename(hooks.join("pre-commit"), outside.path().join("pre-commit")).expect("move");
    std::fs::remove_dir(&hooks).expect("empty");
    std::os::unix::fs::symlink(outside.path(), &hooks).expect("link");

    let workspace = Workspace::open(repo.path(), Source::Index).expect("open");
    let error = write(&workspace, &[], &[]).expect_err("removal through a link");
    assert!(
        error.message().contains(
            "gate sync refuses to write or remove .githooks/pre-commit: .githooks is a symbolic link"
        ),
        "{}",
        error.message()
    );
    assert!(
        outside.path().join("pre-commit").exists(),
        "nothing outside was removed"
    );
}

/// A runtime whose one generator writes `generated/new.md` and owns `glob`.
struct Owns(&'static str);

impl Generators for Owns {
    fn generate(
        &self,
        _project: Option<&str>,
        _generator: &PluginItem,
        _options: Option<&serde_json::Value>,
    ) -> Result<Vec<GeneratedFile>> {
        Ok(vec![GeneratedFile {
            path: "generated/new.md".to_owned(),
            contents: b"generated\n".to_vec(),
            seed: false,
        }])
    }

    fn owns(&self, _project: Option<&str>, _generator: &PluginItem) -> Result<Vec<String>> {
        Ok(vec![self.0.to_owned()])
    }

    fn reads_history(&self, _filter: &RuleFilter) -> bool {
        false
    }
}

#[test]
fn a_tracked_file_under_an_owned_glob_that_no_generator_writes_is_reported_and_removed() {
    let repo = with_generator();
    repo.write("generated/old.md", "old\n");
    repo.write("docs/kept.md", "kept\n");
    repo.commit("an earlier output");
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    let generators = Owns("generated/**");
    let outputs = plan(&workspace, &generators).expect("plan");
    let owned = owned(&workspace, &generators).expect("owned");

    let mut paths: Vec<String> = check(&workspace, &outputs, &owned)
        .expect("check")
        .iter()
        .filter_map(|finding| finding.location.as_ref()?.path().map(str::to_owned))
        .collect();
    paths.sort();
    assert_eq!(paths, ["generated/new.md", "generated/old.md"]);

    let synced = write(&workspace, &outputs, &owned).expect("write");
    assert_eq!(synced.removed, ["generated/old.md"]);
    assert!(!repo.path().join("generated/old.md").exists());
    assert!(repo.path().join("generated/new.md").exists());
    assert!(repo.path().join("docs/kept.md").exists());
}

#[test]
fn an_owned_glob_over_a_core_file_never_reports_or_removes_it() {
    let repo = with_generator();
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    let generators = Owns("*.toml");
    let outputs = plan(&workspace, &generators).expect("plan");
    let owned = owned(&workspace, &generators).expect("owned");
    let reported = check(&workspace, &outputs, &owned).expect("check");
    assert!(
        reported.iter().all(|finding| finding
            .location
            .as_ref()
            .and_then(|location| location.path())
            != Some("gate.toml")),
        "{reported:?}"
    );
    let synced = write(&workspace, &outputs, &owned).expect("write");
    assert_eq!(synced.removed, Vec::<String>::new());
    assert!(repo.path().join("gate.toml").exists());
}

#[test]
fn a_nested_projects_gate_toml_under_an_owned_glob_stays_and_the_rest_syncs() {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        &format!(
            "{ROOT}\n[workspace]\nprojects = [\"docs/site\"]\n\n[plugins]\ndemo = {{ source = \"ElsiKora/Gate:demo@0.1.0\" }}\n\n[generate]\n\"demo/files\" = {{}}\n"
        ),
    );
    repo.write("docs/site/gate.toml", "[project]\nname = \"site\"\n");
    repo.write("docs/old.md", "old\n");
    repo.commit("a repository generator owns docs/**");
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    let generators = Owns("docs/**");
    let outputs = plan(&workspace, &generators).expect("plan");
    let owned = owned(&workspace, &generators).expect("owned");

    let synced = write(&workspace, &outputs, &owned).expect("nothing is refused");
    assert_eq!(synced.removed, ["docs/old.md"]);
    assert!(repo.path().join("docs/site/gate.toml").exists());
    assert!(repo.path().join("generated/new.md").exists());
}

#[cfg(unix)]
#[test]
fn a_refused_path_anywhere_in_the_list_leaves_the_tree_as_it_was() {
    let repo = with_generator();
    repo.write("generated/old.md", "old\n");
    repo.commit("an earlier output");
    let outside = tempfile::tempdir().expect("tempdir");
    std::os::unix::fs::symlink(outside.path(), repo.path().join("linked")).expect("link");
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    let generators = Writes(vec!["a/first.md", "linked/second.md"]);
    let outputs = plan(&workspace, &generators).expect("plan");
    let owned = vec![gate_host::sync::Owned {
        item: "demo/files".to_owned(),
        glob: "generated/**".to_owned(),
    }];

    let error = write(&workspace, &outputs, &owned).expect_err("linked is a symbolic link");
    assert!(
        error.message().contains("symbolic link"),
        "{}",
        error.message()
    );
    assert!(
        !repo.path().join("a/first.md").exists(),
        "nothing is written"
    );
    assert!(
        repo.path().join("generated/old.md").exists(),
        "nothing is removed"
    );
    assert!(!outside.path().join("second.md").exists());
}

/// A runtime whose one generator writes `generated/new.md` and seeds `seeds`; it owns `owns`.
struct Seeds(Vec<&'static str>, Vec<&'static str>);

impl Generators for Seeds {
    fn generate(
        &self,
        _project: Option<&str>,
        _generator: &PluginItem,
        _options: Option<&serde_json::Value>,
    ) -> Result<Vec<GeneratedFile>> {
        let mut files = vec![GeneratedFile {
            path: "generated/new.md".to_owned(),
            contents: b"generated\n".to_vec(),
            seed: false,
        }];
        files.extend(self.0.iter().map(|path| GeneratedFile {
            path: (*path).to_owned(),
            contents: b"seed\n".to_vec(),
            seed: true,
        }));
        Ok(files)
    }

    fn owns(&self, _project: Option<&str>, _generator: &PluginItem) -> Result<Vec<String>> {
        Ok(self.1.iter().map(|glob| (*glob).to_owned()).collect())
    }

    fn reads_history(&self, _filter: &RuleFilter) -> bool {
        false
    }
}

/// The paths `gate sync --check` reports.
fn reported(workspace: &Workspace, generators: &dyn Generators) -> Vec<String> {
    let outputs = plan(workspace, generators).expect("plan");
    let owned = owned(workspace, generators).expect("owned");
    let mut paths: Vec<String> = check(workspace, &outputs, &owned)
        .expect("check")
        .iter()
        .filter_map(|finding| finding.location.as_ref()?.path().map(str::to_owned))
        .collect();
    paths.sort();
    paths
}

#[test]
fn a_seed_is_written_only_where_nothing_is_and_never_reported() {
    let repo = with_generator();
    repo.write("docs/edited.md", "a person's text\n");
    repo.commit("an edited seed");
    repo.write("docs/untracked.md", "not added yet\n");
    std::fs::create_dir_all(repo.path().join("docs/directory.md")).expect("directory");
    let generators = Seeds(
        vec![
            "docs/absent.md",
            "docs/edited.md",
            "docs/untracked.md",
            "docs/directory.md",
        ],
        vec!["generated/**"],
    );
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    assert_eq!(
        reported(&workspace, &generators),
        ["generated/new.md"],
        "an absent or edited seed is never drift; the owned file beside it is"
    );

    let outputs = plan(&workspace, &generators).expect("plan");
    let owned = owned(&workspace, &generators).expect("owned");
    let synced = write(&workspace, &outputs, &owned).expect("write");
    assert_eq!(synced.written, ["docs/absent.md", "generated/new.md"]);
    assert_eq!(
        synced.untracked,
        ["docs/absent.md", "docs/untracked.md", "generated/new.md"]
    );
    let text = |path: &str| std::fs::read_to_string(repo.path().join(path)).expect("read");
    assert_eq!(text("docs/absent.md"), "seed\n");
    assert_eq!(text("docs/edited.md"), "a person's text\n");
    assert_eq!(text("docs/untracked.md"), "not added yet\n");
    assert!(repo.path().join("docs/directory.md").is_dir());

    std::fs::write(repo.path().join("docs/absent.md"), "filled\n").expect("fill");
    let synced = write(&workspace, &outputs, &owned).expect("write again");
    assert_eq!(synced.written, Vec::<String>::new());
    assert_eq!(text("docs/absent.md"), "filled\n");
}

#[cfg(unix)]
#[test]
fn a_seed_whose_path_is_a_symbolic_link_is_left_alone() {
    let repo = with_generator();
    let outside = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(repo.path().join("docs")).expect("docs");
    std::os::unix::fs::symlink(
        outside.path().join("target.md"),
        repo.path().join("docs/linked.md"),
    )
    .expect("link");
    let generators = Seeds(vec!["docs/linked.md"], Vec::new());
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    let outputs = plan(&workspace, &generators).expect("plan");
    let synced = write(&workspace, &outputs, &[]).expect("a seed at a link is not an error");
    assert_eq!(synced.written, ["generated/new.md"]);
    assert!(!outside.path().join("target.md").exists());
}

#[test]
fn a_seed_under_a_glob_its_generator_owns_is_refused() {
    let repo = with_generator();
    let generators = Seeds(vec!["generated/seeded.md"], vec!["generated/**"]);
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    let error = plan(&workspace, &generators).expect_err("an owned seed is refused");
    assert!(
        error.message().contains("seeds generated/seeded.md")
            && error.message().contains("\"generated/**\", a glob it owns"),
        "{}",
        error.message()
    );
    assert_eq!(error.kind(), ErrorKind::Environment, "{}", error.message());
}

#[test]
fn a_seed_filled_and_committed_is_never_drift_never_rewritten_and_never_stale() {
    let repo = with_generator();
    let generators = Seeds(vec!["docs/seeded.md"], vec!["generated/**"]);
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    let outputs = plan(&workspace, &generators).expect("plan");
    let globs = owned(&workspace, &generators).expect("owned");
    let synced = write(&workspace, &outputs, &globs).expect("write");
    assert_eq!(synced.written, ["docs/seeded.md", "generated/new.md"]);
    repo.write("docs/seeded.md", "filled\n");
    repo.commit("the seed, filled");

    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    assert_eq!(
        reported(&workspace, &generators),
        Vec::<String>::new(),
        "a filled seed is not drift"
    );
    let outputs = plan(&workspace, &generators).expect("plan");
    let synced = write(&workspace, &outputs, &globs).expect("write");
    assert_eq!(synced.written, Vec::<String>::new());
    assert_eq!(synced.removed, Vec::<String>::new());

    // Its generator removed from [generate]: the page stays the project's.
    repo.write("gate.toml", &format!("{ROOT}\n"));
    repo.commit("no generator");
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    assert_eq!(reported(&workspace, &generators), Vec::<String>::new());
    let outputs = plan(&workspace, &generators).expect("plan");
    let globs = owned(&workspace, &generators).expect("owned");
    let synced = write(&workspace, &outputs, &globs).expect("write");
    assert_eq!(synced.removed, Vec::<String>::new());
    assert_eq!(
        std::fs::read_to_string(repo.path().join("docs/seeded.md")).expect("read"),
        "filled\n"
    );
}

/// A runtime of two generators: `demo/files` writes `generated/new.md` and owns `docs/**`;
/// `docs/layout` seeds `docs/README.md`.
struct Overlap;

impl Generators for Overlap {
    fn generate(
        &self,
        _project: Option<&str>,
        generator: &PluginItem,
        _options: Option<&serde_json::Value>,
    ) -> Result<Vec<GeneratedFile>> {
        Ok(if generator.plugin() == "docs" {
            vec![GeneratedFile {
                path: "docs/README.md".to_owned(),
                contents: b"seed\n".to_vec(),
                seed: true,
            }]
        } else {
            vec![GeneratedFile {
                path: "generated/new.md".to_owned(),
                contents: b"generated\n".to_vec(),
                seed: false,
            }]
        })
    }

    fn owns(&self, _project: Option<&str>, generator: &PluginItem) -> Result<Vec<String>> {
        Ok(if generator.plugin() == "demo" {
            vec!["docs/**".to_owned()]
        } else {
            Vec::new()
        })
    }

    fn reads_history(&self, _filter: &RuleFilter) -> bool {
        false
    }
}

#[test]
fn a_seed_under_a_glob_another_generator_owns_is_refused_before_anything_is_written() {
    let repo = monorepo();
    repo.write(
        "gate.toml",
        &format!(
            "{ROOT}\n[plugins]\ndemo = {{ source = \"ElsiKora/Gate:demo@0.1.0\" }}\ndocs = {{ source = \"ElsiKora/Gate:docs@0.3.0\" }}\n\n[generate]\n\"demo/files\" = {{}}\n\"docs/layout\" = {{}}\n"
        ),
    );
    repo.write("docs/README.md", "a person's page\n");
    repo.commit("two generators, one owning where the other seeds");
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    let error = plan(&workspace, &Overlap).expect_err("the seed is refused");
    assert_eq!(error.kind(), ErrorKind::Environment, "{}", error.message());
    assert!(
        error.message().contains(
            "docs/layout seeds docs/README.md, which is under \"docs/**\", a glob demo/files owns"
        ),
        "{}",
        error.message()
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("docs/README.md")).expect("read"),
        "a person's page\n",
        "a refused plan writes and removes nothing"
    );
    assert!(!repo.path().join("generated/new.md").exists());
}

/// Whether `program --version` runs.
fn available(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// The aggregate job's command run as a GitHub runner runs a `run` step (`bash -e`) with
/// `toJSON(needs)` in `NEEDS`, in an empty directory: its exit code, its output, and whether it
/// left anything in that directory.
fn aggregate(needs: &serde_json::Value) -> (Option<i32>, String, bool) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let output = std::process::Command::new("bash")
        .args(["-e", "-c", gate_host::sync::github::AGGREGATE_SCRIPT])
        .env("NEEDS", needs.to_string())
        .current_dir(directory.path())
        .output()
        .expect("bash runs");
    let left = std::fs::read_dir(directory.path())
        .expect("the directory")
        .next()
        .is_some();
    (
        output.status.code(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        left,
    )
}

#[test]
fn the_aggregate_passes_on_success_and_skipped_and_fails_naming_any_other_result() {
    if !available("bash") || !available("jq") {
        eprintln!(
            "skipped: the aggregate job's command needs bash and jq, as GitHub's runners have; this machine lacks one"
        );
        return;
    }
    let job = |result: &str| serde_json::json!({ "result": result, "outputs": {} });
    // Outputs are data: a command substitution, quotes and a newline in one change nothing.
    let hostile = serde_json::json!({
        "result": "success",
        "outputs": { "projects": "$(touch injected) `touch injected` \"quoted\" 'single'\nsecond line" },
    });

    for (needs, code, named) in [
        (
            serde_json::json!({ "changes": job("success"), "lint": job("skipped") }),
            Some(0),
            vec![],
        ),
        (
            serde_json::json!({ "changes": job("success"), "lint": job("failure"), "test": job("success") }),
            Some(1),
            vec!["lint is failure"],
        ),
        (
            serde_json::json!({ "changes": job("cancelled"), "lint": job("skipped") }),
            Some(1),
            vec!["changes is cancelled"],
        ),
        (
            serde_json::json!({ "changes": hostile.clone(), "lint": job("success") }),
            Some(0),
            vec![],
        ),
        (
            serde_json::json!({ "changes": hostile, "lint": job("failure") }),
            Some(1),
            vec!["lint is failure"],
        ),
    ] {
        let (exit, output, left) = aggregate(&needs);
        assert_eq!(exit, code, "{needs}: {output}");
        for name in named {
            assert!(output.contains(name), "{needs}: {output}");
        }
        assert!(!output.contains("changes is success"), "{needs}: {output}");
        assert!(!left, "{needs}: the command wrote a file");
    }
}

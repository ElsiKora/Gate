//! Change selection: which projects run, and why.

use gate_host::{ChangeSet, Decision, Git, Reason, Selection, Source, Workspace};

use crate::fixture::{ROOT, Repo, monorepo};

fn decisions(selection: &Selection) -> (Vec<String>, Vec<String>) {
    let show = |decisions: &[Decision]| {
        decisions
            .iter()
            .map(|decision| format!("{}: {}", decision.project, decision.reason))
            .collect()
    };
    (show(&selection.ran), show(&selection.skipped))
}

fn select(repo: &Repo, changes: &ChangeSet) -> Selection {
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    Selection::select(workspace.projects(), changes, workspace.inputs())
}

#[test]
fn staged_selects_the_staged_project_only() {
    let repo = monorepo();
    repo.write("apps/web/index.ts", "export const web = 2;\n");
    repo.git(&["add", "apps/web/index.ts"]);
    repo.write("apps/server/src/main.ts", "unstaged\n");

    let git = Git::discover(repo.path()).expect("git");
    let changes = ChangeSet::staged(&git).expect("staged");
    assert_eq!(
        changes.paths.iter().collect::<Vec<_>>(),
        ["apps/web/index.ts"]
    );

    let selection = select(&repo, &changes);
    assert_eq!(
        decisions(&selection),
        (
            vec!["web: changed apps/web/index.ts".to_owned()],
            vec![
                "@acme/shared: no change".to_owned(),
                "server: no change".to_owned()
            ]
        )
    );
    assert!(selection.repository);
}

#[test]
fn staged_on_an_unborn_branch_counts_every_staged_path() {
    let repo = Repo::new();
    repo.write("gate.toml", ROOT).write("a.txt", "a\n").add();
    let git = Git::discover(repo.path()).expect("git");
    let changes = ChangeSet::staged(&git).expect("staged");
    assert_eq!(
        changes.paths.iter().collect::<Vec<_>>(),
        ["a.txt", "gate.toml"]
    );
}

#[test]
fn a_change_to_a_dependency_selects_its_dependents() {
    let repo = monorepo();
    repo.git(&["checkout", "-q", "-b", "feature"]);
    repo.write("packages/shared/index.ts", "export const shared = 2;\n")
        .write("packages/shared/extra.ts", "export {};\n");
    repo.commit("change shared");

    let git = Git::discover(repo.path()).expect("git");
    let changes = ChangeSet::since(&git, "main", &Source::WorkTree).expect("since");
    let selection = select(&repo, &changes);
    assert_eq!(
        decisions(&selection).0,
        [
            "@acme/shared: changed packages/shared/extra.ts and 1 more",
            "server: depends on @acme/shared",
            "web: depends on @acme/shared",
        ]
    );
    assert_eq!(selection.skipped, Vec::<Decision>::new());
}

#[test]
fn dependents_are_found_through_a_chain() {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        &format!("{ROOT}\n[workspace]\nprojects = [\"p/*\"]\n"),
    )
    .write("p/a/package.json", "{\"name\": \"a\"}")
    .write(
        "p/b/package.json",
        "{\"name\": \"b\", \"dependencies\": {\"a\": \"*\"}}",
    )
    .write(
        "p/c/package.json",
        "{\"name\": \"c\", \"dependencies\": {\"b\": \"*\"}}",
    )
    .write("p/d/package.json", "{\"name\": \"d\"}");
    repo.commit("initial");
    repo.write("p/a/index.js", "1\n");
    repo.commit("change a");

    let git = Git::discover(repo.path()).expect("git");
    let changes = ChangeSet::since(&git, "HEAD~1", &Source::WorkTree).expect("since");
    assert_eq!(
        decisions(&select(&repo, &changes)),
        (
            vec![
                "a: changed p/a/index.js".to_owned(),
                "b: depends on a".to_owned(),
                "c: depends on b".to_owned()
            ],
            vec!["d: no change".to_owned()]
        )
    );
}

#[test]
fn base_range_includes_uncommitted_work_and_uses_the_common_ancestor() {
    let repo = monorepo();
    repo.git(&["checkout", "-q", "-b", "feature"]);
    repo.write("apps/web/index.ts", "committed on feature\n");
    repo.commit("web");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("packages/shared/index.ts", "moved on main\n");
    repo.commit("main moves");
    repo.git(&["checkout", "-q", "feature"]);
    repo.write("apps/server/src/main.ts", "uncommitted\n");

    let git = Git::discover(repo.path()).expect("git");
    let tree = ChangeSet::since(&git, "main", &Source::WorkTree).expect("tree");
    assert_eq!(
        tree.paths.iter().collect::<Vec<_>>(),
        ["apps/server/src/main.ts", "apps/web/index.ts"],
        "main's own change is not the feature's"
    );

    let head = ChangeSet::since(&git, "main", &Source::Commit("HEAD".to_owned())).expect("rev");
    assert_eq!(head.paths.iter().collect::<Vec<_>>(), ["apps/web/index.ts"]);
}

#[test]
fn push_range_and_a_new_branch() {
    let repo = monorepo();
    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.write("docs/guide.md", "# Guide, again\n");
    let tip = repo.commit("docs");

    let git = Git::discover(repo.path()).expect("git");
    let (changes, range) =
        ChangeSet::push(&git, &format!("{base}..{tip}"), Some("refs/heads/main")).expect("push");
    assert_eq!(range.tip, tip);
    assert_eq!(range.target_ref.as_deref(), Some("refs/heads/main"));
    assert_eq!(changes.paths.iter().collect::<Vec<_>>(), ["docs/guide.md"]);
    let selection = select(&repo, &changes);
    assert!(selection.ran.is_empty(), "docs belong to the root");
    assert!(selection.repository, "repository rules run on any change");

    let zero = "0".repeat(40);
    let (changes, range) =
        ChangeSet::push(&git, &format!("{zero}..{tip}"), None).expect("new branch");
    assert!(changes.everything);
    assert_eq!(range.base, None);
    let selection = select(&repo, &changes);
    assert_eq!(selection.ran.len(), 3);
    assert!(
        selection
            .ran
            .iter()
            .all(|decision| decision.reason == Reason::Everything)
    );
}

#[test]
fn a_change_to_the_root_gate_toml_selects_every_project() {
    let repo = monorepo();
    repo.write(
        "gate.toml",
        &format!("{ROOT}\n[workspace]\nprojects = \"pnpm\"\nignore = []\n"),
    );
    let git = Git::discover(repo.path()).expect("git");
    let changes = ChangeSet::since(&git, "HEAD", &Source::WorkTree).expect("since");
    let selection = select(&repo, &changes);
    assert_eq!(selection.ran.len(), 3);
    assert!(selection.ran.iter().all(|decision| decision.reason
        == Reason::Configuration {
            path: "gate.toml".to_owned()
        }));
}

#[test]
fn no_change_runs_nothing() {
    let repo = monorepo();
    let git = Git::discover(repo.path()).expect("git");
    let changes = ChangeSet::since(&git, "HEAD", &Source::WorkTree).expect("since");
    let selection = select(&repo, &changes);
    assert_eq!(selection.ran, Vec::<Decision>::new());
    assert_eq!(selection.skipped.len(), 3);
    assert!(!selection.repository);
}

#[test]
fn an_unknown_base_is_an_environment_error() {
    let repo = monorepo();
    let git = Git::discover(repo.path()).expect("git");
    let error = ChangeSet::since(&git, "origin/nowhere", &Source::WorkTree).expect_err("unknown");
    assert_eq!(error.kind(), gate_host::ErrorKind::Environment);
    assert!(
        error
            .message()
            .contains("--base origin/nowhere names no commit"),
        "{error}"
    );
}

fn changed_since_head(repo: &Repo) -> (Vec<String>, Vec<String>) {
    let git = Git::discover(repo.path()).expect("git");
    let changes = ChangeSet::since(&git, "HEAD", &Source::WorkTree).expect("since");
    decisions(&select(repo, &changes))
}

#[test]
fn a_change_to_a_workspace_manifest_or_the_baseline_selects_every_project() {
    let repo = monorepo();
    repo.write(
        "pnpm-workspace.yaml",
        "packages:\n  - \"apps/*\"\n  - \"packages/*\"\n  - \"tools/*\"\n",
    );
    let (ran, skipped) = changed_since_head(&repo);
    assert_eq!(
        ran,
        [
            "@acme/shared: configuration input pnpm-workspace.yaml changed",
            "server: configuration input pnpm-workspace.yaml changed",
            "web: configuration input pnpm-workspace.yaml changed",
        ]
    );
    assert_eq!(skipped, Vec::<String>::new());

    let repo = monorepo();
    repo.write("gate/baseline.json", "{\"findings\": []}\n")
        .add();
    let (ran, _) = changed_since_head(&repo);
    assert_eq!(ran.len(), 3, "{ran:?}");
    assert!(
        ran[0].ends_with("configuration input gate/baseline.json changed"),
        "{ran:?}"
    );
}

#[test]
fn a_change_to_a_profiles_options_file_or_a_plugins_reads_selects_its_projects() {
    let repo = monorepo();
    repo.write(
        "gate.toml",
        &format!(
            "{ROOT}\n[workspace]\nprojects = \"pnpm\"\n\n[plugins]\ngit = {{ source = \"ElsiKora/Gate:git@0.1.0\" }}\n\n[[profiles]]\nprojects = [\"apps/web\"]\n\n[profiles.rules]\n\"git/commit-subject\" = {{ options-file = \"gate/web.json\" }}\n"
        ),
    )
    .write("gate/web.json", "{}\n")
    .write(
        "apps/server/gate.toml",
        "[project]\nname = \"server\"\n\n[plugins]\nlint = { source = \"ElsiKora/Gate:lint@0.1.0\", reads = [\"docs\"] }\n",
    )
    .add();
    repo.commit("profiles");

    repo.write("gate/web.json", "{\"max\": 50}\n");
    let (ran, _) = changed_since_head(&repo);
    assert_eq!(
        ran.len(),
        3,
        "a profile's options file is read by the root: {ran:?}"
    );

    repo.git(&["checkout", "--", "gate/web.json"]);
    repo.write("docs/guide.md", "# Guide, revised\n");
    let (ran, skipped) = changed_since_head(&repo);
    assert_eq!(ran, ["server: input docs/guide.md changed"]);
    assert_eq!(skipped, ["@acme/shared: no change", "web: no change"]);
}

#[test]
fn a_symmetric_push_range_is_refused() {
    let repo = monorepo();
    let git = Git::discover(repo.path()).expect("git");
    let error = ChangeSet::push(&git, "HEAD...HEAD", None).expect_err("symmetric range");
    assert_eq!(error.kind(), gate_host::ErrorKind::Config);
    assert!(error.message().contains("write <base>..<tip>"), "{error}");
}

#[test]
fn a_moved_pin_in_the_lock_selects_every_project() {
    let repo = monorepo();
    repo.write("gate.lock", "[core]\nversion = \"0.1.0\"\n");
    repo.commit("lock");
    repo.git(&["checkout", "-q", "-b", "feature"]);
    repo.write("gate.lock", "[core]\nversion = \"0.1.1\"\n");
    repo.commit("move a pin");

    let git = Git::discover(repo.path()).expect("git");
    let changes = ChangeSet::since(&git, "main", &Source::WorkTree).expect("since");
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    assert!(workspace.inputs().everywhere.contains("gate.lock"));
    let selection = Selection::select(workspace.projects(), &changes, workspace.inputs());
    let (ran, skipped) = decisions(&selection);
    assert_eq!(ran.len(), 3, "{ran:?}");
    assert!(ran.iter().all(|line| line.contains("gate.lock")), "{ran:?}");
    assert_eq!(skipped, Vec::<String>::new());
}

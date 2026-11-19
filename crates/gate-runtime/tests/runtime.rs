//! The runtime's sandbox, limits, read set and cache, against the probe plugin in `tests/probe`,
//! built for `wasm32-wasip2` once per process.

use std::collections::BTreeMap;
use std::sync::Arc;

#[path = "support/component.rs"]
mod built;

use gate_runtime::{
    Blob, Call, CallOutcome, Config, ErrorKind, GitQuery, Grant, History, HostError, Plugin,
    ReadSet, Runtime, Tree,
};
use gate_sdk::{CheckRequest, Commit, CommitRange, Location, RuleRun, SyncState};

/// The probe component, built once.
fn probe() -> &'static [u8] {
    &built::component("gate-runtime-probe", "gate_runtime_probe", "").1
}

/// The probe built with cargo `feature`, in a target directory of its own, once.
fn probe_with(feature: &str) -> &'static [u8] {
    &built::component("gate-runtime-probe", "gate_runtime_probe", feature).1
}

/// A snapshot from a list of (path, contents).
struct Files {
    paths: Vec<String>,
    files: BTreeMap<String, Vec<u8>>,
}

impl Files {
    fn new(files: &[(&str, &str)]) -> Arc<Self> {
        let files: BTreeMap<String, Vec<u8>> = files
            .iter()
            .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec()))
            .collect();
        Arc::new(Self {
            paths: files.keys().cloned().collect(),
            files,
        })
    }
}

impl Tree for Files {
    fn paths(&self) -> &[String] {
        &self.paths
    }

    fn read(&self, path: &str) -> Result<Option<Blob>, HostError> {
        Ok(self.files.get(path).map(|bytes| Blob {
            id: format!("blob-of-{path}"),
            bytes: bytes.clone(),
        }))
    }

    fn changed(&self) -> Option<&[String]> {
        None
    }
}

/// The id of the one commit of [`OneCommit`].
const HEAD: &str = "c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00";

/// A history with one commit, which changed `apps/api/a.ts` and `apps/web/b.ts`; every path reads
/// as its own name there, except a path that ends in `absent.ts`, which names no file.
struct OneCommit;

impl History for OneCommit {
    fn head(&self) -> Result<Option<String>, HostError> {
        Ok(Some(HEAD.to_owned()))
    }
    fn branch(&self) -> Result<Option<String>, HostError> {
        Ok(None)
    }
    fn range(&self) -> Result<Option<CommitRange>, HostError> {
        Ok(Some(CommitRange {
            base: None,
            head: HEAD.to_owned(),
            target_ref: Some("refs/heads/main".to_owned()),
        }))
    }
    fn resolve(&self, rev: &str) -> Result<Option<String>, HostError> {
        Ok((rev == "main").then(|| HEAD.to_owned()))
    }
    fn tree(&self, _: &str) -> Result<String, HostError> {
        Ok("7ree".to_owned())
    }
    fn merge_tree(&self, _: &str, _: &str) -> Result<Option<String>, HostError> {
        Ok(None)
    }
    fn revisions(&self, _: &str) -> Result<Vec<String>, HostError> {
        Ok(vec![HEAD.to_owned()])
    }
    fn log(&self, _: &str) -> Result<Vec<Commit>, HostError> {
        Ok(vec![Commit {
            id: HEAD.to_owned(),
            parents: Vec::new(),
            message: "the only commit\n".to_owned(),
            author_date: "1970-01-01T00:00:00+00:00".to_owned(),
        }])
    }
    fn changed_paths(&self, _: &str, _: &str) -> Result<Vec<String>, HostError> {
        Ok(vec!["apps/api/a.ts".to_owned(), "apps/web/b.ts".to_owned()])
    }
    fn read_at(&self, _: &str, path: &str) -> Result<Option<Vec<u8>>, HostError> {
        Ok((!path.ends_with("absent.ts")).then(|| path.as_bytes().to_vec()))
    }
    fn is_ancestor(&self, _: &str, _: &str) -> Result<bool, HostError> {
        Ok(false)
    }
    fn sync_state(&self, _: &str) -> Result<SyncState, HostError> {
        Ok(SyncState {
            produced: vec![
                ".githooks/pre-commit".to_owned(),
                "apps/api/gen/a.md".to_owned(),
                "apps/api/gen/c.md".to_owned(),
                "apps/web/gen/b.md".to_owned(),
            ],
            drift: vec![
                "apps/api/gen/a.md".to_owned(),
                "apps/web/gen/b.md".to_owned(),
            ],
        })
    }
    fn message(&self) -> Result<Option<String>, HostError> {
        Ok(Some("feat(core): add the thing\n".to_owned()))
    }
}

/// [`OneCommit`] whose `head` takes as long as the field says, as git on a cold cache may.
struct SlowHead(std::time::Duration);

impl History for SlowHead {
    fn head(&self) -> Result<Option<String>, HostError> {
        std::thread::sleep(self.0);
        OneCommit.head()
    }
    fn branch(&self) -> Result<Option<String>, HostError> {
        OneCommit.branch()
    }
    fn range(&self) -> Result<Option<CommitRange>, HostError> {
        OneCommit.range()
    }
    fn resolve(&self, rev: &str) -> Result<Option<String>, HostError> {
        OneCommit.resolve(rev)
    }
    fn tree(&self, commit: &str) -> Result<String, HostError> {
        OneCommit.tree(commit)
    }
    fn merge_tree(&self, first: &str, second: &str) -> Result<Option<String>, HostError> {
        OneCommit.merge_tree(first, second)
    }
    fn revisions(&self, range: &str) -> Result<Vec<String>, HostError> {
        OneCommit.revisions(range)
    }
    fn log(&self, range: &str) -> Result<Vec<Commit>, HostError> {
        OneCommit.log(range)
    }
    fn changed_paths(&self, parent: &str, commit: &str) -> Result<Vec<String>, HostError> {
        OneCommit.changed_paths(parent, commit)
    }
    fn read_at(&self, commit: &str, path: &str) -> Result<Option<Vec<u8>>, HostError> {
        OneCommit.read_at(commit, path)
    }
    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool, HostError> {
        OneCommit.is_ancestor(ancestor, descendant)
    }
    fn sync_state(&self, commit: &str) -> Result<SyncState, HostError> {
        OneCommit.sync_state(commit)
    }
}

fn runtime(config: Config) -> Runtime {
    Runtime::new(config).expect("the runtime starts")
}

fn load(runtime: &Runtime) -> Plugin {
    runtime.load("probe", probe()).expect("the probe loads")
}

fn call<'p>(
    plugin: &'p Plugin,
    rule: &str,
    options: &str,
    tree: Arc<dyn Tree>,
    grant: Grant,
) -> Call<'p> {
    Call {
        plugin,
        request: CheckRequest {
            rules: vec![RuleRun {
                name: rule.to_owned(),
                options: options.to_owned(),
                ..RuleRun::default()
            }],
            ..CheckRequest::default()
        },
        tree,
        history: Some(Arc::new(OneCommit)),
        grant,
    }
}

fn locations(outcome: &CallOutcome) -> Vec<Option<String>> {
    outcome
        .findings
        .iter()
        .map(|finding| {
            finding
                .location
                .as_ref()
                .and_then(Location::path)
                .map(str::to_owned)
        })
        .collect()
}

fn messages(outcome: &CallOutcome) -> Vec<String> {
    outcome
        .findings
        .iter()
        .map(|finding| finding.message.clone())
        .collect()
}

fn tree() -> Arc<Files> {
    Files::new(&[
        ("apps/api/src/main.ts", "main"),
        ("apps/web/src/secret.ts", "web"),
        ("docs/ledger/a.md", "ledger"),
        ("docs/other.md", "other"),
    ])
}

#[test]
fn ungranted_paths_read_as_none_and_the_read_set_is_what_was_read() {
    let runtime = runtime(Config::default());
    let plugin = load(&runtime);
    let grant = Grant::new("apps/api", &["docs/ledger".to_owned()]);
    let options = r#"{"paths": ["src/main.ts", "src/absent.ts", "../web/src/secret.ts", "../../docs/ledger/a.md", "../../docs/other.md", "../../../outside", "src/../../../docs/ledger/a.md"], "list": "../../docs"}"#;
    let outcome = runtime
        .check(&call(&plugin, "read", options, tree(), grant))
        .expect("the call runs");

    assert_eq!(
        messages(&outcome),
        [
            "src/main.ts: main",
            "src/absent.ts: none",
            "../web/src/secret.ts: none",
            "../../docs/ledger/a.md: ledger",
            "../../docs/other.md: none",
            "../../../outside: none",
            "src/../../../docs/ledger/a.md: ledger",
            "list ../../docs: ../../docs/ledger/a.md",
        ]
    );
    let mut expected = ReadSet::default();
    expected.listed.insert("docs/ledger".to_owned());
    expected.read.insert(
        "apps/api/src/main.ts".to_owned(),
        Some("blob-of-apps/api/src/main.ts".to_owned()),
    );
    expected
        .read
        .insert("apps/api/src/absent.ts".to_owned(), None);
    expected.read.insert(
        "docs/ledger/a.md".to_owned(),
        Some("blob-of-docs/ledger/a.md".to_owned()),
    );
    assert_eq!(outcome.read_set, expected);
}

#[test]
fn finding_locations_are_repository_paths() {
    let runtime = runtime(Config::default());
    let plugin = load(&runtime);
    let grant = || Grant::new("apps/api", &["docs".to_owned()]);
    let at = |path: &str| {
        let options = format!(r#"{{"paths": ["src/main.ts"], "at": "{path}"}}"#);
        runtime.check(&call(&plugin, "read", &options, tree(), grant()))
    };

    let outcome = at("../../docs/x.md").expect("the call runs");
    assert_eq!(
        locations(&outcome),
        [
            Some("apps/api/probe".to_owned()),
            Some("docs/x.md".to_owned())
        ]
    );
    let outcome = at("src/./main.ts").expect("the call runs");
    assert_eq!(
        locations(&outcome)[1].as_deref(),
        Some("apps/api/src/main.ts")
    );

    let error = at("../../../x.md").expect_err("a location outside the repository fails");
    assert_eq!(error.kind(), ErrorKind::Plugin);
    assert_eq!(error.plugin(), "probe");
    assert!(
        error.to_string().contains("outside the repository"),
        "{error}"
    );
}

#[test]
fn a_rule_without_reads_history_cannot_call_git() {
    let runtime = runtime(Config::default());
    let plugin = load(&runtime);
    let error = runtime
        .check(&call(&plugin, "sneak", "", tree(), Grant::repository()))
        .expect_err("git is refused");
    assert_eq!(error.kind(), ErrorKind::Trap);
    assert_eq!(error.plugin(), "probe");
    assert!(
        error
            .to_string()
            .contains("no rule of this call declares reads-history"),
        "{error}"
    );
}

#[test]
fn a_rule_with_reads_history_gets_git_and_its_queries_are_recorded() {
    let runtime = runtime(Config::default());
    let plugin = load(&runtime);
    let outcome = runtime
        .check(&call(&plugin, "history", "", tree(), Grant::repository()))
        .expect("the call runs");
    assert_eq!(
        messages(&outcome),
        [
            format!("head: {HEAD}"),
            format!("main: {HEAD}"),
            "tree: 7ree".to_owned(),
            "log: 1".to_owned(),
            format!("range: none..{HEAD} to refs/heads/main")
        ]
    );
    assert!(
        outcome
            .findings
            .iter()
            .all(|finding| finding.location == Some(Location::commit(HEAD))),
        "a commit location passes through as the full id"
    );
    let ids = |query: GitQuery| outcome.read_set.git.get(&query).cloned();
    let one = || Some(vec![HEAD.to_owned()]);
    assert_eq!(ids(GitQuery::Head), one(), "head records the id it named");
    assert_eq!(
        ids(GitQuery::Resolve {
            rev: "main".to_owned()
        }),
        one(),
        "resolve records the id the ref named"
    );
    assert_eq!(
        ids(GitQuery::Log {
            range: "HEAD".to_owned()
        }),
        one(),
        "log records the ids its range named"
    );
    assert_eq!(
        ids(GitQuery::Range),
        Some(vec![HEAD.to_owned(), "refs/heads/main".to_owned()]),
        "range records its ids and the ref it pushes to"
    );
    assert_eq!(
        ids(GitQuery::Tree {
            commit: HEAD.to_owned()
        }),
        Some(Vec::new()),
        "an answer about a commit id records nothing to re-check"
    );
}

#[test]
fn fuel_used_is_counted_per_invocation_and_the_same_on_every_run() {
    let runtime = runtime(Config::default());
    let plugin = load(&runtime);
    let mut request = call(
        &plugin,
        "read",
        r#"{"paths": ["src/main.ts"]}"#,
        tree(),
        Grant::repository(),
    );
    request.request.rules.push(RuleRun {
        name: "history".to_owned(),
        ..RuleRun::default()
    });
    let first = runtime.check(&request).expect("the call runs");
    let again = runtime.check(&request).expect("the call runs again");

    let rules: Vec<&[String]> = first
        .invocations
        .iter()
        .map(|run| run.rules.as_slice())
        .collect();
    assert_eq!(
        rules,
        [["read".to_owned()], ["history".to_owned()]],
        "snapshot, then history"
    );
    assert!(first.invocations.iter().all(|run| run.fuel_used > 0));
    assert_eq!(
        first.fuel_used,
        first
            .invocations
            .iter()
            .map(|run| run.fuel_used)
            .sum::<u64>()
    );
    assert!(first.fuel_used < Config::default().fuel);
    assert_eq!(
        first.fuel_used, again.fuel_used,
        "fuel does not depend on the machine's load"
    );
}

#[test]
fn a_trap_fails_with_the_plugin_name_and_its_panic() {
    let runtime = runtime(Config::default());
    let plugin = load(&runtime);
    let error = runtime
        .check(&call(&plugin, "panic", "", tree(), Grant::repository()))
        .expect_err("it traps");
    assert_eq!(error.kind(), ErrorKind::Trap);
    let message = error.to_string();
    assert!(message.starts_with("plugin probe: trapped"), "{message}");
    assert!(message.contains("the probe panics on purpose"), "{message}");
}

#[test]
fn a_call_that_never_ends_runs_out_of_fuel() {
    let runtime = runtime(Config {
        fuel: 50_000_000,
        ..Config::default()
    });
    let plugin = load(&runtime);
    let error = runtime
        .check(&call(&plugin, "spin", "", tree(), Grant::repository()))
        .expect_err("it stops");
    assert_eq!(error.kind(), ErrorKind::OutOfFuel);
    assert!(
        error
            .to_string()
            .starts_with("plugin probe: hit the fuel limit of 50000000 units"),
        "{error}"
    );
}

#[test]
fn a_call_past_the_deadline_fails_naming_the_plugin_and_the_call() {
    let runtime = runtime(Config {
        fuel: u64::MAX,
        deadline: std::time::Duration::from_secs(2),
        ..Config::default()
    });
    let plugin = load(&runtime);
    let started = std::time::Instant::now();
    let error = runtime
        .check(&call(&plugin, "spin", "", tree(), Grant::repository()))
        .expect_err("it stops");
    assert_eq!(error.kind(), ErrorKind::Deadline);
    assert!(
        error
            .to_string()
            .starts_with("plugin probe: the call check spin ran past the deadline of 2 s"),
        "{error}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(60),
        "stopped near the deadline, not at the fuel limit: {:?}",
        started.elapsed()
    );
}

#[test]
fn time_in_a_host_call_does_not_count_against_the_deadline() {
    let runtime = runtime(Config {
        deadline: std::time::Duration::from_secs(2),
        ..Config::default()
    });
    let plugin = load(&runtime);
    let mut slow = call(&plugin, "history", "", tree(), Grant::repository());
    slow.history = Some(Arc::new(SlowHead(std::time::Duration::from_secs(5))));
    let outcome = runtime
        .check(&slow)
        .expect("five seconds in git.head are the host's, not the plugin's");
    assert!(
        messages(&outcome)
            .iter()
            .any(|message| message.starts_with("head: ")),
        "{:?}",
        messages(&outcome)
    );
}

#[test]
fn a_describe_that_never_ends_fails_to_load_out_of_fuel_on_every_run() {
    let runtime = runtime(Config {
        fuel: 50_000_000,
        ..Config::default()
    });
    let spinning = probe_with("describe-spins");
    for _ in 0..2 {
        let error = runtime
            .load("other", spinning)
            .expect_err("describe never ends");
        assert_eq!(error.kind(), ErrorKind::OutOfFuel, "{error}");
    }
}

#[test]
fn a_component_built_for_another_interface_fails_to_load_naming_the_interface() {
    let runtime = runtime(Config::default());
    // An empty component: valid WebAssembly, but without the exports of this core's world, as a
    // component built against another version of the interface lacks them or types them apart.
    let empty = b"\0asm\x0d\x00\x01\x00";
    let error = runtime
        .load("hello", empty)
        .expect_err("it has no describe");
    assert_eq!(error.kind(), ErrorKind::Load, "{error}");
    let version = env!("CARGO_PKG_VERSION");
    assert!(
        error.to_string().contains(&format!(
            "hello was built for another plugin interface; this gate {version} loads plugins built for gate:plugin@0.3.0 only: pin the plugin's release built for it (the plugins of ElsiKora/Gate: version {version}, released with this core), or rebuild it with gate-sdk {version}"
        )),
        "{error}"
    );
    // The interface the error names is the one the core is built against.
    let wit = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../wit/gate.wit"))
        .expect("the interface");
    assert!(
        wit.contains(&format!("\npackage {};\n", gate_runtime::INTERFACE)),
        "{}",
        gate_runtime::INTERFACE
    );

    let error = runtime
        .load("hello", b"not a component")
        .expect_err("not WebAssembly");
    assert!(
        error.to_string().contains("cannot load the component"),
        "{error}"
    );
}

#[test]
fn a_call_past_the_memory_limit_fails_naming_the_limit() {
    let runtime = runtime(Config {
        memory_bytes: 64 << 20,
        ..Config::default()
    });
    let plugin = load(&runtime);
    let error = runtime
        .check(&call(&plugin, "hog", "", tree(), Grant::repository()))
        .expect_err("it stops");
    assert_eq!(error.kind(), ErrorKind::OutOfMemory);
    assert!(
        error
            .to_string()
            .starts_with("plugin probe: hit the memory limit of 67108864 bytes"),
        "{error}"
    );

    let runtime = self::runtime(Config::default());
    let plugin = load(&runtime);
    let outcome = runtime
        .check(&call(&plugin, "hog", "", tree(), Grant::repository()))
        .expect("the default memory limit allows 256 MiB");
    assert_eq!(messages(&outcome), ["allocated 268435456"]);
}

#[test]
fn ambient_answers_are_fixed_and_output_is_logged() {
    let runtime = runtime(Config::default());
    let plugin = load(&runtime);
    let run = || {
        runtime
            .check(&call(&plugin, "ambient", "", tree(), Grant::repository()))
            .expect("the call runs")
    };
    let first = run();
    let second = run();
    assert_eq!(messages(&first), messages(&second));
    assert_eq!(&messages(&first)[..3], ["env: 0", "args: 0", "wall: 0"]);
    assert_eq!(messages(&first)[4], "fs: false");
    assert_eq!(
        first.log,
        ["probe log", "stdout: probe stdout", "stderr: probe stderr"]
    );
}

#[test]
fn the_module_cache_is_hit_on_the_second_run() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let config = Config {
        module_cache: Some(directory.path().join("modules")),
        ..Config::default()
    };
    let first = runtime(config.clone());
    load(&first);
    assert_eq!(first.cache_counts(), (0, 1), "the first run compiles");
    let entries: Vec<_> = std::fs::read_dir(directory.path())
        .expect("the cache root exists")
        .map(|entry| entry.expect("an entry").file_name())
        .collect();
    assert_eq!(
        entries,
        ["modules"],
        "modules are cached in the given directory only"
    );
    assert!(
        std::fs::read_dir(directory.path().join("modules"))
            .expect("the module cache exists")
            .next()
            .is_some(),
        "the compiled module is in the module cache"
    );
    let second = runtime(config);
    load(&second);
    assert_eq!(
        second.cache_counts(),
        (1, 0),
        "the second run loads the compiled module"
    );
}

#[test]
fn calls_on_the_worker_pool_answer_in_order() {
    let runtime = runtime(Config {
        workers: 4,
        ..Config::default()
    });
    let plugin = load(&runtime);
    let tree: Arc<dyn Tree> = tree();
    let calls: Vec<Call<'_>> = ["apps/api", "apps/web", "docs/ledger", "docs"]
        .iter()
        .map(|root| {
            call(
                &plugin,
                "read",
                r#"{"list": ""}"#,
                Arc::clone(&tree),
                Grant::new(root, &[]),
            )
        })
        .collect();
    let lists: Vec<Vec<String>> = runtime
        .check_all(&calls)
        .into_iter()
        .map(|outcome| messages(&outcome.expect("the call runs")))
        .collect();
    assert_eq!(
        lists,
        [
            vec!["list : src/main.ts".to_owned()],
            vec!["list : src/secret.ts".to_owned()],
            vec!["list : a.md".to_owned()],
            vec!["list : ledger/a.md,other.md".to_owned()],
        ]
    );
}

#[test]
fn generated_paths_and_owned_globs_are_mapped_to_the_repository() {
    let runtime = runtime(Config::default());
    let plugin = load(&runtime);
    assert_eq!(
        plugin.info().generators[0].owns,
        ["gen/**", "GENERATED.md"],
        "describe carries the project-relative globs"
    );
    let grant = Grant::new("apps/api", &[]);
    assert_eq!(
        plugin.owned("write", &grant),
        Ok(Some(vec![
            "apps/api/gen/**".to_owned(),
            "apps/api/GENERATED.md".to_owned()
        ]))
    );
    assert_eq!(
        plugin.owned("write", &Grant::repository()),
        Ok(Some(vec!["gen/**".to_owned(), "GENERATED.md".to_owned()]))
    );
    assert_eq!(plugin.owned("other", &grant), Ok(None));

    let (output, _, _) = runtime
        .generate(
            &plugin,
            tree(),
            &grant,
            "write",
            r#"{"files": {"gen/a.md": "a", "./GENERATED.md": "b"}, "seeds": {"docs/README.md": "c"}, "echo_plugin_options": true}"#,
            r#"{"kind": "service"}"#,
        )
        .expect("the generator runs");
    let files: Vec<(&str, &[u8], bool)> = output
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.contents.as_slice(), file.seed))
        .collect();
    assert_eq!(
        files,
        [
            ("apps/api/GENERATED.md", &b"b"[..], false),
            ("apps/api/gen/a.md", &b"a"[..], false),
            ("apps/api/docs/README.md", &b"c"[..], true),
            (
                "apps/api/plugin-options.json",
                &br#"{"kind":"service"}"#[..],
                false
            ),
        ]
    );

    for outside in ["../web/x.md", "/etc/x", "..", "gen\\a.md"] {
        let error = runtime
            .generate(
                &plugin,
                tree(),
                &grant,
                "write",
                &format!(r#"{{"files": {{{outside:?}: "x"}}}}"#),
                "",
            )
            .expect_err("a file outside the project fails");
        assert_eq!(error.kind(), ErrorKind::Plugin, "{error}");
        assert!(
            error
                .to_string()
                .contains("not a path below the project root"),
            "{error}"
        );
    }
}

#[test]
fn generators_never_write_or_own_the_cores_files() {
    let runtime = runtime(Config::default());
    let plugin = load(&runtime);
    let write = |grant: &Grant, path: &str| {
        runtime.generate(
            &plugin,
            tree(),
            grant,
            "write",
            &format!(r#"{{"files": {{{path:?}: "x"}}}}"#),
            "",
        )
    };
    let project = Grant::new("apps/api", &[]);
    for (grant, path) in [
        (&Grant::repository(), ".git/config"),
        (&Grant::repository(), "gate.toml"),
        (&Grant::repository(), "gate.lock"),
        (&Grant::repository(), "gate.exceptions.toml"),
        (&Grant::repository(), "bin/gate"),
        (&Grant::repository(), ".githooks/pre-commit"),
        (&project, ".GIT/hooks/pre-commit"),
        (&project, "Gate.toml"),
    ] {
        let error = write(grant, path).expect_err("a file the core owns is refused");
        assert_eq!(error.kind(), ErrorKind::Plugin, "{path}: {error}");
        assert!(
            error.to_string().contains("a file the core owns"),
            "{path}: {error}"
        );
    }
    write(&project, "bin/gate").expect("a project's own bin/gate is not the launcher");

    let error = plugin
        .owned("toml", &project)
        .expect_err("a glob over the project's gate.toml is refused");
    assert_eq!(error.kind(), ErrorKind::Load, "{error}");
    assert!(
        error.to_string().contains(r#"covers "apps/api/gate.toml""#),
        "{error}"
    );
}

#[test]
fn log_and_output_past_the_limit_are_dropped_with_one_note_and_the_call_goes_on() {
    let runtime = runtime(Config::default());
    let plugin = load(&runtime);
    let outcome = runtime
        .check(&call(
            &plugin,
            "flood",
            r#"{"log": 2048, "stdout": 2048}"#,
            tree(),
            Grant::repository(),
        ))
        .expect("the call goes on past the limits");
    assert_eq!(messages(&outcome), ["done"]);
    let notes: Vec<&String> = outcome
        .log
        .iter()
        .filter(|line| line.starts_with("gate: "))
        .collect();
    assert_eq!(
        notes,
        [
            "gate: the plugin logged more than 1048576 bytes in this call; the rest is dropped",
            "gate: the plugin wrote more than 1048576 bytes to stdout in this call; the rest is dropped",
        ]
    );
    let logged: usize = outcome
        .log
        .iter()
        .filter(|line| !line.starts_with("gate: ") && !line.starts_with("stdout: "))
        .map(|line| line.len() + 1)
        .sum();
    assert!(logged <= 1 << 20, "{logged} bytes of log kept");
    let printed: usize = outcome
        .log
        .iter()
        .filter_map(|line| line.strip_prefix("stdout: "))
        .map(|line| line.len() + 1)
        .sum();
    assert_eq!(printed, 1 << 20, "the first MiB of stdout is kept");
}

#[test]
fn git_takes_full_commit_ids_and_answers_only_granted_paths() {
    let runtime = runtime(Config::default());
    let plugin = load(&runtime);
    let grant = || Grant::new("apps/api", &[]);
    let options = r#"{"read_at": [["main", "a.ts"], ["main", "../web/b.ts"], ["main", "/apps/web/b.ts"], ["main", "absent.ts"]], "changed_paths": [["main", "main"]], "sync_state": ["main"]}"#;
    let outcome = runtime
        .check(&call(&plugin, "git", options, tree(), grant()))
        .expect("the call runs");
    assert_eq!(
        messages(&outcome),
        [
            "read-at main:a.ts: apps/api/a.ts",
            "read-at main:../web/b.ts: none",
            "read-at main:/apps/web/b.ts: none",
            "read-at main:absent.ts: none",
            "changed-paths main main: a.ts",
            "sync-state main: produced gen/a.md,gen/c.md; drift gen/a.md",
        ]
    );
    assert!(
        outcome.read_set.git.contains_key(&GitQuery::SyncState {
            commit: HEAD.to_owned()
        }),
        "sync-state is recorded by its commit: {:?}",
        outcome.read_set.git
    );
    let read_at = |path: &str| {
        outcome
            .read_set
            .git
            .get(&GitQuery::ReadAt {
                commit: HEAD.to_owned(),
                path: path.to_owned(),
            })
            .cloned()
    };
    assert_eq!(
        read_at("apps/api/a.ts"),
        Some(Vec::new()),
        "read-at records the repository path, and a file found as nothing to ask again: {:?}",
        outcome.read_set.git
    );
    assert_eq!(
        read_at("apps/api/absent.ts"),
        Some(vec!["none".to_owned()]),
        "no file is recorded as none, to ask again: a fetch can bring the commit"
    );

    let upper = HEAD.to_uppercase();
    for raw in ["main", "c0ffee", upper.as_str()] {
        let options = format!(r#"{{"raw_tree": {raw:?}}}"#);
        let error = runtime
            .check(&call(&plugin, "git", &options, tree(), grant()))
            .expect_err("a revision is not a commit id");
        assert_eq!(error.kind(), ErrorKind::Plugin, "{error}");
        assert!(error.to_string().contains("resolve a revision"), "{error}");
    }
    let error = runtime
        .check(&call(
            &plugin,
            "git",
            r#"{"sync_state": ["c0ffee"]}"#,
            tree(),
            grant(),
        ))
        .expect_err("sync-state takes a full id");
    assert_eq!(error.kind(), ErrorKind::Plugin, "{error}");
}

#[test]
fn a_finding_of_an_undeclared_aspect_or_a_short_commit_fails_the_call() {
    let runtime = runtime(Config::default());
    let plugin = load(&runtime);
    let check =
        |options: &str| runtime.check(&call(&plugin, "read", options, tree(), Grant::repository()));
    let error = check(r#"{"at": "x", "aspect": "nope"}"#).expect_err("the aspect is not declared");
    assert_eq!(error.kind(), ErrorKind::Plugin, "{error}");
    assert!(error.to_string().contains("aspect \"nope\""), "{error}");

    let error = check(r#"{"commit": "c0ffee"}"#).expect_err("a short id is not a location");
    assert_eq!(error.kind(), ErrorKind::Plugin, "{error}");
    assert!(
        error.to_string().contains("not a full commit id"),
        "{error}"
    );
    check(&format!(r#"{{"commit": "{HEAD}"}}"#)).expect("a full id is a location");
}

#[test]
fn a_file_added_under_an_extra_read_is_under_a_listed_prefix() {
    let runtime = runtime(Config::default());
    let plugin = load(&runtime);
    let grant = || Grant::new("apps/api", &["docs/ledger".to_owned()]);
    let list = |tree: Arc<Files>| {
        runtime
            .check(&call(&plugin, "read", r#"{"list": ""}"#, tree, grant()))
            .expect("the call runs")
    };
    let before = list(tree());
    assert_eq!(
        before.read_set.listed.iter().collect::<Vec<_>>(),
        ["apps/api/", "docs/ledger"]
    );
    let after = list(Files::new(&[
        ("apps/api/src/main.ts", "main"),
        ("docs/ledger/a.md", "ledger"),
        ("docs/ledger/b.md", "added"),
    ]));
    assert_ne!(messages(&before), messages(&after), "the listing changed");
    assert!(
        before
            .read_set
            .listed
            .iter()
            .any(|prefix| "docs/ledger/b.md".starts_with(prefix.as_str())),
        "the added file is under a recorded prefix, so the cached result is re-run"
    );
}

#[test]
fn a_rule_that_reads_the_message_without_declaring_it_gets_the_error_naming_reads_message() {
    let runtime = runtime(Config::default());
    let plugin = load(&runtime);
    let outcome = runtime
        .check(&call(
            &plugin,
            "message-undeclared",
            "",
            tree(),
            Grant::repository(),
        ))
        .expect("the call runs");
    let said = messages(&outcome);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].contains("does not declare READS_MESSAGE") && !said[0].contains("add the thing"),
        "the SDK refuses the message to a rule that does not declare it: {said:?}"
    );
}

#[test]
fn a_project_scope_rule_that_declares_the_message_does_not_load() {
    let runtime = runtime(Config::default());
    let component = probe_with("message-in-project");
    let error = runtime
        .load("other", &component)
        .expect_err("the message belongs to the repository");
    assert_eq!(error.kind(), ErrorKind::Load, "{error}");
    assert!(
        error.to_string().contains("project-message")
            && error.to_string().contains("reads-message")
            && error.to_string().contains("repository-scope"),
        "{error}"
    );
}

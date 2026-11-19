//! Tools pinned by sha256, fetched through the verified fetch: a download is stored only when it
//! matches, a changed cache entry or unpacked executable is replaced by the verified one, a
//! machine without a target is told what to add, and `gate cache prune` keeps what gate.toml pins.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

use gate_host::cache::{CACHE_TAG, Cache, Sha256Sum};
use gate_host::config::{Name, Tool, ToolTarget};
use gate_host::fetch::{Endpoint, FetchConfig, Fetcher};
use gate_host::lock::{self, Pinned};
use gate_host::run::{self, RunRequest};
use gate_host::tools::{directory, install_for};
use gate_host::{Source, Workspace};

use crate::fixture::{ROOT, Repo};
use crate::http::{Route, Server, sha256};

const PLATFORM: &str = "x86_64-unknown-linux-gnu";
const SCRIPT: &[u8] = b"#!/bin/sh\necho formatted\n";
const ROUTE: &str = "/tools/fmt-1.0.0";

fn fetcher(server: &Server, cache: &Path) -> Fetcher {
    Fetcher::new(
        FetchConfig {
            github: Endpoint {
                web_url: server.url(""),
                api_url: server.url(""),
            },
            enterprise: None,
            tokens: Vec::new(),
            gh: None,
        },
        Cache::at(cache),
    )
}

fn tools(url: &str, sha256: &str) -> BTreeMap<Name, Tool> {
    let mut targets = BTreeMap::new();
    targets.insert(
        PLATFORM.to_owned(),
        ToolTarget {
            url: url.to_owned(),
            sha256: sha256.to_owned(),
            bin: None,
        },
    );
    let mut tools = BTreeMap::new();
    tools.insert(
        Name::try_from("fmt".to_owned()).expect("name"),
        Tool {
            version: "1.0.0".to_owned(),
            targets,
        },
    );
    tools
}

fn platform() -> Vec<String> {
    vec![PLATFORM.to_owned()]
}

#[test]
fn a_matching_download_is_installed_once_and_then_served_from_the_cache() {
    let cache = tempfile::tempdir().expect("cache");
    let server = Server::start();
    server.serve(ROUTE, SCRIPT);
    let pinned = tools(&server.url(ROUTE), &sha256(SCRIPT));

    let installed = install_for(&pinned, &fetcher(&server, cache.path()), &platform())
        .expect("install")
        .expect("tools");
    assert_eq!(server.requests().len(), 1);
    let entry = installed.bin.join("fmt");
    assert_eq!(std::fs::read(&entry).expect("entry"), SCRIPT);
    let output = std::process::Command::new(&entry).output().expect("run");
    assert_eq!(output.stdout, b"formatted\n");

    let again = install_for(&pinned, &fetcher(&server, cache.path()), &platform())
        .expect("from the cache")
        .expect("tools");
    assert_eq!(again, installed);
    assert_eq!(
        server.requests().len(),
        1,
        "the second run downloads nothing"
    );
}

#[test]
fn a_download_with_another_checksum_is_refused_and_not_stored() {
    let cache = tempfile::tempdir().expect("cache");
    let server = Server::start();
    server.serve(ROUTE, SCRIPT);
    let pinned = sha256(b"the release that was pinned");
    let error = install_for(
        &tools(&server.url(ROUTE), &pinned),
        &fetcher(&server, cache.path()),
        &platform(),
    )
    .expect_err("checksum mismatch");

    assert!(
        error.message().starts_with(&format!(
            "gate.toml: [tools.fmt] {PLATFORM}: checksum mismatch for the tool fmt 1.0.0 from {}: gate.toml [tools.fmt] pins sha256 {pinned}, the download has sha256 {}; nothing entered the cache",
            server.url(ROUTE),
            sha256(SCRIPT)
        )),
        "{}",
        error.message()
    );
    let cache = Cache::at(cache.path());
    for sum in [&pinned, &sha256(SCRIPT)] {
        assert!(!cache.blob(&Sha256Sum::parse(sum).expect("sum")).exists());
    }
    assert!(!directory(&cache).join(&pinned).exists());
}

#[test]
fn a_changed_download_in_the_cache_is_fetched_again_and_verified() {
    let cache = tempfile::tempdir().expect("cache");
    let server = Server::start();
    server.serve(ROUTE, SCRIPT);
    let sum = sha256(SCRIPT);
    let pinned = tools(&server.url(ROUTE), &sum);
    install_for(&pinned, &fetcher(&server, cache.path()), &platform()).expect("install");

    let blob = Cache::at(cache.path()).blob(&Sha256Sum::parse(&sum).expect("sum"));
    std::fs::write(&blob, b"#!/bin/sh\necho tampered\n").expect("tamper");
    let executable = directory(&Cache::at(cache.path()))
        .join(&sum)
        .join("files")
        .join("fmt");
    std::fs::remove_file(&executable).expect("remove the unpacked file");
    install_for(&pinned, &fetcher(&server, cache.path()), &platform()).expect("again");
    assert_eq!(
        server.requests().len(),
        2,
        "the changed entry is downloaded again"
    );
    assert_eq!(std::fs::read(&blob).expect("blob"), SCRIPT);
    assert_eq!(std::fs::read(&executable).expect("executable"), SCRIPT);
}

#[test]
fn a_changed_unpacked_executable_or_link_is_replaced_by_the_verified_one() {
    let cache = tempfile::tempdir().expect("cache");
    let server = Server::start();
    server.serve(ROUTE, SCRIPT);
    let sum = sha256(SCRIPT);
    let pinned = tools(&server.url(ROUTE), &sum);
    let installed = install_for(&pinned, &fetcher(&server, cache.path()), &platform())
        .expect("install")
        .expect("tools");
    let executable = installed.tools["fmt"].clone();

    std::fs::write(&executable, b"#!/bin/sh\necho tampered\n").expect("tamper");
    let again = install_for(&pinned, &fetcher(&server, cache.path()), &platform())
        .expect("again")
        .expect("tools");
    assert_eq!(again, installed);
    assert_eq!(std::fs::read(&executable).expect("executable"), SCRIPT);
    assert_eq!(
        server.requests().len(),
        1,
        "unpacked again from the verified download"
    );

    let link = installed.bin.join("fmt");
    std::fs::remove_file(&link).expect("remove the link");
    std::fs::write(&link, b"#!/bin/sh\necho tampered\n").expect("plant a file");
    install_for(&pinned, &fetcher(&server, cache.path()), &platform()).expect("rebuilt");
    let output = std::process::Command::new(&link).output().expect("run");
    assert_eq!(output.stdout, b"formatted\n");
}

#[test]
fn two_concurrent_first_installs_both_succeed_with_one_complete_entry() {
    let cache = tempfile::tempdir().expect("cache");
    let server = Server::start();
    server.route(ROUTE, Route::ok(SCRIPT).delayed(Duration::from_millis(300)));
    let pinned = tools(&server.url(ROUTE), &sha256(SCRIPT));

    let results: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let fetcher = fetcher(&server, cache.path());
                let pinned = &pinned;
                scope.spawn(move || install_for(pinned, &fetcher, &platform()))
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("thread"))
            .collect()
    });

    assert_eq!(
        server.requests().len(),
        2,
        "both fetched before either finished"
    );
    let mut installed = Vec::new();
    for result in results {
        let tools = result.expect("install").expect("tools");
        let output = std::process::Command::new(tools.bin.join("fmt"))
            .output()
            .expect("run");
        assert_eq!(output.stdout, b"formatted\n");
        installed.push(tools);
    }
    assert_eq!(installed[0], installed[1]);

    let tools = directory(&Cache::at(cache.path()));
    let mut entries: Vec<String> = std::fs::read_dir(&tools)
        .expect("tools")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    entries.sort();
    assert_eq!(
        entries,
        [sha256(SCRIPT), "bin".to_owned()],
        "no temporary is left"
    );
    let unpacked = tools.join(sha256(SCRIPT));
    assert_eq!(
        std::fs::read_to_string(unpacked.join("executable")).expect("record"),
        format!("fmt\n{}\n", sha256(SCRIPT))
    );
    assert_eq!(
        std::fs::read(unpacked.join("files/fmt")).expect("executable"),
        SCRIPT
    );
}

#[test]
fn a_machine_without_a_target_is_told_which_target_to_add() {
    let cache = tempfile::tempdir().expect("cache");
    let server = Server::start();
    let error = install_for(
        &tools(&server.url(ROUTE), &sha256(SCRIPT)),
        &fetcher(&server, cache.path()),
        &["aarch64-apple-darwin".to_owned()],
    )
    .expect_err("no target");
    assert_eq!(
        error.message(),
        "gate.toml: [tools.fmt] has no target for this machine (aarch64-apple-darwin); add [tools.fmt.targets.\"aarch64-apple-darwin\"]"
    );
}

#[test]
fn cache_prune_keeps_the_tools_gate_toml_pins_and_removes_the_others() {
    let cache_dir = tempfile::tempdir().expect("cache");
    let server = Server::start();
    server.serve(ROUTE, SCRIPT);
    let old = b"#!/bin/sh\necho old\n";
    server.serve("/tools/fmt-0.9.0", old.as_slice());
    let sum = sha256(SCRIPT);
    let fetcher = fetcher(&server, cache_dir.path());
    install_for(&tools(&server.url(ROUTE), &sum), &fetcher, &platform()).expect("install");
    install_for(
        &tools(&server.url("/tools/fmt-0.9.0"), &sha256(old)),
        &fetcher,
        &platform(),
    )
    .expect("install the old one");

    let repo = Repo::new();
    repo.write(
        "gate.toml",
        &format!(
            "{ROOT}\n[tools.fmt]\nversion = \"1.0.0\"\ntargets.{PLATFORM} = {{ url = \"{}\", sha256 = \"{sum}\" }}\n",
            server.url(ROUTE)
        ),
    );
    let mut pinned = Pinned::default();
    lock::pinned_in(repo.path(), &mut pinned).expect("pinned");
    assert_eq!(
        pinned.tools,
        BTreeSet::from([Sha256Sum::parse(&sum).expect("sum")])
    );

    let cache = Cache::at(cache_dir.path());
    assert!(cache.root().join(CACHE_TAG).is_file());
    // A run unpacking a tool right now, and one that died two hours ago.
    let tools = cache.root().join("tools");
    let unpacking = tools.join(format!(".{}.4242.0", "c".repeat(64)));
    let abandoned = tools.join(format!(".{}.4343.0", "d".repeat(64)));
    for directory in [&unpacking, &abandoned] {
        std::fs::create_dir_all(directory.join("files")).expect("temporary");
    }
    std::fs::File::open(&abandoned)
        .expect("open")
        .set_modified(std::time::SystemTime::now() - Duration::from_hours(2))
        .expect("age it");
    let removed: BTreeSet<String> = cache
        .unreferenced(&pinned.checksums, false)
        .expect("unreferenced")
        .into_iter()
        .map(|entry| {
            entry
                .path
                .strip_prefix(cache.root())
                .expect("inside")
                .display()
                .to_string()
        })
        .collect();
    let old_sum = sha256(old);
    assert_eq!(
        removed,
        BTreeSet::from([
            format!("blobs/sha256/{old_sum}"),
            format!("tools/{old_sum}"),
            format!("tools/.{}.4343.0", "d".repeat(64)),
            "tools/bin".to_owned(),
        ]),
        "a tool temporary younger than an hour stays"
    );
}

#[test]
fn a_run_installs_only_the_tools_of_the_steps_that_run_and_each_step_sees_its_own() {
    let cache = tempfile::tempdir().expect("cache");
    let server = Server::start();
    let format = b"#!/bin/sh\necho formatted\n";
    let lint = b"#!/bin/sh\necho linted\n";
    server.serve("/tools/fake-format", format.as_slice());
    server.serve("/tools/fake-lint", lint.as_slice());
    let target = |route: &str, sum: &str| {
        format!(
            "version = \"1.0.0\"\ntargets.{PLATFORM} = {{ url = \"{}\", sha256 = \"{sum}\" }}\n",
            server.url(route)
        )
    };
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        &format!(
            "{ROOT}\n[tools.fake-format]\n{}\n[tools.fake-lint]\n{}\n[tools.fake-unused]\n{}\n{}",
            target("/tools/fake-format", &sha256(format)),
            target("/tools/fake-lint", &sha256(lint)),
            // Not served: installing it would fail the run.
            target("/tools/fake-unused", &"a".repeat(64)),
            r#"
[steps]
formats = { run = "fake-format > formats.txt; command -v fake-lint >> formats.txt || echo no lint >> formats.txt", tools = ["fake-format"] }
plain = { run = "command -v fake-format > plain.txt || echo no format > plain.txt" }
both = { run = "fake-format > both.txt; fake-lint >> both.txt", tools = ["fake-lint", "fake-format"] }
skipped = { run = "fake-unused", files = "changed", glob = "*.none", tools = ["fake-unused"] }
broken = { run = "exit 3" }
after = { run = "fake-unused", tools = ["fake-unused"] }

[suites]
all = ["formats", "plain", "skipped", "both"]
stop = { steps = ["broken", "after"], fail-fast = true }
"#
        ),
    );
    repo.add();
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    let declared = &workspace.root_config().file().tools;
    let fetcher = fetcher(&server, cache.path());
    let mut asked: Vec<Vec<String>> = Vec::new();
    let start = |name: &str, asked: &mut Vec<Vec<String>>| {
        let request = RunRequest {
            name: name.to_owned(),
            ..RunRequest::default()
        };
        run::run(
            &workspace,
            &request,
            &mut |names| {
                asked.push(names.iter().map(ToString::to_string).collect());
                let wanted = declared
                    .iter()
                    .filter(|(name, _)| names.contains(*name))
                    .map(|(name, tool)| (name.clone(), tool.clone()))
                    .collect();
                Ok(install_for(&wanted, &fetcher, &platform())?.map(|installed| installed.bin))
            },
            &mut |_| {},
        )
        .expect("run")
    };

    let report = start("all", &mut asked);
    assert_eq!(report.exit_code(), 0, "{report:?}");
    assert_eq!(
        asked,
        [vec!["fake-format"], vec!["fake-format", "fake-lint"]],
        "one install per set, for the steps that run: not the skipped step's tool"
    );
    asked.clear();
    let report = start("stop", &mut asked);
    assert_eq!(report.exit_code(), 1, "{report:?}");
    assert!(
        asked.is_empty(),
        "a step that fail-fast stops installs nothing: {asked:?}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.file("formats.txt")).expect("formats"),
        "formatted\nno lint\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.file("plain.txt")).expect("plain"),
        "no format\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.file("both.txt")).expect("both"),
        "formatted\nlinted\n"
    );
}

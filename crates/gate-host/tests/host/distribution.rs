//! gate.lock, verified fetch and the cache, against the local fixture server.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use gate_host::ErrorKind;
use gate_host::cache::{Bounds as PruneBounds, CACHE_TAG, Cache, Reason, Sha256Sum};
use gate_host::config::{PluginName, PluginSource, SettingOrigin};
use gate_host::fetch::{Bounds, Endpoint, FetchConfig, Fetcher, Secret};
use gate_host::lock::{self, CorePin, Declaration, Lock, Pin, TARGETS, Wanted};
use sha2::{Digest as _, Sha256};

use crate::http::{Route, Server, release_json, sha256};

const PLUGIN: &[u8] = b"\0asm\x01\0\0\0 a synthetic plugin";

fn config(server: &Server, token: Option<&str>) -> FetchConfig {
    FetchConfig {
        github: Endpoint {
            web_url: server.url(""),
            api_url: server.url(""),
        },
        enterprise: None,
        tokens: vec![
            (
                "GATE_GITHUB_TOKEN".to_owned(),
                token.map(|value| Secret::new(value.to_owned())),
            ),
            ("GITHUB_TOKEN".to_owned(), None),
        ],
        gh: None,
    }
}

fn sum(text: &str) -> Sha256Sum {
    Sha256Sum::parse(text).expect("checksum")
}

/// Every file below `root`, relative, with its contents.
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path != root.join(CACHE_TAG) {
                let relative = path.strip_prefix(root).expect("below root").to_path_buf();
                found.insert(relative, std::fs::read(&path).expect("read"));
            }
        }
    }
    found
}

#[test]
fn a_pinned_download_is_verified_cached_and_then_needs_no_network() {
    let server = Server::start();
    server.serve("/git.wasm", PLUGIN);
    let cache_dir = tempfile::tempdir().expect("cache");
    let cache = Cache::at(cache_dir.path());
    let pin = Pin {
        url: server.url("/git.wasm"),
        sha256: sum(&sha256(PLUGIN)),
    };

    let path = Fetcher::new(config(&server, None), cache.clone())
        .fetch("git.wasm", &pin)
        .expect("fetch");
    assert_eq!(std::fs::read(&path).expect("read"), PLUGIN);
    assert_eq!(path, cache.blob(&pin.sha256));
    server.stop();

    let again = Fetcher::new(FetchConfig::from_env().expect("environment"), cache.clone())
        .fetch("git.wasm", &pin)
        .expect("from the cache, with the server gone");
    assert_eq!(again, path);
    assert!(
        files(&cache_dir.path().join("tmp")).is_empty(),
        "no temporary file is left"
    );
    assert!(cache.tagged(), "the first fill writes {CACHE_TAG}");
}

/// A one-second read bound, so a stalled server fails the test in seconds, not in 30.
const SHORT: Bounds = Bounds {
    connect: Duration::from_secs(5),
    read: Duration::from_secs(1),
};

/// Fetches `route` at `/git.wasm` within [`SHORT`], and checks that it failed in time with the
/// one timeout error, leaving nothing in the cache.
fn fails_within_the_bound(route: Route, body: &[u8]) {
    let server = Server::start();
    server.route("/git.wasm", route);
    let cache_dir = tempfile::tempdir().expect("cache");
    let cache = Cache::at(cache_dir.path());
    let pin = Pin {
        url: server.url("/git.wasm"),
        sha256: sum(&sha256(body)),
    };

    let started = Instant::now();
    let error = Fetcher::bounded(config(&server, None), cache.clone(), SHORT)
        .fetch("git.wasm", &pin)
        .expect_err("a server that stops sending");
    let waited = started.elapsed();

    assert!(waited < Duration::from_secs(10), "failed after {waited:?}");
    assert_eq!(error.kind(), ErrorKind::Environment);
    let message = error.message();
    assert!(
        message.contains(&format!("cannot download git.wasm from {}", pin.url))
            && message.ends_with("no data for 1 s; try again"),
        "{message}"
    );
    assert_eq!(cache.get(&pin.sha256).expect("read"), None);
    assert_eq!(
        files(&cache_dir.path().join("tmp")),
        BTreeMap::new(),
        "no temporary file is left"
    );
}

#[test]
fn a_download_that_stops_sending_fails_within_its_read_bound() {
    let body = vec![7_u8; 256 * 1024];
    fails_within_the_bound(
        Route::ok(body.clone()).stalled(1024, Duration::from_secs(30)),
        &body,
    );
}

#[test]
fn a_server_that_never_answers_fails_within_its_read_bound() {
    fails_within_the_bound(Route::ok(PLUGIN).delayed(Duration::from_secs(30)), PLUGIN);
}

#[test]
fn a_server_that_never_completes_the_connection_fails_within_its_connect_bound() {
    // A listener that takes the TCP connection and never speaks TLS. The TLS handshake is part of
    // connecting, so the connect bound ends it, on any machine and without the network.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    let _accepting = std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            std::thread::sleep(Duration::from_secs(30));
            drop(stream);
        }
    });
    let server = Server::start();
    let cache_dir = tempfile::tempdir().expect("cache");
    let cache = Cache::at(cache_dir.path());
    let pin = Pin {
        url: format!("https://{address}/git.wasm"),
        sha256: sum(&sha256(PLUGIN)),
    };
    let bounds = Bounds {
        connect: Duration::from_secs(1),
        read: Duration::from_secs(5),
    };

    let started = Instant::now();
    let error = Fetcher::bounded(config(&server, None), cache.clone(), bounds)
        .fetch("git.wasm", &pin)
        .expect_err("a connection that is never made");
    let waited = started.elapsed();

    assert!(waited < Duration::from_secs(10), "failed after {waited:?}");
    assert_eq!(error.kind(), ErrorKind::Environment);
    let message = error.message();
    assert_eq!(
        message,
        format!(
            "cannot download git.wasm from {}: no connection within 1 s; check the network (and HTTPS_PROXY, if one is needed) and try again",
            pin.url
        ),
    );
    assert_eq!(cache.get(&pin.sha256).expect("read"), None);
}

#[test]
fn a_slow_server_that_answers_within_the_bound_is_not_cut() {
    let server = Server::start();
    server.route(
        "/git.wasm",
        Route::ok(PLUGIN).delayed(Duration::from_millis(300)),
    );
    let cache_dir = tempfile::tempdir().expect("cache");
    let cache = Cache::at(cache_dir.path());
    let pin = Pin {
        url: server.url("/git.wasm"),
        sha256: sum(&sha256(PLUGIN)),
    };
    let path = Fetcher::bounded(config(&server, None), cache, SHORT)
        .fetch("git.wasm", &pin)
        .expect("a server that answers within the bound");
    assert_eq!(std::fs::read(&path).expect("read"), PLUGIN);
}

#[test]
fn a_corrupt_or_linked_cache_entry_is_never_returned_and_is_downloaded_again() {
    let server = Server::start();
    server.serve("/git.wasm", PLUGIN);
    let cache_dir = tempfile::tempdir().expect("cache");
    let cache = Cache::at(cache_dir.path());
    let pin = Pin {
        url: server.url("/git.wasm"),
        sha256: sum(&sha256(PLUGIN)),
    };
    let fetcher = Fetcher::new(config(&server, None), cache.clone());
    let path = fetcher.fetch("git.wasm", &pin).expect("fetch");

    // Truncated in place: the hit no longer hashes to its name.
    std::fs::write(&path, &PLUGIN[..4]).expect("truncate");
    assert_eq!(cache.get(&pin.sha256).expect("read"), None);
    assert!(!path.exists(), "the corrupt entry is removed");
    let again = fetcher.fetch("git.wasm", &pin).expect("downloaded again");
    assert_eq!(std::fs::read(&again).expect("read"), PLUGIN);

    // A symbolic link, even to the right bytes, is not an entry.
    let elsewhere = cache_dir.path().join("elsewhere.wasm");
    std::fs::write(&elsewhere, PLUGIN).expect("write");
    std::fs::remove_file(&path).expect("remove");
    std::os::unix::fs::symlink(&elsewhere, &path).expect("link");
    assert_eq!(cache.get(&pin.sha256).expect("read"), None);
    assert!(
        std::fs::symlink_metadata(&path).is_err(),
        "the link is removed"
    );
    assert!(elsewhere.exists(), "its target is left alone");
    let replaced = fetcher.fetch("git.wasm", &pin).expect("downloaded again");
    assert!(
        !std::fs::symlink_metadata(&replaced)
            .expect("entry")
            .file_type()
            .is_symlink()
    );
    let downloads = server
        .requests()
        .iter()
        .filter(|request| request.path == "/git.wasm")
        .count();
    assert_eq!(downloads, 3, "each bad entry was downloaded again");
}

#[test]
fn every_redirect_hop_is_checked() {
    let server = Server::start();
    server.serve("/git.wasm", PLUGIN);
    server.route("/hop", Route::redirect("/git.wasm"));
    server.route(
        "/away",
        Route::redirect("http://localhost:1@evil.example/git.wasm"),
    );
    server.route("/plain", Route::redirect("http://example.invalid/git.wasm"));
    let cache_dir = tempfile::tempdir().expect("cache");
    let fetcher = Fetcher::new(config(&server, None), Cache::at(cache_dir.path()));
    let pin = |path: &str| Pin {
        url: server.url(path),
        sha256: sum(&sha256(PLUGIN)),
    };

    fetcher
        .fetch("git.wasm", &pin("/hop"))
        .expect("a relative redirect on the same host");
    std::fs::remove_dir_all(cache_dir.path().join("blobs")).expect("empty the cache");
    for path in ["/away", "/plain"] {
        let error = fetcher
            .fetch("git.wasm", &pin(path))
            .expect_err("a hop gate does not fetch");
        assert_eq!(error.kind(), ErrorKind::Environment);
        assert!(error.message().contains("leads to"), "{error}");
    }
}

#[test]
fn a_checksum_mismatch_fails_closed_and_leaves_the_cache_as_it_was() {
    let server = Server::start();
    server.serve("/kept.wasm", b"kept".to_vec());
    server.serve("/git.wasm", PLUGIN);
    let cache_dir = tempfile::tempdir().expect("cache");
    let cache = Cache::at(cache_dir.path());
    let fetcher = Fetcher::new(config(&server, None), cache.clone());
    fetcher
        .fetch(
            "kept.wasm",
            &Pin {
                url: server.url("/kept.wasm"),
                sha256: sum(&sha256(b"kept")),
            },
        )
        .expect("an entry that must survive");
    let before = files(cache_dir.path());

    let expected = "c".repeat(64);
    let error = fetcher
        .fetch(
            "git.wasm",
            &Pin {
                url: server.url("/git.wasm"),
                sha256: sum(&expected),
            },
        )
        .expect_err("mismatch");

    assert_eq!(error.kind(), ErrorKind::Environment);
    let message = error.message();
    assert!(
        message.contains("checksum mismatch for git.wasm"),
        "{message}"
    );
    assert!(message.contains(&server.url("/git.wasm")), "{message}");
    assert!(message.contains(&expected), "{message}");
    assert!(message.contains(&sha256(PLUGIN)), "{message}");
    assert_eq!(files(cache_dir.path()), before, "the cache is as it was");
}

/// A private repository on the fixture: the download URL refuses, the API answers with
/// `Bearer good-credential` only, and the asset's API URL redirects to signed storage.
fn private_release(server: &Server) {
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
    server.serve("/storage/house", PLUGIN);
}

#[test]
fn a_private_asset_needs_a_credential_that_is_never_shown_nor_sent_on() {
    let server = Server::start();
    private_release(&server);
    let pin = Pin {
        url: server.url("/acme/rules/releases/download/plugin-house-v1.2.0/house.wasm"),
        sha256: sum(&sha256(PLUGIN)),
    };
    let cache_dir = tempfile::tempdir().expect("cache");
    let cache = Cache::at(cache_dir.path());

    let missing = Fetcher::new(config(&server, None), cache.clone())
        .fetch("house.wasm", &pin)
        .expect_err("no credential");
    assert_eq!(missing.kind(), ErrorKind::Environment);
    let message = missing.message();
    assert!(message.contains("house.wasm"), "{message}");
    assert!(message.contains(&pin.url), "{message}");
    assert!(message.contains("GATE_GITHUB_TOKEN"), "{message}");

    let wrong = "wrong-credential-that-must-not-show";
    let refused = Fetcher::new(config(&server, Some(wrong)), cache.clone())
        .fetch("house.wasm", &pin)
        .expect_err("refused credential");
    let text = format!("{refused} {refused:?}");
    assert!(text.contains("$GATE_GITHUB_TOKEN"), "{text}");
    assert!(!text.contains(wrong), "the credential appears: {text}");
    assert!(files(cache_dir.path()).is_empty(), "nothing was cached");

    let path = Fetcher::new(config(&server, Some("good-credential")), cache)
        .fetch("house.wasm", &pin)
        .expect("with the credential");
    assert_eq!(std::fs::read(path).expect("read"), PLUGIN);
    for request in server.requests() {
        if request.authorization.is_some() {
            assert!(
                request.path.starts_with("/repos/"),
                "a credential went to {}",
                request.path
            );
        }
    }
}

#[test]
fn two_concurrent_first_fetches_both_succeed_with_one_entry() {
    let server = Server::start();
    server.route(
        "/git.wasm",
        Route::ok(PLUGIN).delayed(Duration::from_millis(300)),
    );
    let cache_dir = tempfile::tempdir().expect("cache");
    let pin = Pin {
        url: server.url("/git.wasm"),
        sha256: sum(&sha256(PLUGIN)),
    };

    let results: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let fetcher = Fetcher::new(config(&server, None), Cache::at(cache_dir.path()));
                let pin = &pin;
                scope.spawn(move || fetcher.fetch("git.wasm", pin))
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("thread"))
            .collect()
    });

    let downloads = server
        .requests()
        .iter()
        .filter(|request| request.path == "/git.wasm")
        .count();
    assert_eq!(downloads, 2, "both fetched before either finished");
    for result in results {
        assert_eq!(std::fs::read(result.expect("fetch")).expect("read"), PLUGIN);
    }
    let entries = files(cache_dir.path());
    assert_eq!(
        entries.keys().cloned().collect::<Vec<_>>(),
        vec![PathBuf::from("blobs/sha256").join(sha256(PLUGIN))]
    );
}

fn source(text: &str) -> PluginSource {
    PluginSource::parse(text).expect("source")
}

fn wanted(sources: &[(&str, &str)]) -> Wanted {
    let mut plugins = BTreeMap::new();
    for (name, text) in sources {
        plugins.insert(
            source(text),
            vec![Declaration {
                name: PluginName::try_from((*name).to_owned()).expect("name"),
                origin: SettingOrigin {
                    file: "gate.toml".to_owned(),
                    line: 1,
                    section: "[plugins]".to_owned(),
                },
            }],
        );
    }
    Wanted {
        core: "0.1.0".to_owned(),
        plugins,
    }
}

/// Releases on the fixture: core v0.1.0 with both archives, and plugin git 0.1.0. `digest`
/// replaces the plugin's published digest.
fn releases(server: &Server, digest: Option<&str>) {
    let mut core = Vec::new();
    for target in TARGETS {
        let name = format!("gate-{target}.tar.gz");
        let body = format!("core archive {target}");
        server.serve(
            &format!("/ElsiKora/Gate/releases/download/v0.1.0/{name}"),
            body.clone(),
        );
        core.push((name, sha256(body.as_bytes())));
    }
    let core_assets: Vec<(String, String, String, Option<String>)> = core
        .iter()
        .map(|(name, digest)| {
            (
                name.clone(),
                server.url("/repos/ElsiKora/Gate/releases/assets/1"),
                server.url(&format!("/ElsiKora/Gate/releases/download/v0.1.0/{name}")),
                Some(digest.clone()),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str, &str, Option<&str>)> = core_assets
        .iter()
        .map(|(name, api, download, digest)| {
            (
                name.as_str(),
                api.as_str(),
                download.as_str(),
                digest.as_deref(),
            )
        })
        .collect();
    server.serve(
        "/repos/ElsiKora/Gate/releases/tags/v0.1.0",
        release_json(&borrowed),
    );

    server.serve(
        "/ElsiKora/Gate/releases/download/plugin-git-v0.1.0/git.wasm",
        PLUGIN,
    );
    let published = digest.map_or_else(|| sha256(PLUGIN), ToOwned::to_owned);
    server.serve(
        "/repos/ElsiKora/Gate/releases/tags/plugin-git-v0.1.0",
        release_json(&[(
            "git.wasm",
            &server.url("/repos/ElsiKora/Gate/releases/assets/2"),
            &server.url("/ElsiKora/Gate/releases/download/plugin-git-v0.1.0/git.wasm"),
            Some(&published),
        )]),
    );
}

#[test]
fn lock_resolves_releases_into_pins_and_keeps_what_it_has() {
    let server = Server::start();
    releases(&server, None);
    let cache_dir = tempfile::tempdir().expect("cache");
    let fetcher = Fetcher::new(config(&server, None), Cache::at(cache_dir.path()));
    let wanted = wanted(&[("git", "ElsiKora/Gate:git@0.1.0")]);

    let first = lock::resolve(&wanted, None, &fetcher).expect("resolve");
    assert_eq!(
        first.resolved,
        vec!["core 0.1.0", "ElsiKora/Gate:git@0.1.0"]
    );
    let git = &first.lock.plugins[&source("ElsiKora/Gate:git@0.1.0")];
    assert_eq!(
        git.url,
        server.url("/ElsiKora/Gate/releases/download/plugin-git-v0.1.0/git.wasm")
    );
    assert_eq!(git.sha256.as_str(), sha256(PLUGIN));
    assert_eq!(first.lock.mismatches(&wanted), Vec::<String>::new());
    assert!(
        Cache::at(cache_dir.path())
            .get(&git.sha256)
            .expect("read")
            .is_some(),
        "the download is cached"
    );

    server.stop();
    let second = lock::resolve(&wanted, Some(&first.lock), &fetcher).expect("no network needed");
    assert_eq!(second.lock, first.lock);
    assert_eq!(second.resolved, Vec::<String>::new());
}

#[test]
fn lock_refuses_a_download_that_differs_from_the_published_digest() {
    let server = Server::start();
    releases(&server, Some(&"d".repeat(64)));
    let cache_dir = tempfile::tempdir().expect("cache");
    let fetcher = Fetcher::new(config(&server, None), Cache::at(cache_dir.path()));

    let error = lock::resolve(
        &wanted(&[("git", "ElsiKora/Gate:git@0.1.0")]),
        None,
        &fetcher,
    )
    .expect_err("digest mismatch");
    assert_eq!(error.kind(), ErrorKind::Environment);
    assert!(
        error
            .message()
            .contains("the release publishes sha256 dddd"),
        "{error}"
    );
    let left: Vec<PathBuf> = files(cache_dir.path()).into_keys().collect();
    assert!(
        left.iter()
            .all(|path| !path.starts_with("blobs/sha256") || !path.ends_with(sha256(PLUGIN))),
        "the plugin never entered the cache: {left:?}"
    );
}

fn locked(plugins: &[(&str, char)]) -> Lock {
    let pin = |url: String, digit: char| Pin {
        url,
        sha256: sum(&digit.to_string().repeat(64)),
    };
    Lock {
        core: Some(CorePin {
            version: "0.1.0".to_owned(),
            targets: TARGETS
                .iter()
                .map(|target| {
                    (
                        (*target).to_owned(),
                        pin(format!("https://example.invalid/gate-{target}.tar.gz"), 'a'),
                    )
                })
                .collect(),
        }),
        plugins: plugins
            .iter()
            .map(|(text, digit)| {
                (
                    source(text),
                    pin(format!("https://example.invalid/{text}.wasm"), *digit),
                )
            })
            .collect(),
    }
}

#[test]
fn lock_set_writes_exactly_the_pin() {
    let before = locked(&[
        ("ElsiKora/Gate:git@0.1.0", 'b'),
        ("acme/rules:house@1.2.0", 'c'),
    ]);
    let after_release = wanted(&[
        ("git", "ElsiKora/Gate:git@0.1.0"),
        ("house", "acme/rules:house@1.3.0"),
    ]);
    let url = "https://github.com/acme/rules/releases/download/plugin-house-v1.3.0/house.wasm";
    let set = lock::set(
        &after_release,
        Some(before.clone()),
        &PluginName::try_from("house".to_owned()).expect("name"),
        Pin {
            url: url.to_owned(),
            sha256: sum(&"e".repeat(64)),
        },
    )
    .expect("set");

    let old = before.render();
    let new = set.render();
    let removed: Vec<&str> = old
        .lines()
        .filter(|line| !new.lines().any(|other| other == *line))
        .collect();
    let added: Vec<&str> = new
        .lines()
        .filter(|line| !old.lines().any(|other| other == *line))
        .collect();
    assert_eq!(
        removed,
        vec![
            "[plugins.\"acme/rules:house@1.2.0\"]",
            "url = \"https://example.invalid/acme/rules:house@1.2.0.wasm\"",
            &format!("sha256 = \"{}\"", "c".repeat(64)),
        ]
    );
    assert_eq!(
        added,
        vec![
            "[plugins.\"acme/rules:house@1.3.0\"]",
            &format!("url = \"{url}\""),
            &format!("sha256 = \"{}\"", "e".repeat(64)),
        ]
    );
    assert_eq!(set.mismatches(&after_release), Vec::<String>::new());

    let unknown = lock::set(
        &after_release,
        Some(before.clone()),
        &PluginName::try_from("angular".to_owned()).expect("name"),
        Pin {
            url: url.to_owned(),
            sha256: sum(&"e".repeat(64)),
        },
    );
    assert!(unknown.is_err_and(|error| {
        error
            .message()
            .contains("no gate.toml declares plugin angular")
    }));
    let none = lock::set(
        &after_release,
        None,
        &PluginName::try_from("house".to_owned()).expect("name"),
        Pin {
            url: url.to_owned(),
            sha256: sum(&"e".repeat(64)),
        },
    );
    assert!(none.is_err_and(|error| error.message().contains("run gate lock first")));
}

#[test]
fn a_lock_that_does_not_match_says_what_differs() {
    let lock = locked(&[("ElsiKora/Gate:old@0.1.0", 'b')]);
    let mut wanted = wanted(&[("git", "ElsiKora/Gate:git@0.1.0")]);
    wanted.core = "0.2.0".to_owned();
    assert_eq!(
        lock.mismatches(&wanted),
        vec![
            "it pins core 0.1.0, and gate.toml asks for 0.2.0 ([gate] version)",
            "it does not pin plugin git = ElsiKora/Gate:git@0.1.0 (gate.toml:1 [plugins])",
            "it pins ElsiKora/Gate:old@0.1.0, which no gate.toml declares",
        ]
    );

    let wanted = Wanted::core("0.1.0");
    let mut no_core = locked(&[]);
    no_core.core = None;
    assert_eq!(
        no_core.mismatches(&wanted),
        vec!["it pins no core, and gate.toml asks for 0.1.0 ([gate] version)"]
    );
    let mut one_target = locked(&[]);
    if let Some(core) = one_target.core.as_mut() {
        core.targets.remove(TARGETS[1]);
    }
    assert_eq!(
        one_target.mismatches(&wanted),
        vec![format!("it has no core archive for {}", TARGETS[1])]
    );
}

#[test]
fn prune_keeps_every_entry_a_lock_pins_and_modules_while_a_plugin_is_pinned() {
    let cache_dir = tempfile::tempdir().expect("cache");
    let cache = Cache::at(cache_dir.path());
    let kept = sum(&"a".repeat(64));
    let dropped = sum(&"b".repeat(64));
    for (path, contents) in [
        (cache.blob(&kept), "kept"),
        (cache.blob(&dropped), "dropped"),
        (cache.core(&kept).join("gate"), "kept core"),
        (cache.core(&dropped).join("gate"), "dropped core"),
        // wasmtime's own layout, opaque to gate.
        (
            cache.modules().join("wasmtime-cache").join("x1y2"),
            "module",
        ),
    ] {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directories");
        std::fs::write(path, contents).expect("write");
    }
    let referenced: BTreeSet<Sha256Sum> = [kept.clone()].into_iter().collect();

    let refused = cache
        .unreferenced(&referenced, false)
        .expect_err("no tag: maybe not gate's cache");
    assert!(
        refused.message().contains("has no CACHEDIR.TAG"),
        "{refused}"
    );
    std::fs::write(
        cache_dir.path().join(CACHE_TAG),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .expect("tag");

    let paths = |entries: &[gate_host::cache::Entry]| {
        let mut paths: Vec<PathBuf> = entries.iter().map(|entry| entry.path.clone()).collect();
        paths.sort();
        paths
    };
    let mut expected = vec![cache.blob(&dropped), cache.core(&dropped)];
    expected.sort();
    let entries = cache.unreferenced(&referenced, true).expect("list");
    assert_eq!(
        paths(&entries),
        expected,
        "modules stay while a plugin is pinned"
    );
    assert_eq!(files(cache_dir.path()).len(), 5, "listing removes nothing");

    let without_plugins = cache.unreferenced(&referenced, false).expect("list");
    expected.push(cache.modules());
    expected.sort();
    assert_eq!(
        paths(&without_plugins),
        expected,
        "modules/ goes as one unit"
    );

    cache.remove(&without_plugins).expect("remove");
    let left: Vec<PathBuf> = files(cache_dir.path()).into_keys().collect();
    assert_eq!(left.len(), 2);
    assert!(
        left.iter()
            .all(|path| path.to_string_lossy().contains(kept.as_str()))
    );
    assert!(!cache.modules().exists());

    // A symbolic link in the layout is never followed: prune refuses and removes nothing.
    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(outside.path().join("precious"), "keep").expect("write");
    std::os::unix::fs::symlink(outside.path(), cache.root().join("core").join("linked"))
        .expect("link");
    std::fs::remove_dir_all(cache.root().join("tmp")).ok();
    std::os::unix::fs::symlink(outside.path(), cache.root().join("tmp")).expect("link tmp");
    let refused = cache
        .unreferenced(&referenced, true)
        .expect_err("a linked tmp/");
    assert!(
        refused.message().contains("is a symbolic link"),
        "{refused}"
    );
    std::fs::remove_file(cache.root().join("tmp")).expect("unlink tmp");
    let entries = cache.unreferenced(&referenced, true).expect("list");
    assert_eq!(
        paths(&entries),
        vec![cache.root().join("core").join("linked")],
        "a linked entry is listed as the link itself"
    );
    cache.remove(&entries).expect("remove");
    assert!(
        outside.path().join("precious").exists(),
        "removing a link leaves its target"
    );
}

/// The size of the tag [`tagged_cache`] writes.
const TAG_BYTES: u64 = 44;

/// A tagged, empty cache.
fn tagged_cache() -> (tempfile::TempDir, Cache) {
    let directory = tempfile::tempdir().expect("cache");
    std::fs::write(
        directory.path().join(CACHE_TAG),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .expect("tag");
    let cache = Cache::at(directory.path());
    (directory, cache)
}

/// Writes `contents` at `path`, last used `days` days before `now`.
fn put(path: &Path, contents: &str, now: SystemTime, days: u64) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("directories");
    std::fs::write(path, contents).expect("write");
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open")
        .set_modified(now - Duration::from_hours(24 * days))
        .expect("set the time");
}

/// Where the content of the blob `id` is stored.
fn stored_blob(cache: &Cache, id: &str) -> PathBuf {
    cache.answer(
        "blobs",
        &Sha256Sum::from_digest(&Sha256::digest(id.as_bytes())),
    )
}

/// The removed paths of a plan, with their reasons.
fn removed(plan: &gate_host::cache::Plan) -> Vec<(PathBuf, Reason)> {
    plan.entries
        .iter()
        .map(|entry| (entry.path.clone(), entry.reason))
        .collect()
}

#[test]
fn prune_removes_derived_entries_nothing_can_use_and_bounds_never_remove_a_pinned_download() {
    let (_directory, cache) = tagged_cache();
    let now = SystemTime::now();
    let pinned = sum(&"a".repeat(64));
    let unpinned = sum(&"c".repeat(64));
    let key = sum(&"e".repeat(64));
    // A pinned download and its core, last used long ago: no bound removes them.
    put(&cache.blob(&pinned), "plugin", now, 400);
    put(&cache.core(&pinned).join("gate"), "core", now, 400);
    // Results of the pinned plugin, of an unpinned one, and the format-2 layout.
    let kept_result = cache.result(pinned.as_str(), &key);
    put(&kept_result, "kept result", now, 40);
    put(&cache.result(unpinned.as_str(), &key), "unpinned", now, 0);
    let format_2 = cache.results().join("ee");
    put(&format_2.join("e".repeat(64)), "format 2", now, 0);
    // One stored answer names the content of blob 2...2; the content of 3...3 is named by none.
    let answers = cache.answer("answers/1", &sum(&"f".repeat(64)));
    let answer = format!(
        r#"{{"kind":"read-at","of":["{}","a.txt"],"answer":{{"blob":"{}"}}}}"#,
        "1".repeat(40),
        "2".repeat(40)
    );
    put(&answers, &format!("{answer}\n"), now, 0);
    let named = stored_blob(&cache, &"2".repeat(40));
    put(&named, "named", now, 0);
    let unnamed = stored_blob(&cache, &"3".repeat(40));
    put(&unnamed, "unnamed", now, 0);
    let sync_state = cache.answer("sync-state", &key);
    put(&sync_state, "state", now, 40);

    let referenced: BTreeSet<Sha256Sum> = [pinned.clone()].into_iter().collect();
    let plugins = referenced.clone();

    // Without a bound: what nothing can use goes, and nothing else.
    let plan = cache
        .plan(&referenced, &plugins, &PruneBounds::default())
        .expect("plan");
    let mut expected = vec![
        (format_2.clone(), Reason::Unpinned),
        (cache.results().join(unpinned.as_str()), Reason::Unpinned),
        (unnamed.clone(), Reason::Unnamed),
    ];
    expected.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(removed(&plan), expected);
    assert_eq!(plan.over, None);
    assert_eq!(files(cache.root()).len(), 9, "planning removes nothing");

    // Older than 30 days: the derived files last used 40 days ago go too; the pinned download
    // and core, 400 days old, stay.
    let bounds = PruneBounds {
        older_than: Some(30),
        max_size: None,
    };
    let plan = cache.plan(&referenced, &plugins, &bounds).expect("plan");
    expected.extend([
        (kept_result.clone(), Reason::Older(30)),
        (sync_state.clone(), Reason::Older(30)),
    ]);
    expected.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(removed(&plan), expected);

    // Every bound at its tightest: every derived file goes, no pinned download does, and the
    // plan says the cache stays over the size.
    let bounds = PruneBounds {
        older_than: Some(0),
        max_size: Some(1),
    };
    let plan = cache.plan(&referenced, &plugins, &bounds).expect("plan");
    let paths: Vec<PathBuf> = plan
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect();
    for kept in [cache.blob(&pinned), cache.core(&pinned)] {
        assert!(
            !paths
                .iter()
                .any(|path| kept.starts_with(path) || path.starts_with(&kept)),
            "{kept:?} is pinned: {paths:?}"
        );
    }
    for gone in [&kept_result, &answers, &named, &sync_state] {
        assert!(paths.contains(gone), "{gone:?} should go: {paths:?}");
    }
    assert_eq!(plan.over, Some(1));
    assert_eq!(
        plan.left,
        "plugin".len() as u64 + "core".len() as u64 + TAG_BYTES
    );

    cache.remove(&plan.entries).expect("remove");
    let left: Vec<PathBuf> = files(cache.root()).into_keys().collect();
    assert_eq!(left.len(), 2, "{left:?}");
}

#[test]
fn max_size_removes_the_least_recently_used_derived_files_first_by_path_on_a_tie() {
    let (_directory, cache) = tagged_cache();
    let now = SystemTime::now();
    let pinned = sum(&"a".repeat(64));
    put(&cache.blob(&pinned), &"p".repeat(1000), now, 400);
    let state = |digit: &str| cache.answer("sync-state", &sum(&digit.repeat(64)));
    let hundred = "s".repeat(100);
    put(&state("4"), &hundred, now, 0);
    put(&state("3"), &hundred, now, 2);
    put(&state("2"), &hundred, now, 2);
    put(&state("1"), &hundred, now, 3);
    let total: u64 = files(cache.root())
        .values()
        .map(|bytes| bytes.len() as u64)
        .sum::<u64>()
        + TAG_BYTES;

    let referenced: BTreeSet<Sha256Sum> = [pinned].into_iter().collect();
    let bounds = PruneBounds {
        older_than: None,
        max_size: Some(total - 150),
    };
    let plan = cache.plan(&referenced, &referenced, &bounds).expect("plan");
    let max = total - 150;
    assert_eq!(
        removed(&plan),
        vec![
            (state("1"), Reason::OverSize(max)),
            (state("2"), Reason::OverSize(max)),
        ],
        "the oldest, then of the two used at the same time the first by path"
    );
    assert_eq!(plan.left, total - 200);
    assert_eq!(plan.over, None);
}

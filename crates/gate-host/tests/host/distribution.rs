//! gate.lock, verified fetch and the cache, against the local fixture server.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gate_host::ErrorKind;
use gate_host::cache::{CACHE_TAG, Cache, Sha256Sum};
use gate_host::config::{PluginName, PluginSource, SettingOrigin};
use gate_host::fetch::{Bounds, Endpoint, FetchConfig, Fetcher, Secret};
use gate_host::lock::{self, CorePin, Declaration, Lock, Pin, TARGETS, Wanted};

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
    assert!(
        files(&cache_dir.path().join("tmp")).is_empty(),
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

//! The limits measured against the bundled plugins: the fuel and wall-clock time of each plugin's
//! slowest call over synthetic trees, against [`Config::default`]. A measurement, not a test:
//! `cargo test --release -p gate-runtime --test measure -- --ignored --nocapture`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gate_runtime::{Blob, Call, Config, ErrorKind, Grant, HostError, Plugin, Runtime, Tree};
use gate_sdk::{CheckRequest, RuleRun};

/// The component of `package`, built for `wasm32-wasip2` in release.
fn component(package: &str, library: &str) -> Vec<u8> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("measure");
    let status = Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .args([
            "build",
            "--locked",
            "--release",
            "-p",
            package,
            "--target",
            "wasm32-wasip2",
        ])
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .expect("cargo runs");
    assert!(status.success(), "{package} builds for wasm32-wasip2");
    std::fs::read(target.join(format!("wasm32-wasip2/release/{library}.wasm")))
        .expect("the component exists")
}

/// A tree of `files` TypeScript sources of `lines` lines each, half of them TSX, every tenth with
/// the marker of unfinished work.
struct Synthetic {
    paths: Vec<String>,
    files: BTreeMap<String, Vec<u8>>,
}

impl Synthetic {
    fn new(files: usize, lines: usize) -> Arc<Self> {
        let mut tree = BTreeMap::new();
        for n in 0..files {
            let tsx = n % 2 == 1;
            let mut text = String::from("import { helper } from \"./helper\";\n\n");
            let mut line = 2;
            let mut block = 0;
            while line < lines {
                let _ = write!(
                    text,
                    "export interface Shape{block}<T extends object = object> {{\n  id: number;\n  name: string;\n  tags: ReadonlyArray<string>;\n  value?: T;\n}}\n\nexport class Store{block}<T extends object> {{\n  private readonly items = new Map<number, Shape{block}<T>>();\n\n  add(item: Shape{block}<T>): void {{\n    if (item.id < 0) {{\n      throw new Error(`negative id ${{item.id}}`);\n    }}\n    this.items.set(item.id, {{ ...item, tags: [...item.tags, \"added\"] }});\n  }}\n\n  find(predicate: (item: Shape{block}<T>) => boolean): Shape{block}<T>[] {{\n    return [...this.items.values()].filter(predicate).map((item) => helper(item));\n  }}\n}}\n\n"
                );
                line += 24;
                if tsx {
                    let _ = write!(
                        text,
                        "export const View{block} = ({{ items }}: {{ items: string[] }}) => (\n  <ul className=\"list\">\n    {{items.map((item) => <li key={{item}}>{{item}}</li>)}}\n  </ul>\n);\n\n"
                    );
                    line += 6;
                }
                if n % 10 == 0 && block == 0 {
                    text.push_str("// FIXME: split this file\n");
                    line += 1;
                }
                block += 1;
            }
            let extension = if tsx { "tsx" } else { "ts" };
            tree.insert(format!("src/m{n:05}/index.{extension}"), text.into_bytes());
        }
        Arc::new(Self {
            paths: tree.keys().cloned().collect(),
            files: tree,
        })
    }

    fn bytes(&self) -> usize {
        self.files.values().map(Vec::len).sum()
    }
}

impl Tree for Synthetic {
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

fn call<'p>(plugin: &'p Plugin, rule: &str, tree: &Arc<Synthetic>) -> Call<'p> {
    Call {
        plugin,
        request: CheckRequest {
            rules: vec![RuleRun {
                name: rule.to_owned(),
                options: "{}".to_owned(),
                ..RuleRun::default()
            }],
            ..CheckRequest::default()
        },
        tree: Arc::clone(tree) as Arc<dyn Tree>,
        history: None,
        grant: Grant::repository(),
    }
}

/// Whether `rule` of the component `wasm` finishes over `tree` within `fuel`.
fn fits(wasm: &[u8], rule: &str, tree: &Arc<Synthetic>, fuel: u64) -> bool {
    let runtime = Runtime::new(Config {
        fuel,
        deadline: Duration::from_secs(3600),
        ..Config::default()
    })
    .expect("the runtime starts");
    let plugin = runtime.load("measured", wasm).expect("the plugin loads");
    match runtime.check(&call(&plugin, rule, tree)) {
        Ok(_) => true,
        Err(error) if error.kind() == ErrorKind::OutOfFuel => false,
        Err(error) => panic!("{rule}: {error}"),
    }
}

/// The fuel `rule` uses over `tree`, to within 1%.
fn fuel_used(wasm: &[u8], rule: &str, tree: &Arc<Synthetic>) -> u64 {
    let mut high = 1_000_000_u64;
    while !fits(wasm, rule, tree, high) {
        high *= 2;
    }
    let mut low = high / 2;
    while high - low > high / 100 {
        let middle = low + (high - low) / 2;
        if fits(wasm, rule, tree, middle) {
            high = middle;
        } else {
            low = middle;
        }
    }
    high
}

/// The wall-clock time of `rule` over `tree` under the default limits, its fastest of three.
fn wall_time(wasm: &[u8], rule: &str, tree: &Arc<Synthetic>) -> Duration {
    let runtime = Runtime::new(Config::default()).expect("the runtime starts");
    let plugin = runtime.load("measured", wasm).expect("the plugin loads");
    (0..3)
        .map(|_| {
            let started = Instant::now();
            runtime
                .check(&call(&plugin, rule, tree))
                .expect("the call finishes under the default limits");
            started.elapsed()
        })
        .min()
        .unwrap_or_default()
}

#[test]
#[ignore = "a measurement; run it with --ignored --nocapture"]
#[expect(
    clippy::cast_precision_loss,
    reason = "the margins are printed to one decimal"
)]
fn the_default_limits_against_the_slowest_calls_of_the_bundled_plugins() {
    let plugins = [
        (
            "typescript/parse",
            component("gate-plugin-typescript", "typescript"),
            "parse",
        ),
        (
            "hello/marker",
            component("gate-plugin-hello", "gate_plugin_hello"),
            "marker",
        ),
    ];
    let defaults = Config::default();
    println!(
        "default fuel {} units, default deadline {} s",
        defaults.fuel,
        defaults.deadline.as_secs_f64()
    );
    for (files, lines) in [(100, 300), (2_000, 300), (200, 5_000)] {
        let tree = Synthetic::new(files, lines);
        for (label, wasm, rule) in &plugins {
            let fuel = fuel_used(wasm, rule, &tree);
            let time = wall_time(wasm, rule, &tree);
            println!(
                "{label} over {files} files of {lines} lines ({} bytes): fuel {fuel} ({:.1}x under the default), {:.3} s ({:.1}x under the default)",
                tree.bytes(),
                defaults.fuel as f64 / fuel as f64,
                time.as_secs_f64(),
                defaults.deadline.as_secs_f64() / time.as_secs_f64()
            );
        }
    }
}

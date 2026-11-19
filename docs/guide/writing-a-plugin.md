# Writing a Plugin

A plugin is one WebAssembly component, `<name>.wasm`, that holds rules, generators and presets. This
guide builds one in Rust with the SDK, tests it on fixture trees, releases it and pins it in a
repository. The rule inside it is covered in [writing-a-rule.md](writing-a-rule.md).

## 🧠 What a Plugin Is

- **One file for every platform**, pinned by one sha256. Nobody who runs your plugin needs a compiler.
- **A sandbox.** The plugin sees what the core gives it and nothing else: the tracked files of its
  scope, read-only; git data, if a rule declares that it reads history; its options. It cannot write
  files, open the network, read the clock or the environment. That is what makes results cacheable and
  your plugin safe for others to run.
- **Three exports.** `describe` returns the plugin's name, version, rules, generators and presets.
  `check` runs the enabled rules of one project and returns findings. `generate` returns the files of
  one generator.
- **One call per project.** One `check` call carries every enabled rule of the plugin with its options,
  so the plugin reads and parses each file once for all of its rules.

The interface is the WIT package `gate:plugin`, kept in `wit/` and sketched in
[architecture.md](../architecture.md#4-plugin-interface). The Rust SDK wraps it; any language that builds WebAssembly components can implement it directly.

## 📋 Prerequisites

- Rust through `rustup`, on the channel that Gate's own `rust-toolchain.toml` pins at the tag you
  build against, with the `wasm32-wasip2` target. A `rust-toolchain.toml` in your crate makes `rustup`
  install both:

  ```toml
  # rust-toolchain.toml
  [toolchain]
  channel = "1.99.1"
  targets = ["wasm32-wasip2"]
  ```

- `jq`, for the reproducible build in [step 3](#3-build).

## 1. Create the Crate

```bash
cargo new --lib plugin-todo
cd plugin-todo
```

```toml
# Cargo.toml
[package]
name = "plugin-todo"
version = "0.1.0"
edition = "2024"
license = "MIT"

[lib]
# cdylib is the component; rlib lets the native fixture tests call the plugin.
crate-type = ["cdylib", "rlib"]

[dependencies]
gate-sdk = { git = "https://github.com/ElsiKora/Gate", tag = "v0.2.0" }
regex = "1"

[dev-dependencies]
gate-test = { git = "https://github.com/ElsiKora/Gate", tag = "v0.2.0" }
```

`gate-sdk` and `gate-test` are not published to crates.io: they are crates of the Gate repository, so
you depend on them by git, at the tag of a Gate release. Take both from the same tag. The SDK
re-exports `serde` and `schemars` for your options, so you need neither as a dependency of your own.

```bash
cargo generate-lockfile
```

Commit `Cargo.lock`: every build below passes `--locked`.

## 2. Declare the Plugin and Its Rules

```rust
// src/lib.rs
mod no_bare_todo;

pub use self::no_bare_todo::NoBareTodo;

gate_sdk::plugin! {
    name: "todo",
    version: env!("CARGO_PKG_VERSION"),
    options: gate_sdk::NoOptions,
    rules: [NoBareTodo],
    generators: [],
    presets: [gate_sdk::Preset {
        name: "recommended",
        rules: r#"{ "no-bare-todo": { "level": "warning", "options": { "pattern": "#[0-9]+" } } }"#,
    }],
}
```

The macro takes its six fields in this order, every one of them, with `[]` for an empty list. It
defines `pub static PLUGIN: gate_sdk::PluginSpec`, which the tests call, and, built for `wasm32`, the
component's three exports: `describe` from the declarations, including the options schema of each
rule and of the plugin itself; `check`, which hands each enabled rule its options; `generate`. A
preset is a `gate_sdk::Preset` whose `rules` is a JSON object from the rule's name inside the plugin
to its `level` and `options`; a repository takes it with `extends = ["todo/recommended"]` under
`[project]`. `options` is the type of
the `[plugins.todo] options` every rule shares, read with `run.plugin_options()`; a plugin without
such options names `gate_sdk::NoOptions`, which accepts only an empty table. Gate checks the
configuration against these schemas when the plugin loads, so a missing, unknown or mistyped option
is a configuration error before anything runs. A plugin option only some rules read is optional in
the plugin's type; each rule that reads it lists it in `const NEEDS: &'static [gate_sdk::Need]`
(the key, the JSON Schema of the plugin options under which it reads the key, and why), and Gate
refuses a configuration that enables the rule without the key, naming the rule.

The rule itself, in `src/no_bare_todo.rs`, is the subject of [writing-a-rule.md](writing-a-rule.md). A
generator is a `#[derive(Debug, Default)]` type that implements `gate_sdk::Generator` (`NAME`, `TITLE`,
the globs it `OWNS` and `generate`, which writes each file with `run.write(path, contents)`) and goes in
`generators: [...]`.

## 3. Build

For development:

```bash
cargo build --release --locked --target wasm32-wasip2
cp target/wasm32-wasip2/release/plugin_todo.wasm todo.wasm
```

For a release, build with `scripts/build-component.sh` of the Gate repository, at the same tag as the
SDK, copied into your crate's `scripts/`. It takes the package and the target, builds that one package
with `--release --locked` and remaps the checkout, target and cargo home paths, so the sha256 depends
only on the source, `Cargo.lock`, the toolchain and the runner image:

```bash
mkdir -p scripts
curl -fsSL https://raw.githubusercontent.com/ElsiKora/Gate/v0.2.0/scripts/build-component.sh -o scripts/build-component.sh
sh scripts/build-component.sh plugin-todo wasm32-wasip2
cp target/wasm32-wasip2/release/plugin_todo.wasm todo.wasm
```

The artifact is `todo.wasm`. Build it in release mode: the core compiles it once per sha256 and caches
the compiled module, so its size matters less than its speed.

## 4. Test on Fixture Trees

Every rule ships at least one passing and one failing fixture. A rule's fixtures are one directory,
one case per subdirectory; a case is the tree the rule sees, the rule's options and the findings it must
produce:

```
tests/fixtures/no-bare-todo/
├── pass/
│   ├── tree/src/main.rs           # TODOs that cite an issue
│   ├── options.json
│   └── expected.txt               # empty
└── fail/
    ├── tree/src/main.rs           # a bare TODO on line 3
    ├── options.json
    └── expected.txt
```

`tree/` holds the files with paths relative to the project root. `options.json` holds the rule's
options; without it the rule gets `{}`. Both cases here use:

```json
{ "pattern": "#[0-9]+" }
```

The failing case's `tree/src/main.rs`:

```rust
fn main() {
    // TODO(#12): log the start
    // TODO tidy this up
}
```

Its `expected.txt` holds one line per finding, `<location> <rule>[#<aspect>]: <message>`, with the
rule's name inside the plugin and no level, then the finding's `target:` and `help:` lines:

```text
src/main.rs:3:8 no-bare-todo: TODO without an issue reference
  target: src/main.rs:3
  help: cite the issue, for example `TODO(#123)`; the pattern is `#[0-9]+`
```

`gate-test` runs the rule natively, through the same SDK code the component runs, on each case's tree,
and compares what it reports with `expected.txt`. It runs neither `todo.wasm` nor git, so the tests need
no build for `wasm32-wasip2`:

```rust
// tests/fixtures.rs
#[test]
fn no_bare_todo() {
    gate_test::check_fixtures(
        &plugin_todo::PLUGIN,
        "no-bare-todo",
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/no-bare-todo"),
    )
    .unwrap_or_else(|failure| panic!("{failure}"));
}
```

```bash
cargo test --locked
```

Set `GATE_BLESS=1` to write each `expected.txt` from what the rule reports, then read the diff before
you commit it.

## 5. Try It in a Repository

Before you release, run the built file in place of the pinned plugin, for one run:

```bash
bin/gate check --plugin todo=../plugin-todo/todo.wasm
```

The override exists only on the command line, never in a file. For that run the report names the
override and its sha256 on every summary line, its results never enter the shared cache, and `gate lock`
refuses to run while one is given. The jobs `gate sync` generates for CI pass `--no-overrides`, which
refuses it. A repository that keeps its own plugin may use `--plugin` in a CI job of its own that proves
a changed plugin on its tree before the release.

Only you, the plugin's author, need the toolchain (Rust with the `wasm32-wasip2` target and `gate-sdk`);
the repositories that run your plugin never do.

## 6. Release

A plugin is released as the asset `<plugin>.wasm` of a GitHub release tagged
`plugin-<plugin>-v<version>`; here `todo.wasm` of `plugin-todo-v0.1.0`. A minimal workflow for a
repository that holds only the plugin:

```yaml
# .github/workflows/release.yml
name: Release
on:
  push:
    tags: ["plugin-todo-v*"]
permissions:
  contents: write
jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo test --locked
      - run: sh scripts/build-component.sh plugin-todo wasm32-wasip2
      - run: |
          cp target/wasm32-wasip2/release/plugin_todo.wasm todo.wasm
          sha256sum todo.wasm > todo.wasm.sha256
      - run: gh release create "$GITHUB_REF_NAME" todo.wasm todo.wasm.sha256 --title "$GITHUB_REF_NAME" --notes-file CHANGELOG.md
        env:
          GH_TOKEN: ${{ github.token }}
```

Build the plugin only in the release job, reproducibly, so the published file is the one the tests ran
on. `scripts/build-component.sh <package> wasm32-wasip2`, copied from Gate's repository, is how Gate
builds its own plugins: one package per cargo call (several at once would unify their dependencies'
features and change each module), with the checkout, the target directory and the cargo home remapped,
so that a rebuild of the tag in another checkout gives the same sha256. A rebuild needs the same build
host and runner image; Gate releases its plugins from `ubuntu-24.04`. `rust-toolchain.toml`
gives the job its toolchain and target.

### A Plugin Kept in the Repository It Checks

When a repository keeps its own plugin next to the code it checks, the pin moves in the release commit
itself. The release job:

1. builds the plugin with `scripts/build-component.sh` and computes its sha256;
2. runs `bin/gate lock --set todo=<release asset URL> --sha256 <sha256>`;
3. commits `gate.lock` as the release commit and tags it `plugin-todo-v<version>`;
4. uploads the same file as the tag's asset.

## 7. Pin It

```toml
[plugins.todo]
source = "your-org/plugin-todo:todo@0.1.0"
```

A plugin entry is always a table: `source` is `<owner>/<repo>:<plugin>@<version>`, next to the
optional `options`, `options-file` and `reads`.

```bash
bin/gate lock
```

`gate lock` resolves the source to `todo.wasm` of the release `plugin-todo-v0.1.0` of
`your-org/plugin-todo`, records its URL and sha256 in `gate.lock`, and every later run verifies it. A new
version is a new release and a new `gate lock`.

For a private repository, Gate authenticates with `$GATE_GITHUB_TOKEN`, else `$GITHUB_TOKEN`, else
`gh auth token`. Without a credential that can read the asset the run fails with exit code `2`, naming
the asset, the URL and the variable to set.

## ⚠️ Common Pitfalls

- **Reading a file that is not there**: `tree.text` and `tree.bytes` return `None` for an untracked
  file, an ignored file or a path outside the plugin's scope. Treat that as absent, not as an error.
- **Returning an error for a violation**: an `Err` from `check` fails the whole run as an environment
  failure. A violation is a finding.
- **Choosing a level in the plugin**: the level is the repository's choice in `gate.toml`. A plugin
  reports findings; presets suggest levels.
- **Reading the clock or the environment**: a plugin has neither. A rule that needs a date takes it as
  an option or uses the commit date from `git`.
- **Output that depends on order**: the core sorts findings, but a message that embeds an iteration
  order (of a hash map, say) changes the fingerprint between runs. Sort what you print.

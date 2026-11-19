# Contributing to Gate

Thank you for helping. This page says how a change gets into Gate: what to open first, how to build and
test, how to write commits, and what a pull request needs.

## 🧭 Before You Start

- **A bug**: open a [bug report](https://github.com/ElsiKora/Gate/issues/new?template=bug_report.yml)
  with the command, the configuration and the output.
- **A feature or a new rule**: open a
  [feature request](https://github.com/ElsiKora/Gate/issues/new?template=feature_request.yml) first, so
  the design is agreed before the code.
- **A vulnerability**: never in a public issue; follow [SECURITY.md](SECURITY.md).
- **A rule for your own conventions**: it does not need to be in this repository. Write it as a plugin
  in your own repository and pin it ([writing a plugin](docs/guide/writing-a-plugin.md)).

## 🏗 What Belongs Where

- The core (`crates/gate`, `crates/gate-host`) holds no rule and never names a language, a framework or
  a file layout. A rule lives in a plugin under `plugins/` and reads the tree only through the plugin
  interface.
- There is one way to do each thing: no aliases for old options, commands or rule ids, and no
  compatibility branches. A breaking change is a major version with a note in the changelog.
- Fixtures and test trees are synthetic and written for the test.

## 🛠 Development Setup

```bash
git clone https://github.com/ElsiKora/Gate.git
cd Gate
cargo build --release --locked
```

The toolchain is pinned in `rust-toolchain.toml`; `rustup` installs it on the first build. Building a
plugin also needs the WebAssembly target (`rustup target add wasm32-wasip2`).

Every pull request must pass:

```bash
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo deny --locked check
```

## 🧪 Tests

Test what would let a broken change through a gate or block a good one:

- each rule's pass and fail, as fixture trees run by `gate-test`;
- project selection and change selection;
- configuration errors, one per class;
- checksum verification failing closed;
- cache invalidation.

Wiring needs no test of its own. A bug fix comes with the test that failed before it.

## 📝 Code

- `unsafe` is forbidden; no `unwrap` outside tests.
- Errors say what is wrong and what to do: `gate.toml: [rules] "git/branch-name": unknown option
  "patern"; expected one of: pattern, ignore`.
- Public items of the SDK and of the plugin interface are documented; every rule has a page.
- English everywhere: code, comments, documents, commit messages.

## 🌿 Branches and Commits

Branches are named `<type>/<subject>`, the subject lowercase kebab:

| Type | For |
| --- | --- |
| `feature/` | new functionality |
| `bugfix/` | a fix of existing functionality |
| `hotfix/` | an urgent fix for a released version |
| `support/` | documentation, CI, maintenance |

Commits follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/). The scope is the
crate or plugin:

```text
feat(host): select dependent projects from the pnpm workspace graph
fix(plugin-git): accept a revert subject with a quoted original
docs(guide): explain exceptions in a monorepo
```

The message says what changed and why, and names the command behind any number in it. `feat` makes a
minor release, every other type a patch, and `BREAKING CHANGE:` in the body a major release; the
changelog is written from these messages.

## 🔀 Pull Requests

1. Branch from `main`, keep the change to one subject.
2. Run the four commands above; paste their result in the pull request if anything is unusual.
3. Update the guides and the rule pages your change affects.
4. Open the pull request with the template filled in.

A maintainer reviews every pull request before it merges. Expect one round of findings; the change
merges when they are resolved.

## 🔒 License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).

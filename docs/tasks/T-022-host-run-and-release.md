# T-022: Hook arguments for steps, tools per step, release notes

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.1.0 |
| Writer | the writer of T-016 |
| Reviewer | a separate review session |
| Depends on | T-016 (merged); T-014 merges first and also touches `crates/gate/src/main.rs` |
| Touches | `crates/gate-host/src/run`, `crates/gate-host/src/sync/{hooks,github}.rs`, `crates/gate-host/src/config`, `crates/gate/src/main.rs` (the `run` command only), `.github/workflows/release*.yml`, `scripts/changelog-section.sh`, `plugins/typescript/CHANGELOG.md`, `docs/architecture.md` sections 7 and 11, `docs/guide/{configuration,writing-a-plugin}.md` |

## Goal

A step started from a git hook sees what the hook received, `gate run` fetches only the tools the
steps it runs use, a local test server can serve tools, generated CI can read private plugin
releases, and a release's notes are its changelog section.

## Scope

1. **Hook arguments reach run steps.** `gate run <name> [-- <argument>...]` gives the arguments to
   every step as `$1`, `$2`, ...; `gate run --stdin` reads its standard input once and gives it to
   every step on its standard input (without `--stdin`, a step's standard input is empty, as now).
   `gate sync` writes every `run` entry of a hook as `bin/gate run <name> ... -- "$@"`, and in
   `pre-push` feeds it the pushed refs with `--stdin`. A `commit-msg` step reads the message file
   as `$1`.
2. **Tools per step.** A step names the tools it calls: `tools = ["<name>", ...]`, each a key of the
   root's `[tools]` (an unknown name is a configuration error at load). `gate run` installs a
   step's tools just before the step runs (fix round 3), and each step's `PATH` starts with its own
   tools only. A step that names no tool gets no tool. One rule, stated in
   architecture section 11; the first word of `run` decides nothing.
3. **Local tools.** A `[tools]` URL passes the same check as `gate lock`: `https`, or plain `http`
   to `127.0.0.1`, `localhost` or `[::1]`.
4. **Token for private plugins in generated CI.** The `bin/gate` steps of the workflow `gate sync`
   writes get `GATE_GITHUB_TOKEN: ${{ secrets.GATE_GITHUB_TOKEN || github.token }}` (fix round 2):
   a repository secret of that name, when set, reads private plugin releases; otherwise the job's
   token.
5. **Release notes from the changelog.** `scripts/changelog-section.sh <file> <version>` prints the
   section of `<version>` (an `#` or `##` heading whose first word is `<version>` or
   `[<version>](...)`, up to the next `#` or `##` heading) and fails when there is none or it is
   empty. `release.yml` puts the `CHANGELOG.md` section of the tag's version above the checksums;
   `release-plugin.yml` does the same with `plugins/<name>/CHANGELOG.md`. No fallback (lead,
   2026-11-18): `plugins/typescript/CHANGELOG.md` gets its `## 0.1.0` section like the other six
   plugins; the root `## 0.1.0` section is written at the v0.1.0 release.
6. `docs/guide/writing-a-plugin.md` names `scripts/build-component.sh` where it says to build
   reproducibly.

## Tests

- A step receives the arguments after `--` as `$1` (a `commit-msg` step reads the message file);
  `--stdin` reaches every step of a suite; without `--stdin` a step reads nothing.
- The generated `commit-msg` and `pre-push` hooks pass `"$@"`, and `pre-push` pipes the refs to
  `--stdin`.
- `gate run` asks for the tools of the planned steps only; a step's `PATH` holds its own tools and
  not another step's; a step naming an undeclared tool fails at load.
- `[tools]` accepts `http://127.0.0.1:<port>/...` and refuses `http://example.com/...`.
- The generated workflow's `GATE_GITHUB_TOKEN` falls back to `github.token`.
- `changelog-section.sh` finds a `#` and an `##` section and a plain `## <version>` heading, stops at
  the next release heading but not at `###`, and fails for a missing or empty section.

## Done when

- `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test
  --locked`, `cargo deny --locked check`, `shellcheck`, `actionlint` and the leak scan exit 0 on the
  pushed commit, rebased on main after the version bump.

## Review fix round (lead, 2026-11-18, review of `4919c1c`: merge after fixes)

1. The changelog comments say the lead writes a version's section before tagging;
   `plugins/ledger/CHANGELOG.md` has its `## 0.1.0` section. No fallback.
2. `GATE_GITHUB_TOKEN` is step environment of the `bin/gate` steps only, not workflow environment;
   `gate run` removes `GATE_GITHUB_TOKEN` and `GITHUB_TOKEN` from a step unless its `env` sets one.
3. A step's tools are installed just before it runs: a skipped step, or one after a fail-fast stop,
   installs nothing.
4. `changelog-section.sh` ignores headings inside fenced code blocks.
5. Tests: `[tools]` accepts `http://[::1]:<port>` and refuses `http://localhost.example` and
   `http://127.0.0.1@evil.example` (config tests, and `check_url`'s own); a step without `--stdin`
   reads nothing (`a_step_reads_nothing_from_the_input_of_gate`); a step sees no token.
6. The `bin/gate` steps get `GATE_BRANCH: ${{ github.head_ref || github.ref_name }}` as step
   environment; the host's branch is `$GATE_BRANCH` when set, else `HEAD`'s full ref without
   `refs/heads/` (`symbolic-ref --short` gives `heads/<name>` beside a tag of that name). Tested
   with such a tag. Architecture sections 8 and 10.

## Resume point

2026-11-18: branch `task/T-022-host-run-and-release`, rebased on main `86b7b04` (0.1.0-rc.1).
First version `4919c1c` reviewed: merge after fixes. Fix round written; next: the gates on the new
tip, then the report.

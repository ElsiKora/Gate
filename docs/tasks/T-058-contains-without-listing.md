# T-058: Answer `Tree::contains` and prefix listings without scanning the tree

| Field | Value |
| --- | --- |
| Status | done (merged at `cf97401`) |
| Release | v0.2.1 (core only; no interface, SDK or plugin change) |
| Writer | "Gate 003: host foundations" |
| Reviewer | a session that did not write it |
| Depends on | `main` at `9b18b32` |
| Touches | `crates/gate-runtime/src/grant.rs`, `crates/gate-runtime/src/state.rs`, `crates/gate-runtime/src/host.rs`, `crates/gate-host/src/plugins.rs`, `CHANGELOG.md` |

## Goal

This item has been open since rc.1: "`paths()` scans the snapshot per call and `Tree::contains` lists
per probe". `Tree::contains(path)` in the SDK is `list(path)` followed by an equality check. On the
host, `tree.paths(prefix)` walks every tracked path of the snapshot. For each one it checks the grant,
maps it to the plugin's form and compares it with the prefix. So each probe costs the whole tree. A
rule that probes once per project or per import costs (probes × tracked paths).

## Today

- SDK (`crates/gate-sdk/src/tree.rs`): `contains(path)` is
  `host.list(path).iter().any(|listed| listed == path)`. `under(dir)` is `list("dir/")` and `paths()`
  is `list("")`.
- WIT (`wit/gate.wit`): `tree.paths(prefix)`, `tree.read(path)` and `tree.changed()`. There is no
  existence question.
- Runtime (`crates/gate-runtime/src/state.rs`, `tree::Host::paths`): it filters
  `tree.paths()` (sorted, as the `Tree` trait documents) by `grant.allows`, maps each path with
  `grant.to_plugin` and keeps those that start with `prefix`. Then it sorts the result and records
  `grant.listed(prefix)` in the read set.
- Read set: `listed` holds the repository prefixes the listing covered. The result cache stores a
  digest of the tracked paths that start with each one (`cache/results.rs`, `listing`). So a probe
  of `package.json` from root `apps/api` records `apps/api/package.json`. Adding or removing any
  path that starts with that string re-runs the call. The content of the file does not.

## Design

The runtime answers `tree.paths(prefix)` with a range search of the sorted snapshot instead of a scan:

- **Root region.** A tracked path inside the root `R` has the plugin form `q` exactly when its
  repository path is `R/q`. That mapping prepends a fixed string, so it keeps order, and
  `q.starts_with(prefix)` holds exactly when `R/q` starts with `R/` + `prefix` (`prefix` alone when
  `R` is empty). The runtime binary-searches that string in `tree.paths()`, takes paths while they
  start with it, and maps each one with `to_plugin`.
- **Read regions.** A granted read path outside the root has a plugin form that starts with `..`.
  Only a prefix that could match such a form takes the scan as today: a prefix that starts with `.`
  and is compatible with `../`. `.github/` is not such a prefix, so it takes the fast path. A read
  path inside the root is already covered by the root region.
- **The read set is unchanged.** `grant.listed(prefix)` is recorded exactly as now, so a stored
  result is invalidated by exactly the same changes as before.
- **The answer is unchanged.** It is the same paths in the same sorted order.
- **The order is part of the contract.** `Tree::paths` is sorted by byte order (`str` order), each
  path once; the host's tree checks it with a `debug_assert` where it is built.
- **One public way to list.** `Grant::list` returns the paths; the scan and the counting entry are
  private, reached by the unit tests in the same file.

A tracked file named exactly as the root, with the prefix `""`, is left as is: git cannot track a
file and a directory at one path, so such a tree loses a path and never exposes one.

Nothing changes in the WIT, the SDK or any plugin: `contains`, `under` and `with_extensions` keep
calling `list`, and every built plugin gets the faster answer from a 0.2.1 core without a rebuild. A
probe then costs a binary search plus the paths that start with the probed string, usually the file
itself.

Two other designs are out, because each one breaks a constraint:
- **A new `tree.exists` import** changes `gate:plugin@0.2.0`. A plugin built with a new SDK would then
  fail to link on a 0.2.0 core, so it cannot ship in a patch release.
- **`contains` as `read(path).is_some()`** copies the bytes across the boundary. It also records the
  blob id, so an edit to the file would re-run the call, where today it does not. That is a
  different read set, not an equal one.

## Scope

1. `tree::Host::paths` in `gate-runtime` answers by a range search when the prefix cannot match a
   `..` form, and by the scan otherwise. A helper on `Grant` gives the repository prefix and says
   whether the fast path applies.
2. An equivalence unit test compares the fast answer with the scan's over this table (the read set
   is recorded by unchanged code, `grant.listed(prefix)`, so the answers are what is compared):
   - grants: the repository root, a nested root, a parent root, a root with a read path inside it,
     and a root with read paths outside it; each path in the range still passes the grant check.
     A grant has no exclusion (`allows` is "inside the root or a read path"), so no path inside
     `<root>/` can be refused; the nearest case, siblings that sort just before and just after the
     root's range (`apps/api-old`, `apps/api.d`, `apps/api2`), is in the table;
   - prefixes: `""` (also from the repository root), `src`, `src/`, `sr`, `.github/`, `.`, `..`,
     `../`, `../../docs/le`, `../../.github/`, `./x`, `a/../b`, `..f`, a path equal to a file, and
     the root's siblings (`api`, `api2`, `apps/api`, `../api2`).
3. A count, not a time: a unit test in `grant.rs` counts the paths compared for 1,000 `contains`
   probes on a synthetic tree of 20,000 paths, each comparison of the binary search counted inside
   its closure. Today that is all paths per probe; after the change it is the search's comparisons
   plus the paths in the range. The test asserts the bound and prints both counts; the task file
   records them with the command that printed them.
4. CHANGELOG `Unreleased`: a performance entry.

Not in scope: the full listing `paths("")`, which has to visit every path; the `gate-test` native
fixture (`Fixture::list` filters a `BTreeMap`, which costs the native tests only); a WIT change.

## Done when

- Tests, each failing with its change taken out:
  - the equivalence table, with the answers equal;
  - the count bound, which a linear `position` in place of `partition_point` breaks.
- `cargo fmt`, `clippy -D warnings`, `cargo test`, `cargo deny` pass in one lane round.
- The leak scan prints nothing.

## Proof

Both lane rounds ran on `cbca98d`, the tip with the review fixes.

- **Round ticket 253** (2026-11-19T22:50:20+00:00 to 22:59:50+00:00):
  - `cargo fmt --all --check` exit 0;
  - `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` exit 0;
  - `cargo deny --offline --locked check` exit 0 (advisories, bans, licenses and sources ok);
  - `cargo test --offline --locked --workspace --no-fail-fast` exit 101: 1015 passed, 1 failed. The
    failure was `a_message_past_the_read_set_limit_is_recorded_by_its_digest` in
    `crates/gate/tests/plugins.rs`, which could not read `git.wasm` while another test of the same
    binary was rebuilding that component. This branch does not touch that code; T-060 builds each
    component once per test binary;
  - the count, `cargo test -p gate-runtime --lib -- --exact
    grant::tests::a_probe_examines_the_paths_that_start_with_it_not_the_tree --nocapture`, exit 0:
    "1000 probes of 20000 paths: listing 18000 steps, scan 20000000 paths";
  - four mutations, each run against the test that must catch it, each exit 101: the range search
    taken out (the count test, 20000000), the scan for `../` prefixes taken out (the equivalence
    test), the root prefix taken out of the range (the equivalence test), and a linear `position` in
    place of `partition_point` (the count test, 18752500).
- **Rerun ticket 258** (2026-11-19T23:38:15+00:00 to 23:45:21+00:00), the failed target alone:
  `cargo test --offline --locked -p gate --test plugins` exit 0, 81 passed, 0 failed.

The leak scan printed nothing and exited 0 before each commit.

## Resume point

2026-11-19T23:46:16+00:00: the review fixes are proven by round 253 and rerun 258 on `cbca98d`
(see Proof). Ready for the lead to merge; nothing open.

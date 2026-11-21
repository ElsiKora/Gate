# T-085: Release-candidate message fixes

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | 0.3.0 |
| Writer | the T-066 writer |
| Reviewer | the lead |
| Depends on | `main` at `e008106` (T-069, T-076 and T-087 merged) |
| Touches | `crates/gate`, `crates/gate-host` (fetch, lock, plugins), `plugins/git`, `plugins/docs`, `plugins/docker`, their tests, rule pages, guides and CHANGELOGs |

## Goal

Fix the messages that the release-candidate trial found (T-083, `docs/tasks/T-083-rc-trial.md` on
its branch). These are findings 3, 4, 5, 6, 9, 13, 16, 17 and 20:

1. **Presets in `gate list` and `gate explain` (3).**
   - Both show each rule a preset turns on. The row is marked `(preset <plugin>/<preset>)`, and its
     origin is the `extends` entry.
   - The presets are read from what each declared plugin declares: its stored declaration, else
     the plugin from the cache, loaded without running. Nothing is downloaded: a plugin the cache
     does not hold is named, with `gate sync` as the way to fetch it.
   - Both commands take `--plugin <name>=<file.wasm>`.
   - When the declarations cannot be read, for example with no `gate.lock`, the output says why on
     its last line, and JSON gives the reason as `presets-error`.
2. **An unknown preset (4).** The error lists the plugin's presets, for example `its presets are
   frontend/adoption and frontend/recommended`, instead of sending the reader to `gate list`.
3. **An option value outside an enum (5).** The error names the allowed values: `/pack: "next" is
   not one of "react-vite", "nextjs" or "react-library"`. This holds for any `enum`, and for any
   `anyOf` or `oneOf` whose every branch is a constant or `null`. Other errors keep the
   validator's own message.
4. **`gate lock` on releases that do not exist (6).**
   - When the release API answers 404 and the repository itself answers with the same
     credential, the release does not exist. That is a configuration error that names the
     repository's releases page and does not blame the credential.
   - `gate lock` names every missing release in one error and writes nothing.
   - When the repository does not answer either, the message about the credential stays as it
     was.
5. **`gate lock --set` for several plugins (9).**
   - Repeat `--set` and `--file`: the n-th `--file` goes with the n-th `--set`.
   - Unequal counts are refused, and so is a plugin named twice.
   - Every pin is checked before any build enters the cache or `gate.lock` is written.
6. **A header without the form (13).**
   - `git/commit-message` reports `wip` once, as `type-empty`, with the message `the header "wip"
     does not have the form <type>(<scope>): <subject>`.
   - `scope-empty` and `subject-empty` judge only a header that matches the header pattern.
   - This departs from commitlint, which gives three findings for it. The rule page says so.
7. **`docs/layout` (16).**
   - `#unknown` gets a help line that lists the kind's entries and how to declare one in `extra`.
   - The absent sections of one index are one `#sections` finding (key `<index>#missing`). It
     names them in the standard's order.
   - Both `#sections` findings get a help line with the order.
8. **The docker stage name (17).** An unnamed runtime stage is `the unnamed runtime stage 1`, not
   `the runtime stage stage 1`.
9. **`gate/budget` (20).**
   - The finding already had a help line, printed in the text, JSON, SARIF and GitHub formats;
     the code shows no output that drops it. The test now asserts that it is printed.
   - The help names the slow step that `--timings` shows, not a slow plugin. It does not name the
     plugin compile of a first run: T-086 stops counting that compile toward the budget.

## Done when

- Each user-visible message change has a test:
  - list and explain with a preset, the unknown preset and the unknown pack (CLI, with the built
    frontend plugin);
  - the enum message (unit);
  - missing releases on a readable repository and on an unreadable one (host, with the HTTP
    fixture);
  - several `--set` pins, unequal counts and a duplicate (CLI);
  - the `wip` header (git);
  - the ordered sections and the help lines (docs);
  - the unnamed stage (docker);
  - the budget help line (CLI).
- The rule pages, README, guides and CHANGELOGs say what changed.
- One lane round on T-085's own target directories, with hooks off in the round worktree. These
  must exit 0: fmt, clippy, test (`--workspace --no-fail-fast`), deny and the release build. The log
  must show every changed crate compiled from T-085's checkout, with every new test listed. The
  leak scan prints nothing.

## Resume point

2026-11-21T04:48:37+00:00: the review's fix round is written.
- The budget help drops the plugin compile; T-086 stops counting it.
- The offline preset check uses `load`'s own test of a stored declaration. A new test covers a
  stored declaration `load` would not use, with no build in the cache.
- `lock --set` hashes every file again under the guard before it admits any.
- The rule page says a `wip` header passes with `#type-empty` off.
- The doc comments of `PresetSetting` and `scopes` are separate.
- The layout message says "lacks the sections".

Next: one lane round.

2026-11-21T04:41:36+00:00: lane ticket 417 ran on `ecc59ee` (rebased onto `e008106`) and exited 0
after 962 s. fmt, clippy, the workspace tests (1514 passed, 0 failed), deny and the release build
each exited 0. The log check found no workspace crate missing. `gate`, `gate-host`,
`gate-plugin-git`, `gate-plugin-docs` and `gate-plugin-docker` were compiled from the round
worktree, and each new test is listed as `ok`. The docs cases are in existing table tests. Next:
the lead's review.

2026-11-21T03:06:07+00:00: rebased onto `main` at `e008106`. Ticket 408 on the old base was
withdrawn while its tests ran. Two conflicts were resolved by keeping both sides:
- `crates/gate/src/distribution.rs`: the several-pin `--set` keeps T-076's guard, taken after the
  files are hashed and before `gate.lock` is read;
- `docs/guide/plugins.md`: main's presets paragraph and example, then the paragraph on `gate list`,
  `gate explain` and the unknown preset.

Next: one lane round on the rebased tip.

2026-11-21T01:39:44+00:00: ticket 400 ran on `a7e8440`. fmt, clippy and deny exited 0. The tests
ran 1489, 4 failed:
- three list and explain tests: the fixture's `gate.lock` pins at `example.invalid`, and `gate
  list` tried to download a plugin to read its presets. The presets are now read from the cache
  only, never downloaded, and a plugin the cache does not hold is named, with `gate sync` as the
  way to fetch it;
- one docs test that still expected the per-section key.

Next: one more lane round.

2026-11-21T00:08:12+00:00: the changes and tests are written, and rustfmt is clean. Next: commit,
then one lane round.

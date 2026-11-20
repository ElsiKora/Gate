# T-081: Preparing the release cut

| Field | Value |
| --- | --- |
| Status | done; ready for the lead; merges at the cut, after the last code task |
| Release | 0.3.0 |
| Writer | the T-066 writer |
| Reviewer | the lead |
| Depends on | `main` at `18c0bf8` (T-078 merged) |
| Touches | `scripts/release-check.sh` (new), `docs/guide/plugins.md` (one example pin), this file; no version or changelog heading |

## Goal

1. **A release check.** `scripts/release-check.sh <version> [--keep <plugin>]...` prints every
   place where a version must agree at a release, and exits 1 when one disagrees with
   `<version>`. It reads tracked files only, with no network and no cargo. It checks:
   - the workspace version and every workspace member's version;
   - the members in `Cargo.lock`;
   - the top version heading of the root changelog and of each plugin's changelog;
   - in the documents: the pins `ElsiKora/Gate:<plugin>@<v>`, the release tags of the repository
     (`releases/download/v<v>/`, which includes the schema URL, `tag = "v<v>"` and
     `plugin-<plugin>-v<v>`), the core version an example pins, and the release README calls
     current;
   - the plugin interface in `wit/gate.wit`, which must be `<major>.<minor>.0`, and every
     `gate:plugin@<v>` named in the documents and in the runtime's `INTERFACE`.

   `--keep` holds a plugin that is not released with the core to its own version. Gate's own
   `gate.toml` pins released assets, so it moves after the release; those pins are printed as
   `later`.
2. **The places that change at the 0.3.0 cut**, listed below. No version or heading changes yet.
3. **Example pins that mix versions wrongly today**, fixed with a reason each.

## Proof

All runs were on this branch, from the repository root.

- `shellcheck scripts/release-check.sh` and `sh -n` are clean.
- `./scripts/release-check.sh 0.3.0` exits 1: `91 of 140 places disagree with 0.3.0`.
- `--keep ledger` holds every ledger place to 0.2.0: the manifest, the lock, the heading and five
  document pins, all `ok`.
- A target that is not `X.Y.Z` (`0.3`) and `--keep` of a plugin that does not exist both exit 2,
  with a message.
- Proof that the reported places are exact:
  - In a scratch clone, only the reported lines were changed (each found version replaced by the
    wanted one, each headingless top heading made `# 0.3.0 (unreleased)`). The members that
    inherit the workspace version were not touched.
  - The check then printed `140 places agree with 0.3.0` and exited 0.
- A scan of every fenced example block in the documents for pins of both 0.2.x and 0.3.x, or a
  0.2.x core beside a 0.3.x plugin, found one block. It is fixed below.

## Example pins fixed now

- `docs/guide/plugins.md`, the `frontend` example: `typescript@0.2.0` becomes `typescript@0.3.0`,
  beside `frontend@0.3.0`. Reason: a 0.3.0 plugin is built for `gate:plugin@0.3.0` and loads only in
  a 0.3.0 core, where a plugin built for 0.2.0 does not load (root CHANGELOG, 0.3.0, sdk). That
  file could not run in any core. The `cdk` example already pins `typescript@0.3.0`.

The other examples pin only 0.2.x plugins with a 0.2.1 core, which is right today. They move at
the cut and are in the list.

## Places that change at the 0.3.0 cut

These are the output of `./scripts/release-check.sh 0.3.0` on this branch, with every plugin
released at 0.3.0. A plugin given `--keep` at the cut drops out of this list.

- The members marked "the workspace's" change with the one line in `Cargo.toml`.
- The `Cargo.lock` lines change with the next `cargo` run after the bump.
- Each headingless changelog (`## Unreleased`) gets `# 0.3.0 (unreleased)`, then its date at the
  cut.

**Cargo manifests** (18):

- `Cargo.toml:10`: workspace version, 0.2.1 to 0.3.0
- `crates/gate/Cargo.toml:8`: gate version (the workspace's), 0.2.1 to 0.3.0
- `crates/gate-host/Cargo.toml:8`: gate-host version (the workspace's), 0.2.1 to 0.3.0
- `crates/gate-runtime/Cargo.toml:8`: gate-runtime version (the workspace's), 0.2.1 to 0.3.0
- `crates/gate-runtime/tests/probe/Cargo.toml:8`: gate-runtime-probe version (the workspace's), 0.2.1 to 0.3.0
- `crates/gate-sdk/Cargo.toml:8`: gate-sdk version (the workspace's), 0.2.1 to 0.3.0
- `crates/gate-test/Cargo.toml:8`: gate-test version (the workspace's), 0.2.1 to 0.3.0
- `examples/plugin-hello/Cargo.toml:8`: gate-plugin-hello version (the workspace's), 0.2.1 to 0.3.0
- `languages/typescript/Cargo.toml:8`: gate-typescript version (the workspace's), 0.2.1 to 0.3.0
- `plugins/agents/Cargo.toml:8`: gate-plugin-agents version, 0.2.0 to 0.3.0
- `plugins/agents/format/Cargo.toml:8`: gate-agents-format version (the workspace's), 0.2.1 to 0.3.0
- `plugins/cdk/Cargo.toml:8`: gate-plugin-cdk version (the workspace's), 0.2.1 to 0.3.0
- `plugins/git/Cargo.toml:8`: gate-plugin-git version, 0.2.1 to 0.3.0
- `plugins/ledger/Cargo.toml:8`: gate-plugin-ledger version, 0.2.0 to 0.3.0
- `plugins/markdown/Cargo.toml:8`: gate-plugin-markdown version, 0.2.0 to 0.3.0
- `plugins/nestjs/Cargo.toml:8`: gate-plugin-nestjs version, 0.2.1 to 0.3.0
- `plugins/node/Cargo.toml:8`: gate-plugin-node version (the workspace's), 0.2.1 to 0.3.0
- `plugins/typescript/Cargo.toml:8`: gate-plugin-typescript version, 0.2.0 to 0.3.0

**Cargo.lock** (17):

- `Cargo.lock:812`: gate locked, 0.2.1 to 0.3.0
- `Cargo.lock:823`: gate-agents-format locked, 0.2.1 to 0.3.0
- `Cargo.lock:832`: gate-host locked, 0.2.1 to 0.3.0
- `Cargo.lock:852`: gate-plugin-agents locked, 0.2.0 to 0.3.0
- `Cargo.lock:864`: gate-plugin-cdk locked, 0.2.1 to 0.3.0
- `Cargo.lock:908`: gate-plugin-git locked, 0.2.1 to 0.3.0
- `Cargo.lock:921`: gate-plugin-hello locked, 0.2.1 to 0.3.0
- `Cargo.lock:930`: gate-plugin-ledger locked, 0.2.0 to 0.3.0
- `Cargo.lock:939`: gate-plugin-markdown locked, 0.2.0 to 0.3.0
- `Cargo.lock:949`: gate-plugin-nestjs locked, 0.2.1 to 0.3.0
- `Cargo.lock:959`: gate-plugin-node locked, 0.2.1 to 0.3.0
- `Cargo.lock:979`: gate-plugin-typescript locked, 0.2.0 to 0.3.0
- `Cargo.lock:988`: gate-runtime locked, 0.2.1 to 0.3.0
- `Cargo.lock:1002`: gate-runtime-probe locked, 0.2.1 to 0.3.0
- `Cargo.lock:1010`: gate-sdk locked, 0.2.1 to 0.3.0
- `Cargo.lock:1021`: gate-test locked, 0.2.1 to 0.3.0
- `Cargo.lock:1030`: gate-typescript locked, 0.2.1 to 0.3.0

**Changelog top headings** (6):

- `plugins/cdk/CHANGELOG.md:6`: top heading, none to 0.3.0
- `plugins/docker/CHANGELOG.md:5`: top heading, none to 0.3.0
- `plugins/ledger/CHANGELOG.md:11`: top heading, 0.2.0 to 0.3.0
- `plugins/markdown/CHANGELOG.md:5`: top heading, 0.2.0 to 0.3.0
- `plugins/repository/CHANGELOG.md:5`: top heading, none to 0.3.0
- `plugins/typescript/CHANGELOG.md:5`: top heading, none to 0.3.0

**Documents** (50):

- `README.md:17`: current-release, 0.2.1 to 0.3.0
- `README.md:208`: core-install, 0.2.1 to 0.3.0
- `README.md:254`: core-pin, 0.2.1 to 0.3.0
- `README.md:257`: pin markdown, 0.2.0 to 0.3.0
- `README.md:260`: pin git, 0.2.1 to 0.3.0
- `README.md:310`: core-pin, 0.2.1 to 0.3.0
- `README.md:317`: pin git, 0.2.1 to 0.3.0
- `README.md:332`: pin typescript, 0.2.0 to 0.3.0
- `README.md:341`: pin nestjs, 0.2.1 to 0.3.0
- `docs/architecture.md:267`: core-tag, 0.2.1 to 0.3.0
- `docs/architecture.md:270`: core-pin, 0.2.1 to 0.3.0
- `docs/architecture.md:277`: pin git, 0.2.1 to 0.3.0
- `docs/architecture.md:281`: pin markdown, 0.2.0 to 0.3.0
- `docs/architecture.md:310`: pin typescript, 0.2.0 to 0.3.0
- `docs/architecture.md:311`: pin nestjs, 0.2.1 to 0.3.0
- `docs/architecture.md:312`: pin ledger, 0.2.0 to 0.3.0
- `docs/architecture.md:313`: pin agents, 0.2.0 to 0.3.0
- `docs/guide/configuration.md:28`: core-tag, 0.2.1 to 0.3.0
- `docs/guide/configuration.md:48`: core-pin, 0.2.1 to 0.3.0
- `docs/guide/configuration.md:100`: pin typescript, 0.2.0 to 0.3.0
- `docs/guide/configuration.md:104`: pin git, 0.2.1 to 0.3.0
- `docs/guide/configuration.md:141`: pin ledger, 0.2.0 to 0.3.0
- `docs/guide/configuration.md:209`: pin typescript, 0.2.0 to 0.3.0
- `docs/guide/configuration.md:210`: pin nestjs, 0.2.1 to 0.3.0
- `docs/guide/configuration.md:220`: pin typescript, 0.2.0 to 0.3.0
- `docs/guide/getting-started.md:19`: core-install, 0.2.1 to 0.3.0
- `docs/guide/getting-started.md:47`: core-pin, 0.2.1 to 0.3.0
- `docs/guide/getting-started.md:50`: pin markdown, 0.2.0 to 0.3.0
- `docs/guide/getting-started.md:53`: pin git, 0.2.1 to 0.3.0
- `docs/guide/monorepo.md:31`: core-pin, 0.2.1 to 0.3.0
- `docs/guide/monorepo.md:38`: pin git, 0.2.1 to 0.3.0
- `docs/guide/monorepo.md:52`: pin typescript, 0.2.0 to 0.3.0
- `docs/guide/monorepo.md:73`: pin nestjs, 0.2.1 to 0.3.0
- `docs/guide/plugins.md:13`: pin git, 0.2.1 to 0.3.0
- `docs/guide/plugins.md:28`: plugin-tag git, 0.2.1 to 0.3.0
- `docs/guide/plugins.md:61`: pin typescript, 0.2.0 to 0.3.0
- `docs/guide/plugins.md:82`: pin git, 0.2.1 to 0.3.0
- `docs/guide/plugins.md:108`: pin markdown, 0.2.0 to 0.3.0
- `docs/guide/plugins.md:163`: pin agents, 0.2.0 to 0.3.0
- `docs/guide/plugins.md:194`: pin ledger, 0.2.0 to 0.3.0
- `docs/guide/plugins.md:226`: pin typescript, 0.2.0 to 0.3.0
- `docs/guide/plugins.md:227`: pin nestjs, 0.2.1 to 0.3.0
- `docs/guide/writing-a-plugin.md:58`: core-tag, 0.2.1 to 0.3.0
- `docs/guide/writing-a-plugin.md:62`: core-tag, 0.2.1 to 0.3.0
- `docs/guide/writing-a-plugin.md:134`: core-tag, 0.2.1 to 0.3.0
- `plugins/agents/rules/instruction-references.md:50`: pin agents, 0.2.0 to 0.3.0
- `plugins/git/rules/branch-name.md:85`: pin git, 0.2.1 to 0.3.0
- `plugins/git/rules/main-history.md:133`: pin git, 0.2.1 to 0.3.0
- `plugins/ledger/rules/README.md:55`: pin ledger, 0.2.0 to 0.3.0
- `plugins/ledger/rules/blockers.md:28`: pin ledger, 0.2.0 to 0.3.0

**Later**, after the release assets exist (`gate lock` must hash them), in Gate's own `gate.toml`:

- `gate.toml:2`: version = "0.2.1"
- `gate.toml:5`: source = "ElsiKora/Gate:git@0.2.1"
- `gate.toml:11`: agents = { source = "ElsiKora/Gate:agents@0.2.0" }
- `gate.toml:12`: markdown = { source = "ElsiKora/Gate:markdown@0.2.0" }
- `gate.toml:13`: ci = { source = "ElsiKora/Gate:ci@0.2.0" }

## Resume point

2026-11-20T20:21:59+00:00: the script, the list and the one example pin are written and proved as
above. Next: the lead's read. At the cut: rebase onto the last `main`, run
`./scripts/release-check.sh 0.3.0` again, make every change it lists, and see it exit 0.

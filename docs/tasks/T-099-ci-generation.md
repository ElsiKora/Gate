# T-099: the generated CI keeps its cache, cancels superseded branch runs, and names its aggregate

| Field | Value |
| --- | --- |
| Status | in progress (design approved) |
| Release | the next core release (additive; the plugin interface stays `gate:plugin@0.3.0`) |
| Writer | "Gate T-099" |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | `main` at `df718c0` (`v0.3.1` released) |
| Touches | `crates/gate-host/src/sync/github.rs`, the `[ci.github]` keys in `crates/gate-host/src/config`, their tests, `docs/guide/configuration.md`, `docs/guide/writing-a-plugin.md`, `docs/plan.md`, this task file |

## Goal

A consumer with CI time bounds asked for three changes to what `gate sync` generates, and named a
window in the documented release order of a plugin kept in the repository it checks:

1. Every generated job starts on a cold Gate cache: nothing restores `$GATE_CACHE_DIR`, so the core,
   the plugins, the compiled modules, the result cache and the git answers are fetched or computed
   again in every job of every run.
2. `cancel-in-progress` is `${{ github.event_name == 'pull_request' }}`: a repository that pushes
   branches and opens no pull requests never cancels a superseded run.
3. The `aggregate` job's display name is its id.
4. `writing-a-plugin.md` commits the pin before the asset is uploaded, so `main` can pin an asset that
   does not exist yet.

## Design

The lead approved the design of `9d1f760` with changes: the cache is opt-in, the core version leaves
the key, the guide states the cache's trust boundary, and the measurement runs on a branch of its own
that is deleted afterwards. This section is the design as approved.

### 1. The Gate cache between runs: `[ci.github] cache = true`

Off by default; without the key the workflow keeps the bytes it has without this change. With it,
every generated job that runs `bin/gate` (`changes` and each job of `[ci.github.jobs]`; not the
aggregate job, which runs only `jq`) gets two steps of `actions/cache`, pinned by commit as core
constants (`actions/cache/restore@55cc834... # v6.1.0` and `actions/cache/save@55cc834... # v6.1.0`):

- `Restore the Gate cache` (`id: gate-cache`), right after the checkout, before `setup`;
- `Save the Gate cache`, last, with
  `if: ${{ !cancelled() && steps.gate-cache.outputs.cache-hit != 'true' }}` and the key the restore
  step tried (`steps.gate-cache.outputs.cache-primary-key`): a red check still saves, a cancelled or
  timed-out job does not, and a rerun of the same commit does not save twice.

The directory is `${{ runner.temp }}/gate-cache`, given to each `bin/gate` step as `GATE_CACHE_DIR`
beside `GATE_GITHUB_TOKEN` and `GATE_BRANCH`; no other step gets it. `runner.temp` is emptied at the
start and end of every job, also on a self-hosted runner. GitHub does not allow the `runner` context
in a job's `env`, which is why the variable sits on the steps.

```text
key:          gate/${{ runner.os }}/${{ hashFiles('gate.lock') }}/<job id>/${{ github.sha }}
restore-keys: gate/${{ runner.os }}/${{ hashFiles('gate.lock') }}/<job id>/
              gate/${{ runner.os }}/${{ hashFiles('gate.lock') }}/
```

`/` separates the parts, since no job id holds one. A GitHub cache entry never changes once saved, so
the commit gives each commit its own entry; the job id keeps two jobs of one run from racing for one
key. `gate.lock` pins the core archives, so its hash moves with every core upgrade and the key needs
no version of its own: `gate sync` does not have to follow `gate lock`.

The lead asked for a third restore-key, `gate/<os>/`, if every derived part is keyed internally by
core and plugin sha256. The check found them keyed this way:

| Part | Keyed by |
| --- | --- |
| `results/<plugin sha256>/` | the result format, the core's version (`CARGO_PKG_VERSION`), the plugin's sha256, the request and the grant (`cache/results.rs`, `key` and `generate_key`) |
| `declarations/<plugin sha256>/` | the format, the plugin interface and the core's version (`plugins/declarations.rs`, `key_for`) |
| `modules/` | wasmtime's own key: the wasm bytes, the engine and compiler settings, the wasmtime version |
| `git/` | the full object ids asked about, which answer the same under any core; only the format is versioned |

Results and declarations are keyed by the core's version, not its sha256, so an entry is wrong only
for two different core builds of one version: a replaced release archive pinned again with
`gate lock`. The writer stopped and reported it; without `gate/<os>/` every restored entry was saved
under the same `gate.lock`, so the same core archive and plugins. The lead chose to leave
`gate/<os>/` out: a change to `gate.lock` starts one cold run per job, and keying results and
declarations by the core's sha256 is a possible backlog item, not for this release.

What is cached:

| Under `$GATE_CACHE_DIR` | Cached | Why |
| --- | --- | --- |
| `blobs/sha256/` (plugins, tool archives) | yes | hashed again on every read against its name, the sha256 `gate.lock` pins; one that does not match is removed and fetched again |
| `modules/`, `results/`, `declarations/`, `git/` | yes | derived; trusted as restored, which is why the cache is opt-in |
| `core/` | **never** | `bin/gate` checks the unpacked core against a sha256 it recorded in the cache itself, not against `gate.lock`; a restored `core/<sha>/gate` with a matching `gate.sha256` would run |
| `tools/` | **never** | the same: the unpacked tool is checked against a record in the cache |
| `tmp/` | no | downloads in progress |

The `path` lists the five cached parts rather than excluding the others, so a part added to the cache
later is not restored until someone decides it may be.

The trust boundary, as the guide states it: GitHub scopes an entry to the branch whose run saved it,
and every branch can read the default branch's entries; so any step of any job on a branch can write
an entry that later runs of that branch trust. A pull request from a fork cannot write the base
branch's entries.

### 2. Cancelling superseded branch runs

```yaml
cancel-in-progress: ${{ github.ref != format('refs/heads/{0}', github.event.repository.default_branch) }}
```

The group stays `<workflow>@${{ github.ref }}`; no setting. A pull request run and a push to any
branch but the default one cancel the run of the same ref still in progress, `release/*` and every
other long-lived branch included; a push to the default branch never cancels a running check there.
The aggregate job of a cancelled run shows red on the superseded commit.

### 3. The aggregate job's display name

`[ci.github] aggregate-name`: the aggregate job's `name:`, which branch protection then requires.
Without it the name is the id, byte for byte as before. Configuration errors at load (exit 2):
`[ci.github] aggregate-name is set without aggregate; set aggregate to the job's id, or remove aggregate-name`,
and `[ci.github] aggregate-name is empty; name the aggregate job or remove the key`.

### Where a configuration's generated workflow changes bytes

| Change | Which configurations | Bytes |
| --- | --- | --- |
| 2 | every `[ci.github]` | the one `cancel-in-progress` line |
| 1 | only one with `cache = true` | each `bin/gate` job gains the two cache steps and `GATE_CACHE_DIR` on each `bin/gate` step |
| 3 | only one with `aggregate-name` | the aggregate job's `name:` |

So upgrading to this core needs one `gate sync`, for the `cancel-in-progress` line, as T-043's
concurrency group did; a test pins that the line is the only difference from the workflow 0.3.0 wrote.

### 4. The release order of a plugin kept in the repository it checks (documents only)

The asset is published first and the pin committed after it. A rerun never replaces an asset: the same
sha256 continues, a different one fails.

## Measurement

Gate's own CI runs the released core `gate.lock` pins (0.3.0), which reads neither `cache` nor the new
workflow. The measurement ran on a branch of its own, `task/T-099-ci-measure`, cut from `3d70ac9`: its
workflow is the one this branch's core (`target/debug/gate`, built from `294a67a`) writes for Gate's
`gate.toml` with `cache = true` and the branch added to `branches`, and its committed `gate.toml` leaves
`cache` out so that the released core can read it. Its `changes` job fails at `sync --check`, as
expected (the released core reports the new workflow as drift), and so does the aggregate; the `gate`
job, which lists no projects, runs either way. Each measuring commit changes one document, so the
`gate` job's `check --push` runs all 7 rules on project `.`. Before the cold run every cache entry of
the branch was deleted (`gh cache delete`). Times are from the jobs' API timestamps and, for the step,
from its log lines (`gh run view <run> --log --job <job>`).

| Run | Run id | Commit | `Restore the Gate cache` | `gate check --push` step | `Gate` job |
| --- | --- | --- | --- | --- | --- |
| cold | 45121494956 | `77901f0` | `Cache not found` | 6.61 s (06:02:32.758 to 06:02:39.372) | 13 s (06:02:29 to 06:02:42) |
| warm | 45121572438 | `d069ccf` | restored the cold run's `gate/Linux/59670ce.../gate/77901f0...`, 11.2 MB in 1.5 s | 0.66 s (06:03:19.556 to 06:03:20.215) | 10 s (06:03:14 to 06:03:24) |

Both runs' `gate check --push` printed `7 rules, 0 errors, 0 warnings; projects: ran .`. The step is
ten times faster warm; for a repository as small as Gate, restoring and saving an 11 MB entry (about
3 s together) takes most of what the job saves, so the gain grows with the plugins' compile time and
the tree. An entry of Gate's `gate` job is 11 to 21 MB (`gh cache list`).

Seen on the way, and recorded because they bear on the design:

- Run 45120749553 (`479618d`): the `gate` job's checkout failed to reach github.com for 5 minutes and
  retried; by then the `changes` job had saved its entry, and the `gate` job restored it through the
  second restore-key (`gate/<os>/<lock>/`, any job): `Cache hit for restore-key: .../changes/479618d...`.
- Run 45121377078 (`5977290`) was `cancelled` by the push of `bdb6acb` a few seconds later: a
  superseded run of a branch that is not the default one, cancelled by the new `cancel-in-progress`.

The branch was deleted on the remote afterwards (`git ls-remote --heads origin task/T-099-ci-measure`
prints nothing), and its cache entries with it (`gh cache list --ref refs/heads/task/T-099-ci-measure`
prints `[]`). Nothing of it is merged.

## Done when

- The lead approved this design (done, with the changes above).
- The generated YAML is tested: the cache steps, their key and paths (exactly `blobs`, `modules`,
  `results`, `declarations`, `git`), the core and tools left out,
  `GATE_CACHE_DIR` on every `bin/gate` step and on no other, the save condition, the new
  `cancel-in-progress`, `aggregate-name` set and unset (unset: the aggregate job as 0.3.1 wrote it),
  the configuration error, `gate sync --check` drift; `ci/recommended` passes on the new workflow.
- A test copies a filled cache to a new directory, as a restore does, tampers the plugin there, and
  shows that the run never loads the tampered file: it is refused and fetched again, or the run fails
  closed when the asset cannot be fetched.
- The guide's `[ci.github]` section, its concurrency text and the release order are written.
- Every command of `AGENTS.md` exits 0 through the lane; the leak scan prints nothing.
- The cold and warm timings are recorded with their run ids, and the measurement branch is deleted on
  the remote; the branch is pushed.

## Proof

## Resume point

2026-11-23T05:55Z: design approved; code and tests in `294a67a`, guide written; a focused lane round
(clippy, the touched tests) is queued. Waiting for the lead on the `gate/<os>/` restore-key. Next:
the full lane round, the measurement branch, then the report.

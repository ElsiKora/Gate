# T-099: the generated CI keeps its cache, cancels superseded branch runs, and names its aggregate

| Field | Value |
| --- | --- |
| Status | design, waiting for the lead |
| Release | the next core release (additive; the plugin interface stays `gate:plugin@0.3.0`) |
| Writer | "Gate T-099" |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | `main` at `df718c0` (`v0.3.1` released) |
| Touches | `crates/gate-host/src/sync/github.rs`, the `[ci.github]` keys in `crates/gate-host/src/config`, their tests, `docs/guide/configuration.md`, `docs/guide/writing-a-plugin.md`, `CHANGELOG.md` (unreleased section), `docs/plan.md`, this task file |

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

### 1. The Gate cache between runs

Every generated job that runs `bin/gate` (`changes` and each job of `[ci.github.jobs]`; not the
aggregate job, which runs only `jq`) gets two steps from `actions/cache`, pinned by commit as
`checkout` is (`actions/cache/restore@<sha> # v6.1.0` and `actions/cache/save@<sha> # v6.1.0`, the
same commit; the pin is a constant of the core, like the generated `jq` line, not a setting):

- `Restore the Gate cache` (`id: gate-cache`), right after the checkout, before `setup`;
- `Save the Gate cache`, last, after `after`, with `if: ${{ !cancelled() && steps.gate-cache.outputs.cache-hit != 'true' }}`:
  a red check still saves (its results are as valid as a green one's), a cancelled or timed-out job
  does not, and a rerun of the same commit does not save twice.

The directory is `${{ runner.temp }}/gate-cache`, given to each `bin/gate` step as `GATE_CACHE_DIR`
in its `env` (beside `GATE_GITHUB_TOKEN` and `GATE_BRANCH`; other steps get none of them, as today).
`runner.temp` is emptied at the start and end of every job, also on a self-hosted runner, so what a
job finds there is what the restore step put there and nothing else. GitHub does not allow the
`runner` context in a job's `env`, which is why the variable sits on the steps.

The key, with `/` between the parts since no job id holds one:

```text
key:          gate/${{ runner.os }}/<core>/${{ hashFiles('gate.lock') }}/<job id>/${{ github.sha }}
restore-keys: gate/${{ runner.os }}/<core>/${{ hashFiles('gate.lock') }}/<job id>/
              gate/${{ runner.os }}/<core>/${{ hashFiles('gate.lock') }}/
              gate/${{ runner.os }}/<core>/
```

`<core>` is `[gate] version` of the root `gate.toml`, the core `gate.lock` pins. A GitHub cache entry
never changes once saved, so a key of only the OS, the core and the lock would keep the result cache
of the first commit forever; the commit at the end gives each commit its own entry, and the
restore-keys take the newest entry of the same job and lock, else of any job of that lock (a new job
still gets the core's compiled plugins), else of any lock of that core (a plugin bump keeps the
others warm). The job id keeps two jobs of one run from racing for one key and restores each job the
results of its own commands. A pull request run restores from its base branch, a branch push from
its branch and then the default branch, by GitHub's own cache scoping.

What is cached, and why each part needs no trust:

| Under `$GATE_CACHE_DIR` | Cached | Why |
| --- | --- | --- |
| `blobs/sha256/` (plugins, tool archives) | yes | each blob is hashed again on every read and must hash to its name, the sha256 `gate.lock` pins; one that does not is removed and fetched again |
| `modules/`, `results/`, `declarations/`, `git/` | yes | derived: see the trust boundary below |
| `core/` | **no** | `bin/gate` checks the unpacked core against a sha256 it recorded in the cache itself at install time, not against `gate.lock`; a restored `core/<sha>/gate` with a matching `gate.sha256` would run. The core is downloaded and checked against `gate.lock` in every job (11.5 MB for `x86_64-unknown-linux-musl` 0.3.1) |
| `tools/` | **no** | the same: the unpacked tool is checked against a record in the cache; it is unpacked again from its blob, which is verified |
| `tmp/` | no | downloads in progress |

The cache `path` lists the four directories it keeps (`blobs`, `modules`, `results`, `declarations`,
`git`) rather than excluding the others, so a cache directory added later is not restored until
someone decides it may be.

The trust boundary, stated in the guide: a GitHub cache entry can be written by any step of any job
of the same branch, not only by the save step (`actions/cache` uses the runner's token, which every
job holds). The core and every plugin are verified by sha256 whatever the cache holds. The derived
parts are not and cannot be: `modules/` is wasmtime's cache of compiled native code, which wasmtime
loads without checking, and `results/`, `declarations/` and `git/` are answers the core computed.
So with the cache, a step of another job on the same branch (a test suite's dependency, say) can
reach what the Gate job runs and reports, which it cannot without it. On a branch push that step
is the pushed commit's own code, which already controls its own workflow; on the default branch it
is merged code. Three answers are open to the lead:

- **(a), proposed:** cache as above, on by default, the boundary written in the guide.
- (b) cache only `blobs/` (verified): safe against any writer, but only the downloads stay warm; the
  compile and the result cache, which the consumer asked for, are cold in every job.
- (c) as (a), but opt-in (`[ci.github] cache = true`).

Why (a) needs no opt-in: the cache never decides what is checked (the plugins and the core are those
`gate.lock` pins, whatever is restored), a missing or evicted entry is a cold run that is slower and
otherwise the same, and the only writers that can forge a derived entry are jobs of the same
repository and branch. The cost is storage: one entry per job and pushed commit, under the
repository's cache limit (10 GB by default), where GitHub evicts the least recently used entry and
any entry not used for 7 days; Gate's entries can push out a repository's other caches. The entry
size of Gate's own jobs is recorded below from the measurement.

### 2. Cancelling superseded branch runs

```yaml
cancel-in-progress: ${{ github.ref != format('refs/heads/{0}', github.event.repository.default_branch) }}
```

The group stays `<workflow>@${{ github.ref }}`. A pull request run (`refs/pull/<n>/merge`) and a push
to any branch but the default one cancel the run of the same ref still in progress; a push to the
default branch never cancels a running check there and waits for it, as today (GitHub keeps one
waiting run per group). `github.event.repository.default_branch` is in the payload of both events the
workflow runs on. The superseded run's aggregate job still runs and fails naming the cancelled jobs,
as the guide already says for pull requests.

No setting: a superseded commit of a branch is never the one that merges, so its check has no reader.
The case a reader might want kept is a long-lived branch other than the default one (a
`release/1.x` in `branches`): its superseded pushes are now cancelled too. That is the same as a
pull request today; if the lead wants such branches to queue, the design adds nothing until a
consumer names one.

No opt-in because it changes the result of no check that anyone reads: only runs whose commit is no
longer the branch's head are stopped, and the default branch keeps every run.

### 3. The aggregate job's display name

A new key of `[ci.github]`, `aggregate-name`: the aggregate job's `name:`. Without it the name is
the id, byte for byte as today. With it, branch protection must require that name, since GitHub names
the check by the job's `name`; the guide says so. `aggregate-name` without `aggregate` is a
configuration error at load (exit 2):
`[ci.github] aggregate-name is set without aggregate; set aggregate to the job's id, or remove aggregate-name`.
An empty name is refused the same way as an empty job `name`.

### Where a configuration's generated workflow changes bytes

| Change | Which configurations | Bytes |
| --- | --- | --- |
| 1 | every `[ci.github]` | each `bin/gate` job gains the two cache steps and `GATE_CACHE_DIR` on each `bin/gate` step; the aggregate job is unchanged |
| 1 | every `[ci.github]`, at every core upgrade | the key carries `[gate] version`, so moving the core moves the workflow: after `gate lock`, `gate sync` rewrites it, and `gate sync --check` (the `changes` job) reports the drift until then |
| 2 | every `[ci.github]` | the one `cancel-in-progress` line |
| 3 | only one that sets `aggregate-name` | the aggregate job's `name:`; without the key, none |

A configuration that does none of the above does not exist: every `[ci.github]` moves with 1 and 2,
so upgrading to this core needs one `gate sync`, as T-043's concurrency group did.

### 4. The release order of a plugin kept in the repository it checks (documents only)

The pin moves only after the asset exists: the release job builds, pins the build with
`lock --set --file` in its own checkout and runs `check` on it, uploads the file to the tag's release,
and only then commits the pin. A rerun of the job never replaces an asset: when the release already
holds `<plugin>.wasm`, the job downloads it and compares sha256s; the same continues to the pin, a
different one fails and changes nothing.

## Measurement

Gate's own CI runs the released core, whose `sync --check` would report the new workflow as drift.
The measurement uses a branch of its own, `task/T-099-ci-measure`, cut from this branch, whose
workflow this branch's `gate sync` writes with `branches` widened to that branch; its `changes` job is
expected to fail at `sync --check` under the released core `gate.lock` pins (0.3.0), which does not stop the `gate` job (it lists no
projects). Two pushes: the first run's `gate` job is cold, the second, on a new commit, restores the
first's entry.

| Run | Run id | `gate` job | Restored entry |
| --- | --- | --- | --- |
| cold | - | - | - |
| warm | - | - | - |

## Done when

- The lead approved this design.
- The generated YAML is tested: the cache steps, their key and paths, the core and tools left out,
  `GATE_CACHE_DIR` on every `bin/gate` step and on no other, the save condition, the new
  `cancel-in-progress`, `aggregate-name` set and unset (unset: the aggregate job as 0.3.1 wrote it),
  the configuration error, `gate sync --check` drift; `ci/recommended` passes on the new workflow.
- A test copies a filled cache to a new directory, as a restore does, tampers the plugin there, and
  shows that the run never loads the tampered file: it is refused and fetched again, or the run fails
  closed when the asset cannot be fetched.
- The guide's `[ci.github]` section, its concurrency text and the release order are written.
- Every command of `AGENTS.md` exits 0 through the lane; the leak scan prints nothing.
- The cold and warm timings are recorded with their run ids; the branch is pushed.

## Proof

## Resume point

2026-11-23: the design is written; waiting for the lead's word. No code yet.

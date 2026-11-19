# T-066: Containers and repository hygiene

| Field | Value |
| --- | --- |
| Status | design approved by the lead at `c557d16` with the decisions below (2026-11-19); code after T-056 is green |
| Release | v0.3.0 (it adds `tree.size` to the plugin interface, which v0.3.0 moves to `gate:plugin@0.3.0`, and gives `nestjs/raw-config-access` aspects, which re-keys its findings) |
| Writer | "Gate 004: public presentation" |
| Reviewer | a separate review session |
| Depends on | the `gate:plugin@0.3.0` bump (T-057 or T-059, whichever merges first) |
| Touches | `wit/gate.wit` (`tree.size`), `crates/gate-sdk` (`Tree::size`), `crates/gate-host` (the tree host call and its read record), a new `plugins/docker` (two rules), a new `plugins/repository` (two rules), `languages/typescript` (the shared environment coverage), `plugins/nestjs` (`raw-config-access`: aspects and options), `Cargo.toml` (two workspace members), `docs/guide/plugins.md`, `CHANGELOG.md` |

## Goal

Every image, every compose file and every repository follows one standard, and a gate holds it:

- An image is built from a base pinned by digest, in stages, runs as a user that is not root, carries
  the OCI labels and no other, ignores what must never enter a build context, and takes no secret
  through a build argument or an environment variable. Its health check is the orchestrator's.
- A compose file has one name form, its services one name form, its images are pinned, and no secret is
  written into it.
- No repository tracks an environment file, a private key, a credential file, a token or a large binary.
- Every variable a service reads has a line in its `.env.example`, and every line there is read.

A hygiene finding never prints a secret's value: it names the path, the line and the kind of thing found.

## What was read for this design

Everything was read read-only, by listing tracked files and reading files at `HEAD`. Nothing was run in
any of these repositories, and no name, path or text of theirs is in this one. Secret-like values were
classified by a script (literal, interpolated, or a file path) by name and length; no value was printed.

- **Three repositories**: two NestJS services of the same shape, and a monorepo with a NestJS server
  project. About 24,000 tracked text files were scanned (each 1 MiB or less).
- **Five Dockerfiles**: the two services' images, the monorepo server's image, a frontend image and an
  end-to-end test harness image.
- **Six compose files**: a development and an end-to-end database for the monorepo server, an
  end-to-end database for each service, and two end-to-end scenario files of one service.
- **Three `.env.example` files**, and every environment read in their projects' code.

What the survey found, in short (the full table is kept outside this repository):

| Subject | Found | Standard below |
| --- | --- | --- |
| Base image | three of five Dockerfiles name a tag only; two pin a digest. Three registries are in use. | `pinned` |
| Stages | four of five are multi-stage; the test harness has one stage. | `stages` |
| User | the two service images run as root in every target; two others set `USER node`. | `user` |
| Health | no image has a `HEALTHCHECK`. | `healthcheck` (decision 1) |
| Ignore file | two `.dockerignore` at the root, two `Dockerfile.dockerignore` beside their Dockerfile, one Dockerfile with none beside it. | `ignore` (decision 2) |
| Labels | every image sets `org.opencontainers.image.revision`; the release id is under a label namespace of each product's own. | `labels` (decision 3) |
| Build secrets | none in `ARG` or `ENV`: all three images that need one use a BuildKit secret mount. | `secret` |
| Compose file names | `docker-compose.<purpose>.yml` and `docker-compose.<scenario>.<purpose>.yml`, purpose `e2e` or `development`. No other form. | `name` |
| Compose services | the single-database files name services `app-<purpose>-<role>`; the scenario files do not. | `service` |
| Compose images | four of six files pin their database image by tag only; the rest by digest or by a required variable. | `pinned` |
| Compose secrets | two end-to-end files hold a literal database password, one a literal access key pair; the monorepo interpolates its passwords. | `secret` |
| Published ports | all on the loopback address with a random host port; no `container_name` anywhere. | `port`, `container-name` |
| Tracked environment files | none other than `.env.example`. | `tracked-secrets#env-file` |
| Keys, credentials, tokens | none tracked. Five content matches, all false: four test strings holding a private-key header with a four-byte body, and a `printf` format line. | `tracked-secrets` |
| Large files | one repository tracks 14 PNG files over 512 KiB (the largest 6.3 MiB); two track text files over 1 MiB (generated code, long documents, a 4 MiB JSON corpus). | `large-files` |
| `.env.example` | keys read but not listed: one in one service, two in the other. Listed but not read by literal in the service's code: 2 to 4 keys each. Two of these are read through a loop over a literal array, one by Node itself, one by a local script, one only by the Dockerfile. | `raw-config-access` aspects |
| `process.env` | each service reads one variable through `process.env` in feature code, outside the configuration boundary; `raw-config-access` does not see it, because `process` is a global and is never imported. | `raw-config-access#process-env` |

## 1. The standard

### Images (`Dockerfile`)

1. **A base is pinned by digest.** Each `FROM` that names an image (not an earlier stage, not `scratch`)
   is `<image>@sha256:<64 hex>`, the grammar `ci/actions-pinned` already uses for `docker://` steps. A tag
   is a pointer its publisher moves; a digest is the image. A `FROM ${ARG}` passes only when the
   argument's default is pinned. When the plugin's `registries` is set, the image's registry is one of
   them.
2. **An image is built in stages.** A Dockerfile has at least two stages, so build tools and
   development dependencies stay out of what runs.
3. **A runtime stage runs as a user that is not root.** A runtime stage is one no other stage builds
   from (`FROM <stage>`) or copies from (`COPY --from=<stage>`). Its effective user, its own last `USER`
   or the one it inherits from its stage chain, is set and is not `root` or `0`. Every runtime stage
   counts, including one that only prints an error: anyone can build it.
4. **No `HEALTHCHECK`** (decision 1). The health check is the orchestrator's: ECS takes a container's
   check from its task definition, and the `cdk` plugin (T-064) requires one for every service
   container. A check in the image would be a second place for the same job. Any `HEALTHCHECK`
   instruction, `NONE` included, is reported.
5. **The build context ignores what must never enter it** (decision 2). Each Dockerfile has its own
   ignore file, `<name>.dockerignore` beside it. BuildKit, the default builder since Docker Engine 23.0,
   reads it whatever the build context is, and the form scales to several Dockerfiles in one
   repository. A `.dockerignore` anywhere is reported. The file either starts as an allow-list (`**` as
   its first pattern) or excludes each pattern of `must-ignore`.
6. **No secret is a build argument or an environment variable.** An `ARG` or `ENV` whose name contains
   one of `secret-names` is reported, with or without a value: both end up in the image's history.
   Secrets reach a step through `RUN --mount=type=secret`, as every image read for this design already
   does.
7. **The labels are the OCI ones** (decision 3). Each runtime stage sets, itself or through its stage
   chain, `org.opencontainers.image.source`, `org.opencontainers.image.revision` and
   `org.opencontainers.image.version`. A label of any other name is reported: the release id is
   `org.opencontainers.image.version`, not a product's own label.
8. **One name per job:** an image's file is `Dockerfile`, one per directory. `Dockerfile.<x>`,
   `<x>.Dockerfile` and `dockerfile` are reported, and so is a `.dockerignore`, whose job the file
   beside the Dockerfile has.

### Compose files

1. **Name**: `docker-compose.<purpose>.yml`, or `docker-compose.<scenario>.<purpose>.yml` for one of
   several stacks of the same purpose. `<purpose>` is one of `purposes` (`development`, `e2e`) and
   `<scenario>` is kebab-case. `compose.yaml`, `.yaml`, `docker-compose.yml` without a purpose,
   `docker-compose.dev.yml` and `docker-compose.test.yml` are reported.
2. **Services**: `<owner>-<purpose>-<role>`, kebab-case. `<owner>` is `app` for the project's own
   services, or whose service it is in a scenario that runs another project's; `<purpose>` is the
   file's purpose; `<role>` is what it is (`postgres`, `dns`). The purpose is in the name for a reason:
   two files in one directory share a compose project name, so a `postgres` service in the development
   file and one in the end-to-end file would be the same service.
3. **No `container_name`**: a fixed name collides between two worktrees or two runs.
4. **Images are pinned**: `image:` is `<image>@sha256:<64 hex>`, or a required variable
   (`${NAME:?...}`), whose value the caller pins. A plain `${NAME}`, or a default that is not pinned
   (`${NAME:-postgres:17}`), is reported. A service built from a Dockerfile has its pin there.
5. **No secret is written in, except in an end-to-end file** (decision 4). An `environment` entry
   whose name contains one of `secret-names` and whose value is a literal, or a `${NAME:-literal}`
   default, is reported. A `${NAME}` value, a name ending in one of `reference-suffixes` (`_FILE`,
   `_PATH`: the value is a path) and compose `secrets:` pass. In a `*.e2e.yml` file a literal passes:
   such a file starts local, throwaway dependencies, and their credentials protect nothing. This is part
   of the rule's definition, written on its page, not an exception.
6. **A published port is on the loopback address**: `127.0.0.1:` or `[::1]:` before the host port.
   Every file read already does this; the rule keeps it so.
7. **A service others wait for declares its health** (decision 1): a service that another service's
   `depends_on` names with `condition: service_healthy` has a `healthcheck`.

### Repository hygiene

1. **No environment file** is tracked except `.env.example`: `.env`, `.env.<anything>` and `*.env`
   are reported wherever they are.
2. **No key or credential file** is tracked, by name: `*.key`, `*.p12`, `*.pfx`, `*.jks`, `*.keystore`,
   `*.kdbx`, `*.ppk`, `id_rsa`, `id_dsa`, `id_ecdsa`, `id_ed25519` (not their `.pub`), `.netrc`,
   `.aws/credentials`, `.git-credentials`.
3. **No private key** is in any file's content: a `-----BEGIN ... PRIVATE KEY-----` block with its
   `END` line and at least 64 base64 characters between them. The survey's four test strings, each a
   header and a four-byte body, do not match; a real key, at 1,600 characters or more, always does.
4. **No token** is in any file's content, by the shapes their issuers publish: AWS access key ids
   (`AKIA` or `ASIA` and 16 more), GitHub tokens (`ghp_`, `gho_`, `ghu_`, `ghs_`, `ghr_` and 36 or
   more), npm tokens (`npm_` and 36), Slack tokens (`xoxa-`, `xoxb-`, `xoxp-`, `xoxr-`, `xoxs-`).
5. **No credential in an `.npmrc`**: an `_authToken`, `_auth` or `_password` line with a literal value.
   `${NAME}` passes.
6. **No large file** (decision 5): a tracked file over 1 MiB is `large`, and over 5 MiB `huge`. The
   page sets them at `warning` and `error` in its example; the level is the configuration's. A project
   declares a large file it needs in `extra` as `{ path, job }`, and it is not reported. Whether a large
   file moves to Git LFS or to storage is the project's call.

### Environment coverage

A project's `.env.example` lists exactly the variables the project reads, for a service and a
frontend alike, with one implementation:

- each key a read names has a line;
- each line names a key some read names;
- a read whose key the rule cannot name is reported, so that it can be written as a literal;
- `process.env` is reached only inside the configuration boundary, like the raw configuration service.

## 2. The plugins

### `docker`: a new plugin, language-free, project scope

Two rules, one per file kind, with aspects:

| Rule | Aspects |
| --- | --- |
| `docker/dockerfile` | `name`, `pinned`, `stages`, `user`, `healthcheck`, `labels`, `ignore`, `secret` |
| `docker/compose` | `name`, `pinned`, `secret`, `service`, `container-name`, `port`, `healthcheck` |

### Hygiene: a new `repository` plugin, language-free, repository scope (recommended)

Two rules:

| Rule | Aspects |
| --- | --- |
| `repository/tracked-secrets` | `env-file`, `key-file`, `private-key`, `token`, `npmrc` |
| `repository/large-files` | `large`, `huge`, `extra-missing` |

**Why a plugin of its own, and not `git`.** Every `git` rule is about history or git's own data (the
commit message, the branch, main's first-parent line, a CI-skip directive), and most read history.
Hygiene reads only the snapshot. In `git`, a repository would have to take on history rules to get
hygiene, and the `git` plugin would cover two subjects. `repository` has one: what the tracked tree
holds, whatever the language.

**Why repository scope.** A secret is a problem wherever it sits, including at a monorepo root that no
project owns. A repository-scope rule runs once and sees every tracked path, as `agents/no-claude-md`
does from 0.3.0 (T-062). It is set in the root `gate.toml`.

**What it does not cover: history.** A key committed and removed again within one push leaves a clean
tip and a key in history. The pre-commit hook checks each commit's index, so a repository that runs the
hooks catches it at the commit. A history aspect (`reads-history`, the pushed range) would belong to
`git`; it is a follow-up, not part of this task.

### Environment coverage: one implementation in `gate-typescript`, called by `nestjs/raw-config-access` and the frontend's rule

The comparison of `.env.example` with the reads is one module of the TypeScript language library,
`gate_typescript::env`. Each rule finds its own reads and hands them over: `raw-config-access` the keys
of its raw configuration getters and of `process.env`, the frontend plugin's environment rule (T-068)
the keys its configuration module declares. The module parses the example file (`KEY=value` and
`export KEY=value` lines, comments and blank lines skipped), compares, and reports through the rule's
`Report` under the same aspect names and the same options in both rules:

```rust
pub struct EnvRead { pub key: Option<String>, pub path: String, pub span: Span, pub expression: String }
pub struct ExternalKey { pub key: String, pub reader: String }
pub fn check_example(report: &mut Report, example_path: &str, example: Option<&str>, reads: &[EnvRead], external: &[ExternalKey]);
```

A read whose key is `None` is one the rule cannot name (`dynamic`).

### `nestjs/raw-config-access` gains aspects

`raw-config-access` already knows the raw configuration services (`raw-imports`), follows every name
that reaches them, and knows the configuration boundary (`approved-*`). Its page already gives the
reason for coverage: without a boundary, "the key inventory no longer lists every reader". Coverage
needs exactly these inputs. A separate rule would need the same `raw-imports` and boundary written a
second time, which is the duplication the owner forbids. So the rule gets aspects:

| Aspect | Reported when | At | Key |
| --- | --- | --- | --- |
| `boundary` | a file outside the boundary reaches a raw configuration export (the rule's finding today) | the file | the file |
| `process-env` | a file outside the boundary reads `process.env` (member, element, destructuring, `in`) | the read | the file |
| `undeclared` | a key read through a getter of a raw configuration export, or through `process.env`, has no line in `example-file` | the read | the key |
| `unread` | a line of `example-file` names a key no read names and `external-keys` does not list | the line | the key |
| `dynamic` | a getter's key argument, or a `process.env` element, is none of: a string literal; a `const` bound to one; a loop variable over a literal array of strings; in the same file | the argument | the file and the expression's text |
| `example-missing` | `example-file` is not tracked, and some read names a key | the project root | the file |

New options:

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `example-file` | path | `".env.example"` | The example environment file, relative to the project root. |
| `getters` | list of strings | `["get", "getOrThrow"]` | The methods of a raw configuration export whose first argument is a key. |
| `external-keys` | list of `{ key, reader }` | `[]` | Keys something other than the project's TypeScript reads (Node.js itself, a local script, the Dockerfile), each with what reads it; they count as read. |

The `boundary` aspect is today's finding. Because the aspect is part of the fingerprint, a baseline
entry of 0.2 re-keys. `CHANGELOG.md` (0.3.0, Breaking Changes) says to run `gate baseline write` once
after the upgrade, after checking that `gate check` reports the same files.

**The alternative**, a `typescript/env-example` rule reading `process.env` only, would miss every key
read through the configuration service, which is how the services read almost all of theirs.

## 3. Shared options and the same principles

**Packs alike.** The two new plugins look like the others: kebab-case rule ids, aspects for the checks
of one subject, a page per rule in the house form (what it checks and why, options, an example
finding, how to fix it), no preset that sets a level.

**One name per job.**

- `pinned` is the aspect name for "an image reference is pinned by digest" in both `docker` rules, with
  the grammar of `ci/actions-pinned`.
- `secret` is the aspect for "a secret is written into this file" in both `docker` rules.
- `name` is the aspect for "the file's name is not the standard's" in both.
- `secret-names` is one list in `[plugins.docker]` options, which both rules read: `PASSWORD`, `PASSWD`,
  `SECRET`, `TOKEN`, `PRIVATE_KEY`, `API_KEY`, `ACCESS_KEY`, `CREDENTIAL`, `AUTH` (a name contains one,
  ignoring case).
- `registries` is one list in `[plugins.docker]` options (default `[]`: any registry).

**Options per rule.**

| Rule | Option | Type | Default |
| --- | --- | --- | --- |
| `docker/dockerfile` | `must-ignore` | list of patterns | `[".git", ".env", ".env.*", "*.env", "*.pem", "*.key"]` |
| `docker/dockerfile` | `labels` | list of names | `["org.opencontainers.image.source", "org.opencontainers.image.revision", "org.opencontainers.image.version"]` |
| `docker/compose` | `purposes` | list of strings | `["development", "e2e"]` |
| `docker/compose` | `reference-suffixes` | list of strings | `["_FILE", "_PATH"]` |
| `repository/tracked-secrets` | `env-examples` | list of file names | `[".env.example"]` |
| `repository/large-files` | `large-bytes` | integer | `1048576` (1 MiB) |
| `repository/large-files` | `huge-bytes` | integer | `5242880` (5 MiB) |
| `repository/large-files` | `extra` | list of `{ path, job }` | `[]`: the large files the project needs, each with its job |

**A secret is never printed.** The SDK has no call that puts a file's text into a finding. For
`tracked-secrets` and the two `secret` aspects, the message names the path, the line and the kind
(`an AWS access key id`, `a literal value for POSTGRES_PASSWORD`). The name of a variable is not a
secret; its value is. The key is the kind and its ordinal in the file, never the matched text. The
target is `<path>:<line>`, so an exception can name a test fixture that holds a fake key on purpose.

## 4. The interface: `tree.size`

`repository/large-files` needs every tracked file's size and nothing else. Through `tree.read` alone it
would read the whole tree on every full run, between 43 and 75 MiB in the repositories read, and its
read set would hold every file, so any change would invalidate its cached result. `gate:plugin@0.3.0`
gets:

```wit
interface tree {
  /// The size in bytes of a tracked path; none when it is not tracked or not granted.
  size: func(path: string) -> option<u64>;
}
```

The host takes the size from the snapshot's index entry or tree object, without reading the content,
and records `(path, size)` in the read set. `large-files` reads no content at all. `tracked-secrets`
still reads every text file, which is its job. On a hook run, the result cache keeps that cost to the
changed files. The other `gate:plugin@0.3.0` additions (seeds, generator plugin options, rule decisions)
touch the same lines of `wit/gate.wit`; whichever branch merges later rebases.

## 5. Tests

Native SDK tests per aspect, each with a fixture that is reported and a passing twin; one end-to-end run
through the built components. Every case is one that would let a broken change through, or block a good
one.

**`docker/dockerfile`:**

- `pinned`: a tag-only base, a `FROM ${BASE}` whose default is a tag, and an image outside `registries`
  are each reported. A digest-pinned base, `FROM <earlier stage>` and `scratch` pass.
- `stages`: a one-stage Dockerfile is reported; two stages pass.
- `user`: a runtime stage without `USER`, with `USER root` and with `USER 0` is reported. A stage that
  inherits `USER node` from its base stage passes. A build stage without `USER` passes. Two runtime
  targets, one without a user, give exactly one finding.
- `healthcheck`: `HEALTHCHECK CMD ...` and `HEALTHCHECK NONE` are each reported; a Dockerfile without
  one passes.
- `labels`: a runtime stage without `org.opencontainers.image.version` is reported, naming it; one
  that inherits the three labels from its base stage passes; `LABEL org.example.release-id=...` is
  reported.
- `ignore`: no `Dockerfile.dockerignore` beside the file is reported. An allow-list file (`**` first)
  passes. A file missing `.env` is reported, naming the missing pattern. A `.dockerignore`, at the root
  or beside a Dockerfile, is reported under `name`.
- `secret`: `ARG NPM_TOKEN`, `ENV DB_PASSWORD=x` and `ARG api_key` are reported. `ARG RELEASE_ID` and
  `RUN --mount=type=secret,id=npm_token` pass. The finding's message, help, target and key never contain
  the value `x`.
- `name`: `Dockerfile.api`, `api.Dockerfile` and `dockerfile` are reported.

**`docker/compose`:**

- `name`: `compose.yaml`, `docker-compose.yml`, `docker-compose.dev.yml` and `docker-compose.e2e.yaml`
  are reported. `docker-compose.e2e.yml` and `docker-compose.load-test.e2e.yml` pass.
- `service`: `postgres`, `app-postgres`, `appE2ePostgres` and `app-development-postgres` in an `e2e`
  file are reported; `app-e2e-postgres` passes.
- `container-name`: any `container_name` is reported.
- `pinned`: `postgres:17.7-alpine`, `${IMAGE}` and `${IMAGE:-postgres:17}` are reported. A digest and
  `${IMAGE:?set the image}` pass. A service with only `build:` passes.
- `secret`: a literal `POSTGRES_PASSWORD`, in map form and in list form, and `${PASSWORD:-literal}` are
  reported in `docker-compose.development.yml`. `${PASSWORD}`, `POSTGRES_PASSWORD_FILE:
  /run/secrets/x` and `secrets:` pass. The same literal in `docker-compose.e2e.yml` and in
  `docker-compose.load.e2e.yml` passes. No output contains the literal.
- `healthcheck`: a service named by another's `depends_on` with `condition: service_healthy` and no
  `healthcheck` is reported; with one it passes; `condition: service_started` needs none.
- `port`: `"5432:5432"` and `"0.0.0.0:5432:5432"` are reported; `"127.0.0.1::5432"` and
  `"[::1]:5432:5432"` pass.
- A file that is not valid YAML stops the run with the parser's message.

**`repository/tracked-secrets`:**

- `env-file`: `.env`, `apps/a/.env.local` and `prod.env` are reported; `.env.example` passes.
- `key-file`: `deploy.key`, `id_ed25519` and `.aws/credentials` are reported; `id_ed25519.pub` passes.
- `private-key`: a full generated test key is reported, at its `BEGIN` line. A header with an 8-character
  body, and a header without an `END` line, pass. The finding carries no key text: a test searches
  the whole output for every base64 line of the fixture key.
- `token`: one fixture per shape is reported. A 15-character `AKIA` string and a `ghp_` with 20
  characters pass.
- `npmrc`: a literal `_authToken` is reported; `${NPM_TOKEN}` passes.
- A file under a project and a file at a monorepo root no project owns are both seen.

**`repository/large-files`:**

- A file of `large-bytes + 1` bytes is `large`, and one of exactly `large-bytes` passes.
- A file of `huge-bytes + 1` bytes is `huge` only, not also `large`.
- A file declared in `extra` passes at any size; a declared path that is not tracked is
  `extra-missing`; an `extra` entry without a `job` is a configuration error.
- The rule reads no content: its read record holds sizes only.

**`tree.size` (host):** the size of a tracked path at the working copy, at the index and at a commit; an
untracked or ungranted path gives none; a changed size invalidates the cached result, and a changed
content of the same size does not invalidate it for a rule that read only the size.

**`gate_typescript::env` (unit tests of the shared module):**

- The parser takes `KEY=value`, `export KEY=value`, `KEY=` and quoted values, and skips comments and
  blank lines; a line that is none of these is reported at its line as `example` syntax, without its
  text.
- `undeclared`, `unread`, `dynamic` and `example-missing` each from a hand-built list of reads, each
  with a passing twin; `external-keys` makes a key read.
- No finding's message, help, target or key holds a value of the example file.

**`nestjs/raw-config-access`:**

- `boundary`: today's fixtures, now under the aspect.
- `process-env`: `process.env.X`, `process.env["X"]` and `const { X } = process.env` outside the
  boundary are reported; inside the boundary, they pass.
- `undeclared`: `config.get("X")` with no `X=` line is reported at the call; with the line it passes.
- `unread`: an `X=` line nobody reads is reported at its line. It passes when a getter reads it, when a
  loop over `["X", "Y"]` passes the loop variable to a getter, and when `external-keys` lists it.
- `dynamic`: `config.get(name)` with `name` a parameter is reported; `const name = "X"` passes.
- `example-missing`: a project with reads and no `.env.example` gives one finding.
- A finding's message names keys, never the example file's values.

**End to end:** one repository with every new rule enabled at `error`: one image, one compose file,
one `.env.example`, a planted fake key and a 2 MiB PNG. `gate check` exits 1 with exactly the planted
findings. With the plants removed, it exits 0.

## Decisions (the lead, 2026-11-19)

1. **Health checks live in the orchestrator, not the image.** ECS takes the container's check from the
   task definition, so the `cdk` plugin (T-064) gets an aspect: every service container declares one.
   A compose service that others wait for with `service_healthy` declares a `healthcheck`. A
   `HEALTHCHECK` in a Dockerfile is a finding: one place per job.
2. **The ignore file sits beside its Dockerfile** (`<name>.dockerignore`). The form scales to several
   Dockerfiles in one repository. A `.dockerignore` is a finding.
3. **Labels are the OCI ones**, `org.opencontainers.image.source`, `.revision` and `.version`. No label
   of another name.
4. **Literal credentials are allowed only in `*.e2e.yml`**, for the local throwaway dependencies those
   files start. It is part of the rule's definition, on its page, not an exception. Everywhere else
   they are a finding.
5. **Large files**: `large` above 1 MiB, `huge` above 5 MiB, at `warning` and `error` in the page's
   example. A project declares a large file it needs in `extra` as `{ path, job }`. Whether it moves to
   Git LFS or to storage is the project's call.
6. **Environment coverage is one implementation**, in `gate-typescript`, called by
   `nestjs/raw-config-access` and by the frontend plugin's environment rule (T-068), under the same
   aspect names and options.

## Done when

- The code, on a branch from `main` once T-056 is green, then one lane round on the pushed tip: fmt,
  clippy, test (`--workspace --no-fail-fast`), deny and the release build exit 0, and the leak scan
  prints nothing.
- Each new rule has its page, and `docs/guide/plugins.md` lists the two plugins.

## Resume point

2026-11-19: the lead approved the design at `c557d16` and decided the five open items and the shared
environment module (section "Decisions"). The `cdk` writer (T-064) and the frontend writer (T-068) were
told their parts. Next: the code, on a branch from `main` once T-056 is green.

Earlier:

2026-11-19: design written from `main` at `ec66737`, after a read-only survey of three repositories
(the divergence table is kept outside this repository).

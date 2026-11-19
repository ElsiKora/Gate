# Security Policy

Gate runs in hooks and CI jobs, downloads its core and plugins, and runs third-party plugins. A weakness
in any of that matters, and we want to hear about it.

## 🛡 Supported Versions

| Version | Supported |
| --- | --- |
| latest `0.x` release | ✅ |
| older releases | ❌ |

Security fixes are released as a new patch version of the latest release.

## 📣 Reporting a Vulnerability

Report it privately through
[GitHub Security Advisories](https://github.com/ElsiKora/Gate/security/advisories/new). Do not open a
public issue or pull request for it.

Please include:

- the Gate version (`gate --version`) and platform;
- what an attacker controls (a repository's files, a plugin, a release asset, the network) and what
  they gain;
- the steps or a minimal repository that shows it.

We confirm a report within three working days, keep you informed while we fix it, and credit you in the
advisory unless you ask us not to.

## 🔒 What Gate Promises

These are the properties a report can hold us to:

- **Checksums fail closed.** The core and every plugin are verified against the sha256 in `gate.lock`
  before they run; a mismatch stops the run, and Gate deletes nothing it did not write.
- **Plugins are sandboxed.** A plugin sees the tracked files of its scope, read-only, the git data its
  rules declare, and its options. It has no file writes, no network, no clock and no environment.
- **Overrides are explicit and contained.** A local plugin given with `--plugin` exists only on the
  command line, is named with its sha256 in the report, never enters the shared cache, and is refused
  by `--no-overrides`, which the generated CI jobs pass.
- **Credentials fail closed.** A private asset is fetched only from the URL in `gate.lock`, with
  `$GATE_GITHUB_TOKEN`, `$GITHUB_TOKEN` or `gh auth token`; without one that can read it, the run stops.
- **Only tracked files exist.** Ignored files, nested worktrees and untracked files are never read.
- **Hooks do not skip themselves.** The generated hooks never skip on an environment variable.

A plugin that escapes its sandbox, a run that executes an unverified artifact, an override that reaches
the cache or a CI job, or a hook that can be skipped silently is a vulnerability.

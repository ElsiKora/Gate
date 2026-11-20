# `repository/tracked-secrets`

No tracked file is an environment file or a key file, or holds a private key, a token of a published
shape or a literal `.npmrc` credential.

## What it checks and why

A secret in a commit stays in the history of every clone, after the file is deleted. The rule reads
every tracked file of the repository: by its name, and, unless a NUL byte in its first 8000 bytes marks
it as binary, by its text. A finding names the path, the line and the kind of secret; never the value.

| Aspect | Reported when | At |
| --- | --- | --- |
| `env-file` | a file is named `.env`, `.env.<anything>` or `<anything>.env`, and is not one of `env-examples` | the file |
| `key-file` | a file has the extension `key`, `p12`, `pfx`, `jks`, `keystore`, `kdbx` or `ppk`, is named `id_rsa`, `id_dsa`, `id_ecdsa`, `id_ed25519`, `.netrc` or `.git-credentials`, or is `.aws/credentials` | the file |
| `private-key` | a `-----BEGIN ... PRIVATE KEY-----` line is followed by at least 64 base64 characters and an `-----END` line | the `BEGIN` line |
| `token` | a word has the shape of an AWS access key id (`AKIA` or `ASIA` and 16 more), a GitHub token (`gh[pousr]_` and at least 36 more), an npm token (`npm_` and 36 more) or a Slack token (`xox[abprs]-` and at least 10 more) | the line |
| `npmrc` | a line of a file named `.npmrc` sets `_authToken`, `_auth` or `_password` to a value that is not `${...}` | the line |

A key block without its body or its `END` line is documentation, not a key, and passes; so does a
string shorter than its token shape. A public key (`id_ed25519.pub`) passes.

Scope: repository. Reads history: no. Aspects: `env-file`, `key-file`, `private-key`, `token`,
`npmrc`. Targets: the path for `env-file` and `key-file`, `<path>:<line>` for the others. A finding's
key is its aspect for a file, and `<kind>#<ordinal>` within the file for content, so a finding keeps
its key when lines above it move.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `env-examples` | list of file names | `[".env.example"]` | The example environment files, which hold placeholders and are tracked. |

```toml
[plugins]
repository = { source = "ElsiKora/Gate:repository@0.3.0" }

[rules]
"repository/tracked-secrets" = { level = "error", options = { env-examples = [".env.example", ".env.sample"] } }
```

## Example finding

```text
error repository/tracked-secrets#env-file apps/api/.env
  apps/api/.env is an environment file
  help: track `.env.example` with placeholders, and keep the real file out with .gitignore
error repository/tracked-secrets#token scripts/deploy.sh:14
  scripts/deploy.sh:14 holds a GitHub token
  help: remove it, revoke it with its issuer, and read it from a secret store; a test fixture that holds a fake on purpose takes an exception for this target
```

## How to fix it

Revoke the secret with its issuer first: it is in the history already. Then remove the file or the
line, keep the file out with `.gitignore`, and read the value from the environment or a secret store.
A test fixture that holds a fake key or token on purpose takes an exception for its target
(`<path>:<line>`), with the reason.

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
| `private-key` | a `-----BEGIN <words> PRIVATE KEY-----` or `-----BEGIN PGP PRIVATE KEY BLOCK-----` header, anywhere in a line, is followed by a run of at least 64 base64 characters in a row and `-----END`, on the next lines or on the same one (a JSON string with `\n` escapes, a string literal); line breaks, the indentation around them and `\n` or `\r` escapes do not break the run | the header's line |
| `token` | a word has the shape of an AWS access key id (`AKIA` or `ASIA` and 16 more), a GitHub token (`gh[pousr]_` and at least 36 more, or a fine-grained `github_pat_`, 22 more, `_` and 59 more), an npm token (`npm_` and 36 more) or a Slack token (`xox[abprs]-` and at least 10 more) | the line |
| `npmrc` | a line of a file named `.npmrc` sets `_authToken`, `_auth` or `_password` to a value that is not `${...}` | the line |

A key block without its body or its `END` line is documentation, not a key, and passes; so does a
string shorter than its token shape. A public key (`id_ed25519.pub`, a `PGP PUBLIC KEY BLOCK`) passes.

Scope: repository. Reads history: no. Aspects: `env-file`, `key-file`, `private-key`, `token`,
`npmrc`.

Code that names the header and the footer as constants, and prose about them, holds no such run
and passes.

A finding about a file has the path as its target and its aspect as its key. A finding about a
file's content is identified by the kind of secret and the secret itself, never by the line or by
its place among others: its key is `<kind>:<hash>`, the kind (`private-key`, `aws-access-key-id`,
`github-token`, `npm-token`, `slack-token`, `npmrc-credential`) and the first 12 hex digits of the
sha256 of the matched text (the token, the credential's value, or the key's base64 body), and its
target is `<path>:<key>` (`test/keys.txt:private-key:3f1c09a2b7e4`). An exception or a baseline entry
therefore keeps matching when the file changes around the secret, and covers that secret only: a
real key added above an excepted fixture has a key of its own and is reported. The same secret
twice in one file is one finding. The 12 digits tell the secrets of a file apart; they are far too
few to recover a key or a token. The location still names the line.

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
(`<path>:<kind>:<hash>`, such as `test/fixtures/keys.pem:private-key:3f1c09a2b7e4`; `gate check
--format json` gives it as the finding's `target`), with the reason.

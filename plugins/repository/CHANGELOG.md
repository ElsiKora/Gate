# Changelog of the repository plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:repository@<version>`.

## Unreleased

Built for the plugin interface `gate:plugin@0.3.0`.

### Features

- `repository/tracked-secrets`: no environment file other than an example, no key or credential
  file, no private key block, no token of a published shape and no literal `.npmrc` credential is
  tracked; a finding names the path, the line and the kind, never the value.
- `repository/large-files`: a tracked file over 1 MiB is `large` and over 5 MiB `huge`, unless the
  project declares it in `extra` with its job; the rule reads sizes only.
- Presets `repository/recommended` (both rules at `error`, `large-files#large` at `warning`) and
  `repository/adoption` (both at `warning`).

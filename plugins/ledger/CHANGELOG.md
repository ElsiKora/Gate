<!--
The lead writes a version's section before tagging it, newest first, in the format of the other
ElsiKora repositories; the release takes that section as its notes and fails without it:
a minor or major release is an `#` heading, a patch an `##` heading, each with `### Features` and
`### Bug Fixes` entries of the form `- **scope:** subject ([sha](commit link))`.
Entries link their commit from the second release on; the first release's section, 0.1.0, links
none, because the public history starts at a fresh root at 0.1.0.
The ledger plugin is versioned on its own; its releases are `plugin-ledger-v<version>`.
-->

## Unreleased

### Changed

- A wrong option, or an exceptions file the options name that is not valid TOML, ends the run as a
  configuration error (exit 2, `configuration error`), not as an environment error. A file the rule
  cannot read stays an environment error.

# 0.1.0

### Features

- **plugin-ledger:** the rules `blockers`, `closing-notes`, `decisions`, `exception-decisions`,
  `exception-owners`, `manifest`, `record-status`, `records` and `reuses`, with the shared
  `[plugins.ledger] options`, whose schema the plugin declares so the core checks them when the
  plugin loads.
- **plugin-ledger:** the generators `decision-counts` and `review-progress`, which write the counts
  of a decision index and the review progress of a roadmap into their managed blocks.

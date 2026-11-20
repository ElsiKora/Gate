# T-078: Documentation for the 0.3.0 release

| Field | Value |
| --- | --- |
| Status | done; ready for the lead |
| Release | 0.3.0 |
| Writer | the T-066 writer |
| Reviewer | the lead |
| Depends on | `main` at `8b9d40c` (T-075 merged) |
| Touches | `plugins/git/rules/commit-message.md`, `README.md`, `docs/guide/*`; documents only |

## Goal

1. **Issue references in a commit body.** [`git/commit-message`](../../plugins/git/rules/commit-message.md)
   gets a short note. As in the conventional-commits parser, a body line that holds `#` and an id,
   followed by whitespace, `,;)]` or the end of the line (`repo#12`), starts the footer, so
   `footer-leading-blank` fails. The note says how to write such text instead and points to
   `has_issue` for the exact rule.
2. **A release-readiness pass over `README.md` and `docs/guide/*`.**
   - Every plugin on `main` is listed with one line and its presets: agents, cdk, ci, docker, docs,
     frontend, git, ledger, markdown, nestjs, node, repository and typescript.
   - Where a version is shown, the plugin interface is named `gate:plugin@0.3.0`.
   - No count is stale.
   - Every link resolves, anchors included, as checked by a script.
   - CHANGELOG headings and versions are left to the release cut.

## Done when

- The note is on the rule page, and each example's behaviour was measured with `bin/gate check
  --commit-message`.
- README and the guides list the 13 plugins with their presets, as declared in each plugin's
  `src/lib.rs`, and the counts match the code.
- The link script reports no broken link or anchor in `README.md` and `docs/guide/*.md`.
- The leak scan prints nothing. There is no code change, so there is no lane round.

## Resume point

2026-11-20T20:16:54+00:00: done, documents only.
- The note is on the commit-message page. Each spelling was measured with `bin/gate check
  --commit-message` (0.2.1): `repo#12` before a space, at the end of a line and in parentheses
  fails `footer-leading-blank`; in backquotes, before `.` or `:`, and in a line with a URL it
  passes.
- README and the plugins guide list the 13 plugins with their presets, as declared in each
  plugin's `src/lib.rs`. The guide also lists the ledger generators, `typescript/barrel-imports`
  and the four git rules with their pages. The roadmap marks the merged work done.
  `writing-a-plugin.md` names `gate:plugin@0.3.0`, as in `wit/gate.wit`.
- A link script, which follows GitHub's heading slugs and reports a broken anchor, checked 114
  relative links in `README.md`, `docs/guide/*.md`, the rule page and this file: 0 broken.
- Left to the release cut: the pinned versions in the examples (`@0.2.0` and `@0.2.1` beside
  `@0.3.0`), the release note in README and the schema URL in `configuration.md`.

2026-11-20T20:14:51+00:00: branch `task/T-078-release-docs` from `main` `8b9d40c`; task file
written. Next: the note, then the README and guide pass.

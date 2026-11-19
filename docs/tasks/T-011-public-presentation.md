# T-011: Public presentation

| Field | Value |
| --- | --- |
| Status | merged |
| Release | v0.1.0 |
| Writer | "Gate 004: public presentation" |
| Reviewer | the lead |
| Depends on | none (the guides follow the interfaces of T-002 and T-003 as they land) |
| Touches | `README.md`, `LICENSE`, `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md` if the house style has one, `.github/ISSUE_TEMPLATE/**`, `.github/PULL_REQUEST_TEMPLATE.md`, `docs/guide/**`, `PROVENANCE.md` (removed) |

## Goal

The repository reads like the company's other public repositories: the same README layout, licence,
changelog, contribution and release conventions; and it teaches a newcomer to install Gate, configure a
monorepo, write a plugin and write a rule.

## Scope

1. Read the company's public repositories on GitHub (`ElsiKora/NestJS-Crud-Automator`,
   `ElsiKora/ESLint-Config`, `ElsiKora/NestJS-Crud-Config`, and two or three more public ones) with
   `gh api` or `gh repo view`: README structure (badges, logo/banner, sections, tables), licence,
   changelog format and tooling, contributing, issue and PR templates, release conventions, security
   policy, topics. Write down the house style in `docs/guide/house-style.md` (short; it is our own
   reference for later repositories) with the exact pattern of each item.
2. Write, in that style: `README.md` (what Gate is, why, features, install, quick start for one project
   and for a monorepo, plugins, configuration overview, commands, links to guides), `LICENSE` (the
   licence those repositories use, with the company as holder), `CHANGELOG.md` (empty `Unreleased` in
   the house format), `CONTRIBUTING.md`, `SECURITY.md`, templates.
3. Guides under `docs/guide/`: `getting-started.md`, `configuration.md`, `monorepo.md`,
   `writing-a-plugin.md`, `writing-a-rule.md`, `plugins.md` (the shared set). Write them against
   [architecture.md](../architecture.md); mark anything that depends on an interface not merged yet
   with `<!-- pending T-00x -->` so it is checked before release.
4. Remove `PROVENANCE.md` (it describes private history).

Not in scope: code; anything that names a private project; product plugins.

## Done when

- Every file above exists; every link in them resolves (`lychee` or a small script, offline).
- The leak scan of the local rules prints nothing.
- `docs/guide/house-style.md` cites each public repository read and the item taken from it.

## Notes

- Reading public GitHub repositories is allowed; never put anything about our private projects in a
  query or a request.
- Russian is never used in files.

## Resume point

2026-11-19T17:53:44+00:00: merged before the public root; its work is in the root commit `59fb163`
and released in `v0.1.0`. Nothing is left.

Earlier:

2026-11-16T03:53:57+00:00: first pass merged. Follow-up on branch `agent/t-011-follow-up`, in
review:
README and guides aligned with architecture section 7 and section 16 (one plugin source form,
`extends` under `[project]`, repository-scope root `[rules]`, `[[profiles]]`, root steps and suites,
`--plugin` override, `--budget`/`--budget-fails`, baselines, `gate cache prune`, private assets, hooks
per clone, CI job keys); `report.finding(aspect, location, message)` in the rule guide; SVG wordmark
`docs/assets/logo.svg` in the README. Link check 68 links, 0 broken; leak scan clean on these paths.
Next: the second pass on the `<!-- pending T-00x -->` markers as T-002 to T-009 merge.

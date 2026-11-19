# Changelog of the git plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:git@<version>`.

## Unreleased

### Added

- `git/branch-name` takes a placeholder `template` (`:type`, `:ticket`, free placeholders taking
  `subject`, optional ones written `:name-`) as an alternative to `pattern`, exactly one of them,
  and checks as `@elsikora/git-branch-lint` checks: `types`, `subject`, `prohibited` (refused in
  hook runs only), `ignore`, `min-length` and `max-length`, each defaulting to the reference's
  value.
- `git/commit-message`: the message of the commit being made (`gate check --commit-message`) has the
  conventional form, checked as commitlint checks it with `@commitlint/config-conventional` and the
  rule levels of git-branch-lint's `commitlint.config.js`; one aspect per commitlint rule, every
  limit and the type list an option.
- The rules read the commit message and whether a git hook runs the check through the plugin
  interface (`message()`, `hook()`, interface 0.2.0); the generated hooks pass `--hook`.
- The preset `git/recommended`: `git/branch-name` with the template `:type/:ticket-:name`, and
  `git/commit-message` with `header-min-length` and `scope-max-length` as warnings, as in the
  reference.
- `git/commit-message` takes `header-pattern`, the header's regular expression with the named groups
  `type`, `scope` and `subject` (commitlint's `headerPattern`), `ignores`, regular expressions of
  messages that are not checked, and `default-ignores = false`, which drops commitlint's default
  ignores, so a policy written as one header expression and its own skip list needs no shell step.
  Both read a pattern as JavaScript reads it without flags; the rule page lists the subset, and a
  construct outside it is an error that names it.
- The rules declare the `[plugins.git] options` keys they read (`Rule::NEEDS`), and under which
  forms, so a key an enabled rule reads is a configuration error (exit 2) when the configuration
  loads, naming the rule, not an error when the rule runs.

### Changed

What passed under 0.1.0 and now fails, with no change to `gate.toml`:

- `git/branch-name` with only `pattern` (and perhaps `min-length` or `max-length`) gets the
  reference's defaults:
  - a name shorter than 5 or longer than 50 characters fails (0.1.0 had no limit unless one was
    set); set `min-length = 0` or `max-length = 0` to turn a limit off;
  - in a hook run (`--hook`, which the generated hooks pass), `main`, `master` and `release` are
    prohibited: committing on, or pushing to, one of them from a working copy fails; set
    `prohibited = []` to allow them;
  - in a hook run, main is checked like any branch (0.1.0 passed main always); outside a hook run
    main still passes;
  - `dev` passes every check but `prohibited` (`ignore`); set `ignore = []` to check it.
- Every pattern option of the plugin is read as JavaScript reads a regular expression without
  flags, in the subset the `git/commit-message` page lists: `git/branch-name`'s `pattern`,
  `template` and `subject`, `git/no-ci-skip`'s `directive-pattern` and `trailer-pattern`, and
  `release-subject`, `branch-pattern` and a form's `subject` in `[plugins.git] options`. `\w`, `\d`
  and `\b` are ASCII, so `^feature/\w+$` now refuses `feature/café`; syntax of Rust's `regex`
  crate such as `\A`, `\pL` or `[[:alpha:]]` means what JavaScript means by it; and an inline
  flag such as `(?i)` is refused as a configuration error. `git/no-ci-skip`'s defaults spell each
  letter in both cases instead of `(?i)` and match the trailer on a line of its own instead of
  `(?m)`, so they find what they found before.
- A wrong option or `[plugins.git] options` key ends the run as a configuration error (exit 2,
  `configuration error`), not as an environment error. A main reference that is missing, or an
  activation commit that is not an ancestor of main, stays an environment error: fetching the
  history may fix it.
- `git/no-ci-skip` also checks the message being written (`gate check --commit-message`, the
  `commit-msg` hook), `#` lines included, so a CI-skip directive fails the hook instead of the next
  check.
- The plugin is built for plugin interface 0.2.0 and loads only in a core of 0.2.0.
- `git/branch-name`, `commit-message` and `main-history` give each finding whose message names a
  line, a path or an option value a stable key (what it is about, never a line or column), so an
  edit above a finding or a reworded message keeps its fingerprint. These findings' fingerprints
  change once, with this release; `gate baseline prune` re-keys their baseline entries when the
  repository upgrades.

# 0.1.0

- `git/main-history`: every first-parent commit of main since activation matches one of the forms of
  `[plugins.git] options forms` and passes the checks that form runs; a commit that matches none is
  reported. The default forms are a clean `--no-ff` merge of a well-named branch, a release commit,
  and a documentation-only direct commit.
- `git/no-ci-skip`: no commit since activation carries a CI-skip directive, except the commits of a
  release form.
- `git/branch-name`: the branch a push updates, or else the checked-out branch, matches `pattern`,
  within the optional `min-length` and `max-length`; main and a detached HEAD pass.
- `[plugins.git] options forms`: the forms of the commits on main, shared by `git/main-history` and
  `git/no-ci-skip`. Per form a kind, an optional parent count, an optional subject pattern, and the
  checks of `git/main-history` it runs, each optional.
- `[plugins.git] options` require only `main`; `activation`, `release-subject` and the other keys
  are required by the rule, form or check that reads them, and the error names it. The plugin
  declares their schema, so the core checks them when the plugin loads.

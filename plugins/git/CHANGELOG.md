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
- `\w`, `\d` and `\b` in `pattern`, `template` and `subject` are ASCII, as in a JavaScript regular
  expression without the `u` flag: `^feature/\w+$` now refuses `feature/café`.
- `git/no-ci-skip` also checks the message being written (`gate check --commit-message`, the
  `commit-msg` hook), `#` lines included, so a CI-skip directive fails the hook instead of the next
  check.
- The plugin is built for plugin interface 0.2.0 and loads only in a core of 0.2.0.

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

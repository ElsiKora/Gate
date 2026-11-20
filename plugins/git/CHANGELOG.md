# Changelog of the git plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:git@<version>`.

# 0.3.0 (unreleased)

Built for the plugin interface `gate:plugin@0.3.0`; it loads only in a 0.3.0 core.

### Breaking Changes

- `git/commit-message` also checks the message of each commit a `gate check --push` run sends (the
  `pre-push` hook and the CI job) adds to a branch, oldest first, with every aspect and option, the
  ignores included: the commits reachable from the pushed head and neither from the remote ref's
  old tip nor from main (`[plugins.git] options main`), so a merge of main or a rebase onto it
  judges the branch's own commits only, and a new branch judges `main..head`, as `git/no-ci-skip`
  does. A finding is on the commit, starts with its short id, its target and key are the commit id,
  and its help names `git rebase -i <id>^` or an exception by id. A pushed tag and a `--rev` run
  check nothing more than before.

  For a consumer, a push that passed can now fail: a pushed commit whose message breaks the rule
  (made with `git commit -n`, cherry-picked, rebased or edited on the web) is reported by the
  `pre-push` hook and by the CI job, under the same aspects as in the `commit-msg` hook. Reword such
  a commit before the push (`git rebase -i`), or, when it is already shared, give it an exception by
  its commit id. Commits already on the remote ref or on main are not checked.
- `git/commit-message` and `git/no-ci-skip` declare `READS_MESSAGE`, so a
  `gate check --commit-message` run runs them; `git/branch-name` and `git/main-history` do not,
  and run only in the other hooks and CI.

### Added

- `git/commit-message` takes `prohibited-body-lines`, regular expressions of the body lines the
  message being written and the pushed commits' messages may not carry, empty by default: each is
  searched in every line after the header that is neither blank nor in the trailer block, and each
  line one matches is a finding of the new aspect `prohibited-body-lines`, an ignored message
  included. Prose that git's quarter rule lets a trailer block hold, above a `Signed-off-by`, is
  read as body lines too. A note such as `Generated with ...`, which is no trailer, can now be refused. A pattern
  that does not compile, or an empty one, is a configuration error.

### Bug Fixes

- `git/commit-message`'s `body-full-stop` judges the body above its trailer block, the block
  `git interpret-trailers` and `prohibited-trailers` find. A body that ends with a full stop and
  then a `Co-authored-by` block no longer fails it, also when the block holds an issue reference
  such as `Refs: #12` that starts the footer, and a body that is only a trailer block passes; a
  body without a full stop above the block still fails.

### Features

- The preset `git/adoption`: `git/branch-name` with the template `:type/:ticket-:name` and
  `git/commit-message`, both at `warning`. `git/recommended` now names each rule's level, `error`;
  its rules, template and aspect levels are unchanged.

## [0.2.1](https://github.com/ElsiKora/Gate/compare/plugin-git-v0.2.0...plugin-git-v0.2.1) (2026-11-20)

Built for the plugin interface `gate:plugin@0.2.0`; it loads in a 0.2.0 or 0.2.1 core.

### Added

- `git/commit-message` takes `prohibited-trailers`, regular expressions of the trailers the
  message being written (the `commit-msg` hook's) may not carry, such as a co-author the
  repository does not accept. The trailers are found as `git interpret-trailers` finds them, in
  the message's last paragraph, and each is matched as `Token: value`; every trailer a pattern
  matches is a finding of the new aspect `prohibited-trailers` that names it, an ignored message
  included. The same text in the body passes. A pattern that does not compile, or an empty one, is
  a configuration error (exit 2). Empty by default. ([2dc41a1](https://github.com/ElsiKora/Gate/commit/2dc41a170c96bdd5477dd116cb2d93a2dd8771ae))
- `git/main-history` takes `release-plugin`, the source of a plugin the repository releases,
  without its version: a release commit may then move that plugin's pin, the version in its
  `source` in each `gate.toml` and its entry in `gate.lock`, to the release's version, and nothing
  else in those files. `release-all-pins = true` also lets it move the core's pin and every other
  plugin's, as a plugin interface change needs. A lock entry's `url` and `sha256` change only with
  its version, and each old lock key gives way to exactly one new key. Anything else, the pin file
  added or deleted included, is a `release` finding (`#beyond-pin`, `#version`, `#unreadable`); the
  files are compared as parsed TOML. ([cb25331](https://github.com/ElsiKora/Gate/commit/cb2533134f222346d45a34eeced04f36d830bff0))

# [0.2.0](https://github.com/ElsiKora/Gate/compare/plugin-git-v0.1.0...plugin-git-v0.2.0) (2026-11-19)

### Added

- `git/branch-name` takes a placeholder `template` (`:type`, `:ticket`, free placeholders taking
  `subject`, optional ones written `:name-`) as an alternative to `pattern`, exactly one of them,
  and checks as `@elsikora/git-branch-lint` checks: `types`, `subject`, `prohibited` (refused in
  hook runs only), `ignore`, `min-length` and `max-length`, each defaulting to the reference's
  value. ([ee616b5](https://github.com/ElsiKora/Gate/commit/ee616b5b65ea4778584cd1fcd0754b8a700da43f))
- `git/commit-message`: the message of the commit being made (`gate check --commit-message`) has the
  conventional form, checked as commitlint checks it with `@commitlint/config-conventional` and the
  rule levels of git-branch-lint's `commitlint.config.js`; one aspect per commitlint rule, every
  limit and the type list an option. ([8cb8230](https://github.com/ElsiKora/Gate/commit/8cb8230d314774b49512092f4d364e91b0118d17))
- The rules read the commit message and whether a git hook runs the check through the plugin
  interface (`message()`, `hook()`, interface 0.2.0); the generated hooks pass `--hook`. ([ccad7cd](https://github.com/ElsiKora/Gate/commit/ccad7cd14c8a3bf624c600c889ae0f4ab6b4c086))
- The preset `git/recommended`: `git/branch-name` with the template `:type/:ticket-:name`, and
  `git/commit-message` with `header-min-length` and `scope-max-length` as warnings, as in the
  reference. ([a2905b4](https://github.com/ElsiKora/Gate/commit/a2905b456554a86f5c19bc33fb9b525efbc6d36f))
- `git/commit-message` takes `header-pattern`, the header's regular expression with the named groups
  `type`, `scope` and `subject` (commitlint's `headerPattern`), `ignores`, regular expressions of
  messages that are not checked, and `default-ignores = false`, which drops commitlint's default
  ignores, so a policy written as one header expression and its own skip list needs no shell step.
  Both read a pattern as JavaScript reads it without flags; the rule page lists the subset, and a
  construct outside it is an error that names it. ([7f89a9c](https://github.com/ElsiKora/Gate/commit/7f89a9cc5a0f5058591d844965ab1ec0b514dd5c))
- The rules declare the `[plugins.git] options` keys they read (`Rule::NEEDS`), and under which
  forms, so a key an enabled rule reads is a configuration error (exit 2) when the configuration
  loads, naming the rule, not an error when the rule runs. ([58800b7](https://github.com/ElsiKora/Gate/commit/58800b73ed09b06aebb3fc147aaa0d81e9d0befb))

### Changed

What passed under 0.1.0 and now fails, with no change to `gate.toml`:

- `git/branch-name` with only `pattern` (and perhaps `min-length` or `max-length`) gets the
  reference's defaults: ([ee616b5](https://github.com/ElsiKora/Gate/commit/ee616b5b65ea4778584cd1fcd0754b8a700da43f))
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
  `(?m)`, so they find what they found before. ([ac27d2d](https://github.com/ElsiKora/Gate/commit/ac27d2d3d3a2208b0a365b0e428287f05a50b714))
- A wrong option or `[plugins.git] options` key ends the run as a configuration error (exit 2,
  `configuration error`), not as an environment error. A main reference that is missing, or an
  activation commit that is not an ancestor of main, stays an environment error: fetching the
  history may fix it. ([883012d](https://github.com/ElsiKora/Gate/commit/883012d3811891eb9f818213dfe52814bd9d8ee6))
- `git/no-ci-skip` also checks the message being written (`gate check --commit-message`, the
  `commit-msg` hook), `#` lines included, so a CI-skip directive fails the hook instead of the next
  check. ([b01fbf7](https://github.com/ElsiKora/Gate/commit/b01fbf794489f5cad05ca696c8739838f2f8dcea))
- The plugin is built for plugin interface 0.2.0 and loads only in a core of 0.2.0. ([9358a89](https://github.com/ElsiKora/Gate/commit/9358a892d01e4613a0e4c19d0224e5ccaea9e03e))
- `git/branch-name`, `commit-message` and `main-history` give each finding whose message names a
  line, a path or an option value a stable key (what it is about, never a line or column), so an
  edit above a finding or a reworded message keeps its fingerprint. These findings' fingerprints
  change once, with this release; `gate baseline prune` re-keys their baseline entries when the
  repository upgrades. ([c688b5d](https://github.com/ElsiKora/Gate/commit/c688b5df1c1e379e4189f0b65accef02a1d3e883))

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

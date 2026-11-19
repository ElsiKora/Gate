# Changelog of the git plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:git@<version>`.

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

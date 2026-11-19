# T-020: `git/branch-name` and subject forms of `git/main-history`

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | the next git plugin release |
| Writer | "Gate 006" |
| Reviewer | a session that did not write it, named by the lead |
| Depends on | main at 86b7b04 |
| Touches | `plugins/git/**`, this file |

## Goal

A repository whose history policy is subject-based (a Conventional Commits subject, `Merge ...` and
`Revert "..."`) can turn `git/main-history` on instead of off, and a repository can check the name of
the branch it works on.

## Scope

1. `git/branch-name`: the branch a push updates (`--push`), or else the checked-out branch, matches
   the rule's `pattern`, and its length is within the optional `min-length` and `max-length`. A
   branch `[plugins.git] options main` names, and a detached HEAD, pass. How the host reads the
   branch (and the branch of a pull-request run) is another task's.
2. `git/main-history` classifies each first-parent commit of main by an ordered list of forms
   (`[plugins.git] options forms`, shared with `git/no-ci-skip`, which exempts a release form's commits):
   each form has a kind (`merge`, `release`, `direct`, the aspect of its findings), an optional parent
   count, an optional subject regular expression, and the checks it runs. The unedited-body,
   branch-name, sync-merge and merge-tree checks of a merge, the files check of a release and the
   files check of a direct commit are each optional per form. A commit that matches no form is
   reported. The default forms are the policy the rule had before.
3. `[plugins.git] options` require only what an enabled rule and its forms read: `main` always,
   `activation` for the history rules, `release-subject` for a release form.
4. Rule pages for both rules; the plugin changelog. Plugin only; the host is not touched.

## Done when

- Pass and fail tests for `git/branch-name` (a match, a mismatch, each length bound, main, detached
  HEAD, a bad option) and for each form of `git/main-history` (merge, release, direct, a subject-based
  policy, a commit matching no form, each form's option errors).
- `cargo fmt --all --check`, `cargo clippy --locked -p gate-plugin-git --all-targets -- -D warnings`,
  the same clippy for `wasm32-wasip2`, `cargo test --locked -p gate-plugin-git` and
  `cargo deny --locked check` exit 0 on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-18: review round at de3a9fe answered (push target, anchoring on the pages, a
merge subject without its branch capture, `pattern` required in the schema, the changelog, the
no-ci-skip exemption tied to a release form, which moved `forms` into `[plugins.git] options`); the
gates run on the fix commit. The host's branch reading is not in this task.

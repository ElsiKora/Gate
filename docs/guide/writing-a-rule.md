# Writing a Rule

A rule looks at what the core gives it, the tracked files of its scope and, if it asks, the git history,
and reports findings. This guide writes `todo/no-bare-todo`, a rule that reports `TODO` comments without
an issue reference, and then covers options, scopes, languages, history, aspects and fixtures. The
plugin around it is built in [writing-a-plugin.md](writing-a-plugin.md).

## 🧾 The Rule's Contract

| Part | Who decides |
| --- | --- |
| id, `<plugin>/<rule>`, lowercase kebab | the plugin |
| what it reads: scope, languages, history | the plugin, declared up front |
| options and their JSON Schema | the plugin; the core validates the repository's values before the rule runs |
| findings: message, location, target, help, aspect | the plugin |
| level (`error`, `warning`, `note`, `off`) | the repository, in `gate.toml`, never the plugin |
| norm (the repository's own rule id) | the repository |
| exceptions | the repository; the core applies them |
| order of findings, fingerprints, reports | the core |

## 1. The Rule

```rust
// src/no_bare_todo.rs
use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Location, Result, Rule, Run, Span};

/// Reports `TODO` comments that do not cite an issue.
#[derive(Debug, Default)]
pub struct NoBareTodo;

/// The options of `todo/no-bare-todo`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields, rename_all = "kebab-case")]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The issue reference a TODO must contain, as a regular expression.
    pub pattern: String,
    /// The file extensions to look at, without the dot.
    #[serde(default = "default_extensions")]
    pub extensions: Vec<String>,
}

fn default_extensions() -> Vec<String> {
    vec!["rs".to_owned(), "ts".to_owned(), "md".to_owned()]
}

impl Rule for NoBareTodo {
    type Options = Options;

    const NAME: &'static str = "no-bare-todo";
    const TITLE: &'static str = "TODO comments cite an issue";

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        let pattern = run.options().pattern.clone();
        let issue = regex::Regex::new(&pattern).map_err(|error| {
            Error::new(format!(
                "the option pattern is not a regular expression ({error}); fix it in gate.toml"
            ))
        })?;
        let wanted = run.options().extensions.clone();
        let extensions: Vec<&str> = wanted.iter().map(String::as_str).collect();
        let tree = run.tree();
        for path in tree.with_extensions(&extensions) {
            let Some(text) = tree.text(&path) else { continue };
            for (index, line) in text.lines().enumerate() {
                let Some(start) = line.find("TODO") else { continue };
                if issue.is_match(&line[start..]) {
                    continue;
                }
                let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
                let column = u32::try_from(line[..start].chars().count() + 1).unwrap_or(u32::MAX);
                let span = Span {
                    line: number,
                    column,
                    end_line: number,
                    end_column: column.saturating_add(3),
                };
                run.report()
                    .finding("", Location::span(path.clone(), span), "TODO without an issue reference")
                    .target(format!("{path}:{number}"))
                    .help(format!("cite the issue, for example `TODO(#123)`; the pattern is `{pattern}`"));
            }
        }
        Ok(())
    }
}
```

A rule is a `#[derive(Debug, Default)]` type that implements `gate_sdk::Rule`: `NAME`, `TITLE` and
`check` are required; `SCOPE`, `ASPECTS`, `LANGUAGES` and `READS_HISTORY` have defaults (project, none,
none, `false`). `check` reads through `run.tree()` and `run.options()` and reports through
`run.report()`.

The repository turns it on:

```toml
[rules]
"todo/no-bare-todo" = { level = "warning", norm = "RULE-DOC-002", options = { pattern = "#[0-9]+" } }
```

## 2. Options

- Options are a Rust type that derives `Deserialize` and `JsonSchema` through the SDK's re-exports,
  `gate_sdk::serde` and `gate_sdk::schemars`, with the `crate = ...` attributes shown above. The SDK
  generates the JSON Schema from it, and `describe` publishes it. The core validates the repository's
  values before your rule runs, so `check` receives a valid value.
- Use `deny_unknown_fields`: a misspelled option must be a configuration error, not a silently ignored
  key. Use `rename_all = "kebab-case"`: options are written in kebab case in `gate.toml`.
- Give defaults only where one value is right for almost everyone. A rule that needs project facts (a
  source root, a list of layers) takes them as required options; it never assumes a layout.
- Large data goes in an `options-file` on the repository's side; the rule sees the same type.
- An option that is valid by the schema but unusable (a regular expression that does not compile) is
  an `Err(Error::new(..))` with a message that names the option and what to do.

## 3. Scope

| `const SCOPE` | The rule runs | `run.tree()` holds |
| --- | --- | --- |
| `Scope::Project` (the default) | once per project that enables it | the project's tracked files, paths relative to the project root |
| `Scope::Repository` | once per run | every tracked file of the repository |

Choose `Project` unless the rule is about the repository as a whole (its history, its root files). A
project rule that needs another project's files gets them only when the repository grants a read path;
`tree.text` returns `None` for anything else.

## 4. Reading the Tree

`let tree = run.tree();` gives a `gate_sdk::Tree`:

| Call | Returns |
| --- | --- |
| `tree.paths()` | every tracked path in scope, sorted |
| `tree.under(directory)` | the tracked paths below a directory (no trailing slash), sorted |
| `tree.with_extensions(&["rs", "ts"])` | the tracked paths with one of the extensions (no dot), sorted |
| `tree.contains(path)` | whether the path is tracked and granted |
| `tree.text(path)` | the file decoded as UTF-8 with replacement characters, or `None` when it is not tracked or not granted |
| `tree.bytes(path)` | the file's bytes, or `None` |
| `tree.changed()` | the paths of the change under review (`--staged`, `--changed`), when the run has one; use it to narrow the work, never to widen what you report |

The core records every path you list or read. That record is the rule's cache key: when none of those
files changed, the call is not run again. So read only what you need, and never depend on anything you
did not read.

## 5. Languages

A rule that looks at source code of one language declares it, in its `impl Rule` block:

```rust
const LANGUAGES: &'static [&'static str] = &["typescript"];
```

The core then skips the plugin for a project or a change with no file of that language. To parse,
link the language library (`gate-typescript` for TypeScript, a crate of the Gate repository taken by
git like `gate-sdk`) instead of a parser of your own: it gives the syntax tree, module resolution and
source tables, and is pinned by the version your plugin is built with. Parse each file once per `check` call and share the result between the plugin's rules.

## 6. Reading History

A rule that needs commits declares it in its `impl Rule` block, and reads them through `run.git()`,
which fails for a rule that does not declare it:

```rust
const SCOPE: Scope = Scope::Repository;
const READS_HISTORY: bool = true;
```

| Call | Returns |
| --- | --- |
| `git.head()`, `git.branch()` | the current commit and branch, if any |
| `git.range()` | the range under review in `--push` and `--rev` runs: `base` (`None` for a new branch), `head`, `target_ref` |
| `git.resolve(rev)` | the full id of the commit a ref or revision names |
| `git.log(range)` | the commits of a range, newest first: `id`, `parents`, `message`, `author_date`, and `subject()`, `body()` |
| `git.changed_paths(parent, commit)` | the paths a commit changed |
| `git.read_at(commit, path)` | a file's bytes as it was at a commit |
| `git.is_ancestor(ancestor, descendant)` | whether one commit is an ancestor of the other |

Every call that takes a commit takes its full id; resolve a ref with `git.resolve` first. Check the range
under review, not the whole history; in `check`:

```rust
let git = run.git()?;
let Some(range) = git.range() else { return Ok(()) };
let Some(base) = range.base else { return Ok(()) };
for commit in git.log(&format!("{base}..{}", range.head)) {
    if commit.subject().chars().count() > 72 {
        run.report()
            .finding("", Location::commit(commit.id.clone()), "the subject is longer than 72 characters")
            .help("shorten the subject to 72 characters and move the detail to the body");
    }
}
```

## 7. Findings

A finding is reported with `run.report().finding(aspect, location, message)`, plus an optional
`.target(..)` and `.help(..)`. There is no level in the call: the level comes from the repository's
configuration alone.

- **Message**: one sentence that says what is wrong, without the rule id or the level (the report adds
  both): `TODO without an issue reference`.
- **Location**: `Location::file(path)` for a whole file, `Location::line(path, line)` for one line,
  `Location::span(path, span)` for a range (a `Span` of one-based line and column, end inclusive,
  columns counted in characters), `Location::commit(id)` for a commit, or `None` for a finding about no
  file.
- **Target**: what the finding is about, in a grammar the rule's page documents
  (`src/main.rs:3`); the repository's exceptions match it.
- **Help**: what to do about it, with an example.
- **Aspect**: a rule that checks several distinct things names each one (`hidden`, `nullable`), shown
  as `<rule>#hidden` in reports. A repository can then set a level or a norm per aspect, with `levels`
  and `norms` in the rule's entry. Declare the aspects in `const ASPECTS`; pass `""` when the rule has a
  single aspect.

The core orders findings by path, line, column, rule, aspect and message, and fingerprints them by rule,
aspect, path and message. Keep messages stable: a message that embeds a count or an iteration order
changes the fingerprint.

## 8. Errors

`Err` means the rule could not run: unusable options, a file the rule requires that is not tracked, an
internal fault. The run fails with exit code `2` and the message names the plugin. A violation is never
an `Err`.

## 9. Fixtures

Every rule ships a passing and a failing fixture (see
[writing-a-plugin.md](writing-a-plugin.md#4-test-on-fixture-trees)). Cover:

- the pass: a tree the rule must accept, including the near misses (`TODO(#12)` in the example);
- each way to fail, one case each, with the exact findings expected;
- each option that changes behaviour;
- for a history rule, a range with commits that pass and commits that fail. `gate-test` runs a rule on a
  tree with no git, so test a history rule by calling `gate_sdk::check` with a host of your own that
  implements `gate_sdk::Host` and `gate_sdk::GitHost` over a synthetic history.

## 10. The Rule's Page

Every rule has a page, `rules/<rule>.md` in the plugin's repository: what it checks and why, its scope,
whether it reads history, its aspects, its options with their defaults, an example finding, the grammar
of its target and how to fix it. Write it with the rule.

`gate explain <plugin>/<rule>` does not print the page: it prints what the repository's configuration
says about the rule (levels, norms and options in every scope, with the file and line they come from,
and the exceptions), or `no gate.toml configures it`.

## ⚠️ Common Pitfalls

- **Reading the clock or the environment**: a plugin has neither, so results stay exact and cacheable.
  A rule about dates takes the date as an option or uses the commit date from `git`.
- **Assuming a layout**: `src/` is an option, never a constant.
- **Reading outside the tracked tree**: there is nothing there; the plugin cannot see the file system.
- **Many plugins parsing the same files**: put rules that share a parse in one plugin.
- **A rule that changes files**: rules only report. Files are written by generators, through
  `gate sync`.

//! `agents/instruction-references`: backticked paths and commands in the agent instructions name
//! things that exist.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use gate_agents_format::markdown::code_spans;
use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::serde_json;
use gate_sdk::{Error, Location, Result, Rule, Run, Tree};
use globset::{GlobBuilder, GlobSetBuilder};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use regex::Regex;

use crate::load::{default_root, has_extension, strict_text};

static LOCATION_SUFFIX: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r":\d+(?::\d+)?$").ok());

/// The rule.
#[derive(Debug, Default)]
pub struct InstructionReferences;

/// The options of `agents/instruction-references`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The directory of the agent instructions, relative to the project; every `*.md` below it is
    /// read.
    pub root: String,
    /// The entry file of the agent instructions, read when it is tracked.
    pub entry: String,
    /// A backticked name without a slash is a path only when it ends with one of these.
    pub extensions: Vec<String>,
    /// The `package.json` whose `scripts` a command must name; read only with `command-pattern`.
    pub package_json: String,
    /// A backticked command matching this regular expression names, in its first group, a script
    /// that must exist. No default: the form of a command is the project's toolchain's, so the
    /// `command` aspect is checked only when this is set.
    pub command_pattern: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            root: default_root(),
            entry: "AGENTS.md".to_owned(),
            extensions: [
                ".md", ".mdc", ".mjs", ".cjs", ".js", ".ts", ".json", ".toml", ".yml", ".yaml",
                ".sh", ".txt",
            ]
            .map(str::to_owned)
            .to_vec(),
            package_json: "package.json".to_owned(),
            command_pattern: None,
        }
    }
}

impl Rule for InstructionReferences {
    type Options = Options;

    const NAME: &'static str = "instruction-references";
    const TITLE: &'static str = "Backticked paths and commands in agent instructions exist";
    const ASPECTS: &'static [&'static str] = &["command", "path"];

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        let tree = run.tree();
        let options = run.options();
        let command = options
            .command_pattern
            .as_deref()
            .map(|pattern| {
                Regex::new(pattern)
                    .ok()
                    .filter(|compiled| compiled.captures_len() > 1)
                    .ok_or_else(|| {
                        Error::config(format!(
                            "command-pattern `{pattern}` is not a regular expression with a group; \
                             write one whose first group captures the script name"
                        ))
                    })
            })
            .transpose()?;

        let paths = tree.paths();
        let root = options.root.trim_end_matches('/');
        let files: Vec<String> = std::iter::once(options.entry.clone())
            .filter(|entry| tree.contains(entry))
            .chain(
                tree.under(root)
                    .into_iter()
                    .filter(|path| has_extension(path, "md")),
            )
            .collect();
        if files.is_empty() {
            return Ok(());
        }

        let lookup = Lookup::new(&tree, paths, run.project())?;
        let scripts = match command {
            Some(_) => scripts(&tree, &options.package_json)?,
            None => BTreeSet::new(),
        };
        let mut findings = Vec::new();

        for path in &files {
            let Some(text) = strict_text(&tree, path)? else {
                continue;
            };
            for span in code_spans(&text) {
                let token = span.text.trim();
                if let Some(captures) = command.as_ref().and_then(|command| command.captures(token))
                {
                    let script = captures.get(1).map_or("", |found| found.as_str());
                    if !scripts.contains(script) {
                        let named = captures
                            .get(0)
                            .map_or("", |found| found.as_str())
                            .trim_end();
                        findings.push((
                            "command",
                            Location::line(path, span.line),
                            script.to_owned(),
                            format!("`{named}` names no {} script", options.package_json),
                        ));
                    }
                    continue;
                }

                let token = LOCATION_SUFFIX.as_ref().map_or_else(
                    || token.to_owned(),
                    |suffix| suffix.replace(token, "").into_owned(),
                );
                if !candidate(
                    &token,
                    |first| lookup.is_top_level(first),
                    &options.extensions,
                ) {
                    continue;
                }
                let directory = token.ends_with('/');
                let target = token.trim_end_matches('/');
                if !lookup.exists(target, directory) {
                    findings.push((
                        "path",
                        Location::line(path, span.line),
                        target.to_owned(),
                        format!("`{}` is neither tracked nor an ignored location", span.text),
                    ));
                }
            }
        }

        for (aspect, location, target, message) in findings {
            let help = if aspect == "command" {
                "name a script the package defines, or add the script"
            } else {
                "correct the path, or write prose without backticks"
            };
            run.report()
                .finding(aspect, location, message)
                .key(target.as_str())
                .target(target)
                .help(help);
        }
        Ok(())
    }
}

/// Where a cited path may exist: in the project (its tracked paths and its `.gitignore`) and, in a
/// project of a workspace, from the repository root (the granted paths and, when granted, the root
/// `.gitignore`).
struct Lookup {
    /// The listed paths, granted paths outside the project with `../`.
    paths: Vec<String>,
    /// The first segments of the project's own paths.
    project: BTreeSet<String>,
    /// Every listed path as the repository names it; empty for the repository itself.
    repository: Vec<String>,
    /// The first segments of the granted paths outside the project, as the repository names them.
    outside: BTreeSet<String>,
    /// The project's tracked `.gitignore`.
    ignored: Gitignore,
    /// The repository's root `.gitignore`, when the project is granted it.
    root_ignored: Option<Gitignore>,
}

impl Lookup {
    fn new(tree: &Tree<'_>, paths: Vec<String>, project: &str) -> Result<Self> {
        let project = project.trim_matches('/');
        let first = |path: &String| path.split('/').next().map(str::to_owned);
        let (repository, root_ignored) = if project.is_empty() {
            (Vec::new(), None)
        } else {
            let root = format!("{}.gitignore", "../".repeat(project.split('/').count()));
            let root_ignored = if tree.contains(&root) {
                Some(gitignore(tree, &root)?)
            } else {
                None
            };
            let named = paths
                .iter()
                .filter_map(|path| repository_path(project, path))
                .collect();
            (named, root_ignored)
        };
        let outside = paths
            .iter()
            .filter(|path| path.starts_with("../"))
            .filter_map(|path| repository_path(project, path))
            .filter_map(|path| first(&path))
            .collect();
        Ok(Self {
            project: paths
                .iter()
                .filter(|path| !path.starts_with("../"))
                .filter_map(first)
                .collect(),
            ignored: gitignore(tree, ".gitignore")?,
            paths,
            repository,
            outside,
            root_ignored,
        })
    }

    /// Whether a cited path may start with `first`: a top-level name of the project or of a
    /// granted path outside it.
    fn is_top_level(&self, first: &str) -> bool {
        self.project.contains(first) || self.outside.contains(first)
    }

    /// Whether `target` exists. Read inside the project (a bare name, a dot-prefixed path or one
    /// that starts with a top-level name of the project), it is tracked or ignored there; read
    /// from the repository root, it is a granted tracked path or ignored by the granted root
    /// `.gitignore`.
    fn exists(&self, target: &str, directory: bool) -> bool {
        let first = target.split('/').next().unwrap_or("");
        let inside =
            !target.contains('/') || first.starts_with('.') || self.project.contains(first);
        (inside
            && (exists(&self.paths, target, directory)
                || self
                    .ignored
                    .matched_path_or_any_parents(target, directory)
                    .is_ignore()))
            || exists(&self.repository, target, directory)
            || self.root_ignored.as_ref().is_some_and(|ignored| {
                ignored
                    .matched_path_or_any_parents(target, directory)
                    .is_ignore()
            })
    }
}

/// The patterns of the tracked ignore file at `path`; none when it is not tracked.
fn gitignore(tree: &Tree<'_>, path: &str) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new("");
    if let Some(text) = strict_text(tree, path)? {
        for line in text.lines() {
            // A line the matcher cannot read matches nothing, as Git ignores it.
            let _ = builder.add_line(None, line);
        }
    }
    Ok(builder.build().unwrap_or_else(|_| Gitignore::empty()))
}

/// The script names of the package file; none when it is absent or not JSON.
fn scripts(tree: &Tree<'_>, path: &str) -> Result<BTreeSet<String>> {
    Ok(strict_text(tree, path)?
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| {
            value
                .get("scripts")
                .and_then(serde_json::Value::as_object)
                .map(|scripts| scripts.keys().cloned().collect())
        })
        .unwrap_or_default())
}

/// Whether a backticked token names a repository path rather than prose.
///
/// Placeholders (`<name>`, `...`), shell syntax and relative import specifiers (`./x`, `../x`)
/// are prose, not repository paths.
fn candidate(token: &str, top_level: impl Fn(&str) -> bool, extensions: &[String]) -> bool {
    if token.is_empty()
        || token.contains(char::is_whitespace)
        || token.contains("://")
        || token.contains("...")
        || token.starts_with('-')
        || token.starts_with("./")
        || token.starts_with("../")
        || token
            .chars()
            .any(|found| matches!(found, '<' | '>' | '{' | '}' | '$' | '|' | '(' | ')' | '='))
    {
        return false;
    }
    let first = token.split('/').next().unwrap_or("");
    if token.contains('/') {
        first.starts_with('.') || top_level(first)
    } else {
        let extension_only = token
            .strip_prefix('.')
            .is_some_and(|rest| !rest.contains('.'));
        extensions
            .iter()
            .any(|extension| token.ends_with(extension.as_str()))
            && !token.starts_with('*')
            && !extension_only
    }
}

/// Whether `target` is among `paths`: a file, a directory, a glob that matches, or a bare file
/// name.
fn exists(paths: &[String], target: &str, directory: bool) -> bool {
    let under = |directory: &str| {
        paths.iter().any(|path| {
            path.strip_prefix(directory)
                .is_some_and(|rest| rest.starts_with('/'))
        })
    };
    if target.contains(['*', '?', '[']) {
        glob_matches(paths, target)
    } else if directory {
        under(target)
    } else if target.contains('/') {
        paths.iter().any(|path| path == target) || under(target)
    } else {
        // A bare name exists when some tracked file has it; a dot-prefixed bare name such as
        // `.constant.ts` may also name a file suffix.
        paths.iter().any(|candidate| {
            let name = candidate.rsplit('/').next().unwrap_or(candidate);
            name == target || (target.starts_with('.') && name.ends_with(target))
        })
    }
}

/// The repository path of `path`, which the tree of the project at `project` lists (a granted path
/// outside it starts with `../`); `None` when it leaves the repository.
fn repository_path(project: &str, path: &str) -> Option<String> {
    let mut segments: Vec<&str> = project
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            named => segments.push(named),
        }
    }
    Some(segments.join("/"))
}

/// Whether a glob matches a tracked file or one of its parent directories.
fn glob_matches(paths: &[String], pattern: &str) -> bool {
    let Some(set) = GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .ok()
        .and_then(|glob| GlobSetBuilder::new().add(glob).build().ok())
    else {
        return false;
    };
    paths.iter().any(|path| {
        set.is_match(path)
            || path
                .match_indices('/')
                .any(|(index, _)| set.is_match(&path[..index]))
    })
}

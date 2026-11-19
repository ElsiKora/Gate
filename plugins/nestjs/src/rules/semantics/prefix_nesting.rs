//! `nestjs/prefix-nesting`: sibling resource folders and same-kind files whose names share a
//! leading run of words are nested under a folder named after that prefix.
//!
//! The tree is every tracked path below `layout.source-root`, of any extension and with no folder
//! skipped. Each folder strictly below the source root is judged:
//!
//! - its child folders that directly hold one of `resource-files` (the resource folders): two or
//!   more sharing a prefix are a group, and one is enough when a sibling resource folder is named
//!   exactly the prefix;
//! - its `.ts` files of each kind (`<stem>.<kind>.ts`, not `.d.ts`; stems in `ignored-stems` left
//!   out): two or more stems sharing a prefix are a group, and one is enough when the folder
//!   `<folder>/<prefix>` already holds a file of that kind at any depth. A prefix equal to the
//!   folder's own name is not a group.
//!
//! Longer prefixes group first, and a name joins one group only. The finding is at the folder.

use std::collections::{BTreeMap, BTreeSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};

use crate::layout::PluginOptions;
use crate::shared::project;
use crate::shared::tree as walk;

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/prefix-nesting`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The files whose direct presence makes a folder a resource folder.
    pub resource_files: Vec<String>,
    /// File stems that never join a group.
    pub ignored_stems: Vec<String>,
    /// The separator between the words of a name; a prefix is a run of whole words. Not empty.
    #[schemars(length(min = 1))]
    pub word_separator: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            resource_files: vec!["module.ts".to_owned(), "controller.ts".to_owned()],
            ignored_stems: vec!["index".to_owned()],
            word_separator: "-".to_owned(),
        }
    }
}

impl gate_sdk::Rule for Rule {
    type Options = Options;

    const NAME: &'static str = "prefix-nesting";
    const TITLE: &'static str =
        "Sibling resources and same-kind files sharing a prefix are nested under it";
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let layout = PluginOptions::read(run)?.layout;
        let options = run.options();

        if options.word_separator.is_empty() {
            return Err(gate_sdk::Error::new(
                "the option word-separator is empty; set it to the text between the words of a name, such as \"-\"",
            ));
        }

        let grouping = Grouping {
            resource_files: options.resource_files.clone(),
            ignored_stems: options.ignored_stems.clone(),
            separator: options.word_separator.clone(),
        };
        let root = layout.source_root;
        let source = project::source(run);
        let tree = Tree::new(walk::files(&source, &root, &walk::UNSKIPPED));
        let below = crate::layout::folder_prefix(&root);

        for (directory, entries) in &tree.directories {
            if !directory.starts_with(&below) {
                continue;
            }

            for (count, prefix, kind) in grouping
                .resource_groups(&tree, directory, entries)
                .into_iter()
                .chain(grouping.file_groups(&tree, directory, entries))
            {
                run.report()
                    .finding(
                        "",
                        Location::file(directory.as_str()),
                        format!(
                            "{directory} has {count} {kind} sharing \"{prefix}\"; nest them under {directory}/{prefix}"
                        ),
                    )
                    .help(format!("move the {kind} into {directory}/{prefix}"))
                    .key(format!("{prefix}#{}", kind.replace(' ', "-")));
            }
        }

        Ok(())
    }
}

/// The child folders and files of every folder that holds a tracked file.
#[derive(Debug, Default)]
struct Tree {
    directories: BTreeMap<String, Entries>,
}

/// The names of one folder's child folders and files.
#[derive(Debug, Default)]
struct Entries {
    directories: BTreeSet<String>,
    files: BTreeSet<String>,
}

impl Tree {
    /// The tree of `paths`; every folder of a path, from its first segment, is an entry.
    fn new<'a>(paths: impl Iterator<Item = &'a str>) -> Self {
        let mut tree = Self::default();

        for path in paths {
            let mut parent = String::new();
            let parts: Vec<&str> = path.split('/').collect();

            for (index, part) in parts.iter().enumerate() {
                let entries = tree.directories.entry(parent.clone()).or_default();

                if index + 1 == parts.len() {
                    entries.files.insert((*part).to_owned());
                } else {
                    entries.directories.insert((*part).to_owned());
                }

                parent = if parent.is_empty() {
                    (*part).to_owned()
                } else {
                    format!("{parent}/{part}")
                };
            }
        }

        tree.directories.remove("");
        tree
    }

    fn entries(&self, directory: &str) -> Option<&Entries> {
        self.directories.get(directory)
    }

    /// Whether `directory` or a folder below it holds a file of `suffix`'s kind (`x.type.ts` or
    /// `type.ts`).
    fn contains_kind(&self, directory: &str, suffix: &str) -> bool {
        let bare = &suffix[1..];
        let nested = format!("{directory}/");
        let is_kind = |entries: &Entries| {
            entries
                .files
                .iter()
                .any(|name| name.ends_with(suffix) || name == bare)
        };

        // `a-b` sorts between `a` and `a/`, so the descendants are the `a/` range.
        self.entries(directory).is_some_and(is_kind)
            || self
                .directories
                .range(nested.clone()..)
                .take_while(|(path, _)| path.starts_with(&nested))
                .any(|(_, entries)| is_kind(entries))
    }
}

/// A group to nest: how many entries share the prefix, the prefix and what the entries are.
type Group = (usize, String, String);

/// The options that decide the groups.
struct Grouping {
    resource_files: Vec<String>,
    ignored_stems: Vec<String>,
    separator: String,
}

impl Grouping {
    /// Distinct shared prefixes of every pair, in first-seen order.
    fn shared_prefixes(&self, names: &[&str]) -> Vec<String> {
        let separator = self.separator.as_str();
        let mut prefixes: Vec<String> = Vec::new();

        for (index, left) in names.iter().enumerate() {
            for right in &names[index + 1..] {
                let right_parts: Vec<&str> = right.split(separator).collect();
                let common: Vec<&str> = left
                    .split(separator)
                    .enumerate()
                    .take_while(|(position, part)| right_parts.get(*position) == Some(part))
                    .map(|(_, part)| part)
                    .collect();
                let prefix = common.join(separator);

                if !prefix.is_empty() && !prefixes.contains(&prefix) {
                    prefixes.push(prefix);
                }
            }
        }

        prefixes
    }

    /// Child resource folders (those that directly hold a resource file) sharing a prefix.
    fn resource_groups(&self, tree: &Tree, directory: &str, entries: &Entries) -> Vec<Group> {
        let resources: Vec<&str> = entries
            .directories
            .iter()
            .map(String::as_str)
            .filter(|name| {
                tree.entries(&format!("{directory}/{name}"))
                    .is_some_and(|child| {
                        self.resource_files
                            .iter()
                            .any(|file| child.files.contains(file))
                    })
            })
            .collect();
        let mut grouped: BTreeSet<&str> = BTreeSet::new();
        let mut groups = Vec::new();

        for prefix in by_length(self.shared_prefixes(&resources)) {
            let qualified = format!("{prefix}{}", self.separator);
            let members: Vec<&str> = resources
                .iter()
                .copied()
                .filter(|name| !grouped.contains(name) && name.starts_with(&qualified))
                .collect();
            let needed = if resources.contains(&prefix.as_str()) {
                1
            } else {
                2
            };

            if members.len() < needed {
                continue;
            }

            groups.push((
                members.len(),
                prefix.clone(),
                "resource directories".to_owned(),
            ));
            grouped.extend(members);

            if let Some(name) = resources.iter().find(|name| **name == prefix) {
                grouped.insert(name);
            }
        }

        groups
    }

    /// Same-kind files sharing a prefix, or a prefix that already names a child folder holding a
    /// file of that kind.
    fn file_groups(&self, tree: &Tree, directory: &str, entries: &Entries) -> Vec<Group> {
        let separator = self.separator.as_str();
        let files: Vec<&str> = entries
            .files
            .iter()
            .map(String::as_str)
            .filter(|name| {
                std::path::Path::new(name)
                    .extension()
                    .is_some_and(|extension| extension == "ts")
            })
            .collect();
        let mut suffixes: Vec<String> = Vec::new();

        for suffix in files.iter().filter_map(|name| kind_suffix(name)) {
            if !suffixes.contains(&suffix) {
                suffixes.push(suffix);
            }
        }

        let own_name = directory.rsplit('/').next().unwrap_or(directory);
        let mut groups = Vec::new();

        for suffix in suffixes {
            let stems: Vec<&str> = files
                .iter()
                .filter_map(|name| name.strip_suffix(suffix.as_str()))
                .filter(|stem| !self.ignored_stems.iter().any(|ignored| ignored == stem))
                .collect();
            let mut prefixes = self.shared_prefixes(&stems);
            let mut owners: BTreeSet<String> = BTreeSet::new();

            for stem in &stems {
                let parts: Vec<&str> = stem.split(separator).collect();

                for length in 1..=parts.len() {
                    let prefix = parts[..length].join(separator);
                    let child = if prefix.is_empty() {
                        directory.to_owned()
                    } else {
                        format!("{directory}/{prefix}")
                    };

                    if tree.entries(&child).is_some() && tree.contains_kind(&child, &suffix) {
                        if !prefixes.contains(&prefix) {
                            prefixes.push(prefix.clone());
                        }

                        owners.insert(prefix);
                    }
                }
            }

            let mut grouped: BTreeSet<&str> = BTreeSet::new();

            for prefix in by_length(prefixes) {
                if prefix == own_name {
                    continue;
                }

                let qualified = format!("{prefix}{separator}");
                let members: Vec<&str> = stems
                    .iter()
                    .copied()
                    .filter(|stem| {
                        !grouped.contains(stem) && (*stem == prefix || stem.starts_with(&qualified))
                    })
                    .collect();
                let needed = if owners.contains(&prefix) { 1 } else { 2 };

                if members.len() < needed {
                    continue;
                }

                groups.push((members.len(), prefix, format!("{suffix} files")));
                grouped.extend(members);
            }
        }

        groups
    }
}

/// Longest prefixes first; equal lengths keep their first-seen order.
fn by_length(mut prefixes: Vec<String>) -> Vec<String> {
    prefixes.sort_by_key(|prefix| std::cmp::Reverse(prefix.chars().count()));
    prefixes
}

/// The kind suffix of a file name (`.type.ts` of `a.type.ts`), excluding declaration files.
fn kind_suffix(name: &str) -> Option<String> {
    if name.ends_with(".d.ts") {
        return None;
    }

    let base = name.strip_suffix(".ts")?;
    let (_, kind) = base.rsplit_once('.')?;

    (!kind.is_empty()).then(|| format!(".{kind}.ts"))
}

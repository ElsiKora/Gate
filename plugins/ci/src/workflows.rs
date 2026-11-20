//! The workflows a rule reads, and the options every rule of the plugin has.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Result, Tree};
use globset::GlobBuilder;

use gate_sdk::yaml::{self, Node};

/// The options of a rule whose only option is the workflow glob.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// A glob, relative to the repository, of the workflow files to read. `*` does not cross a
    /// `/`; `**` does; `{a,b}` is either.
    pub workflows: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            workflows: default_workflows(),
        }
    }
}

/// The default of every rule's `workflows` option.
pub(crate) fn default_workflows() -> String {
    ".github/workflows/*.{yml,yaml}".to_owned()
}

/// One parsed workflow file.
#[derive(Debug)]
pub(crate) struct Workflow {
    pub(crate) path: String,
    pub(crate) root: Node,
    /// The file's lines, for what the YAML reader drops (comments).
    pub(crate) lines: Vec<String>,
}

impl Workflow {
    /// The `name:` of the workflow, or its path when it has none.
    pub(crate) fn name(&self) -> &str {
        self.root
            .get("name")
            .and_then(Node::scalar)
            .unwrap_or(&self.path)
    }

    /// The job nodes under `jobs:`, in file order.
    pub(crate) fn jobs(&self) -> Vec<&Node> {
        self.job_entries().into_iter().map(|(_, job)| job).collect()
    }

    /// Each job under `jobs:` with its id node, in file order.
    pub(crate) fn job_entries(&self) -> Vec<(&Node, &Node)> {
        self.root
            .get("jobs")
            .and_then(Node::mapping)
            .unwrap_or_default()
            .iter()
            .filter(|(key, _)| key.scalar().is_some())
            .map(|entry| (&entry.0, &entry.1))
            .collect()
    }

    /// The text of the one-based line `line`, or empty past the end.
    pub(crate) fn line(&self, line: usize) -> &str {
        line.checked_sub(1)
            .and_then(|index| self.lines.get(index))
            .map_or("", String::as_str)
    }
}

/// A job's steps, or none.
pub(crate) fn steps(job: &Node) -> &[Node] {
    job.get("steps")
        .and_then(Node::sequence)
        .unwrap_or_default()
}

/// Whether `text` holds a GitHub expression (`${{ ... }}`), whose value differs per run.
pub(crate) fn is_expression(text: &str) -> bool {
    text.contains("${{")
}

/// Every tracked file the glob `workflows` matches, parsed, in path order.
///
/// # Errors
/// An invalid glob, a workflow that is not UTF-8, or one that is not valid YAML.
pub(crate) fn load(tree: &Tree<'_>, workflows: &str) -> Result<Vec<Workflow>> {
    let glob = GlobBuilder::new(workflows)
        .literal_separator(true)
        .build()
        .map_err(|error| {
            Error::config(format!(
                "workflows is not a valid glob ({error}); write one such as \
                 \".github/workflows/*.{{yml,yaml}}\""
            ))
        })?
        .compile_matcher();
    let mut found = Vec::new();

    for path in tree.paths().into_iter().filter(|path| glob.is_match(path)) {
        let Some(bytes) = tree.bytes(&path) else {
            continue;
        };
        let text = String::from_utf8(bytes).map_err(|_| {
            Error::new(format!(
                "{path} is not UTF-8 text; save the workflow as UTF-8"
            ))
        })?;
        let root = yaml::parse(&text).map_err(|error| {
            Error::new(format!(
                "{path}: {error}; fix the YAML so that GitHub Actions can read the workflow"
            ))
        })?;
        let lines = text.lines().map(str::to_owned).collect();
        found.push(Workflow { path, root, lines });
    }

    Ok(found)
}

//! `[plugins.node] options`: the project's kind and the repository's separate install roots.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Result, Run};

/// The `[plugins.node] options`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Settings {
    /// What the project is; the standard of its kind applies.
    pub kind: Kind,
    /// Directories that are installs of their own (a spike, a load test), each with its own
    /// lockfile and workspace file, and the job that makes them separate.
    #[serde(default)]
    pub separate: Vec<Separate>,
}

/// The kinds of project, the same names as the `docs` plugin's.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(crate = "gate_sdk::serde", rename_all = "kebab-case")]
#[schemars(crate = "gate_sdk::schemars")]
pub enum Kind {
    /// A deployed back-end service.
    Service,
    /// An application that runs in a browser.
    Frontend,
    /// Infrastructure as code.
    Infrastructure,
    /// A package published for others to use.
    Library,
    /// The root of a monorepo: it owns no code, only the workspace.
    MonorepoRoot,
}

impl Kind {
    /// Every kind, in the order of the standard's table.
    pub const ALL: [Self; 5] = [
        Self::Service,
        Self::Frontend,
        Self::Infrastructure,
        Self::Library,
        Self::MonorepoRoot,
    ];

    /// The name a configuration writes.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::Frontend => "frontend",
            Self::Infrastructure => "infrastructure",
            Self::Library => "library",
            Self::MonorepoRoot => "monorepo-root",
        }
    }
}

/// A directory that is an install of its own.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Separate {
    /// The directory, relative to the repository root, without a trailing slash.
    pub path: String,
    /// Why it is an install of its own, in one sentence.
    pub job: String,
}

impl Settings {
    /// The options of the run's `[plugins.node]` entry, checked.
    ///
    /// # Errors
    /// When a key is unknown or missing, `kind` is not one of the five, or a `separate` entry has
    /// no job or a path that is not a plain relative directory; the error names the key.
    pub fn read<O>(run: &Run<'_, O>) -> Result<Self> {
        let settings: Self = run.plugin_options().map_err(|error| {
            error.with_message(format!(
                "[plugins.node] options: {}; write options = {{ kind = \"<kind>\" }} with one of \
                 service, frontend, infrastructure, library, monorepo-root",
                error.message()
            ))
        })?;
        for (index, separate) in settings.separate.iter().enumerate() {
            if !plain_directory(&separate.path) {
                return Err(Error::config(format!(
                    "[plugins.node] options separate[{index}].path is \"{}\": write a directory \
                     relative to the repository root, without \"..\", \".\", a leading or a \
                     trailing slash",
                    separate.path
                )));
            }
            if separate.job.trim().is_empty() {
                return Err(Error::config(format!(
                    "[plugins.node] options separate[{index}].job is empty: say in one sentence \
                     why {} is an install of its own",
                    separate.path
                )));
            }
        }
        Ok(settings)
    }

    /// The install roots: the repository root (`""`) and every separate directory.
    #[must_use]
    pub fn install_roots(&self) -> Vec<String> {
        let mut roots = vec![String::new()];
        roots.extend(self.separate.iter().map(|separate| separate.path.clone()));
        roots
    }
}

/// Whether `path` is a non-empty relative directory of plain segments.
fn plain_directory(path: &str) -> bool {
    !path.is_empty()
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

//! The rules of the plugin, one module each, and how they report.

pub mod api_boundary;
pub mod asset_placement;
pub mod client_boundary;
pub mod colocated_files;
pub mod component_names;
pub mod env_access;
pub mod file_names;
pub mod hook_names;
pub mod import_alias;
pub mod import_direction;
pub mod import_paths;
pub mod layout;
pub mod literal_text;
pub mod messages;
pub mod packages;
pub mod route_config;
pub mod route_files;
pub mod state_placement;
pub mod style_files;
pub mod typescript_sources;

use std::collections::BTreeSet;

use gate_sdk::{Location, Run};
use gate_typescript::call::TypeScriptCall;

/// One finding as a rule collects it before reporting.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Found {
    /// The path the finding is at.
    pub path: String,
    /// The one-based line; `None` for the whole file or folder.
    pub line: Option<u32>,
    /// The aspect; empty for a rule without aspects.
    pub aspect: &'static str,
    /// What is wrong.
    pub message: String,
    /// What to do.
    pub help: String,
    /// The target an exception names: the path, or the path and the name the finding is about.
    pub target: String,
    /// The stable identity of what the finding is about within its path: the file or folder name,
    /// or the name the finding is about.
    pub key: String,
}

impl Found {
    /// A finding at `path` whose target is `path`.
    pub fn at(
        aspect: &'static str,
        path: &str,
        message: impl Into<String>,
        help: impl Into<String>,
    ) -> Self {
        Self {
            path: path.to_owned(),
            line: None,
            aspect,
            message: message.into(),
            help: help.into(),
            target: path.to_owned(),
            key: crate::standard::name_of(path).to_owned(),
        }
    }

    /// The finding at line `line`.
    #[must_use]
    pub fn line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }

    /// The finding with the target `<path>#<name>`, for a finding about one name of a file.
    #[must_use]
    pub fn named(mut self, name: &str) -> Self {
        self.target = format!("{}#{name}", self.path);
        name.clone_into(&mut self.key);
        self
    }
}

/// Reports `found` in path and line order, each finding once, keyed by what it is about so that
/// neither a moved line nor a reworded message changes its fingerprint.
pub fn report<O>(run: &mut Run<'_, O, TypeScriptCall>, found: impl IntoIterator<Item = Found>) {
    let found: BTreeSet<Found> = found.into_iter().collect();
    let mut seen = BTreeSet::new();

    for finding in found {
        if !seen.insert((finding.aspect, finding.target.clone(), finding.line)) {
            continue;
        }

        let location = match finding.line {
            Some(line) => Location::line(&finding.path, line as usize),
            None => Location::file(&finding.path),
        };
        run.report()
            .finding(finding.aspect, location, finding.message)
            .target(finding.target)
            .help(finding.help)
            .key(finding.key);
    }
}

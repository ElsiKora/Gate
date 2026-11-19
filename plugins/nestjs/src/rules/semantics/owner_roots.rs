//! `nestjs/owner-roots`: every accepted owner root holds source, the modules root holds only the
//! module planes, and no retired path remains.
//!
//! - `owner-root`: a `required-roots` entry with no tracked `.ts` file below it (the files below
//!   `layout.source-root`, declaration files and every folder included), or a folder directly in
//!   `layout.modules-root` that holds a tracked file and is not one of `layout.module-planes`;
//! - `shared-composition`: a `retired-paths` entry that is a tracked file or a folder holding one.

use std::collections::BTreeSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};

use crate::layout::PluginOptions;
use crate::shared::{project, tree};

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/owner-roots`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The folders that must each hold a `.ts` file below the source root.
    pub required_roots: Vec<String>,
    /// Paths, files or folders, that must not exist.
    #[serde(default)]
    pub retired_paths: Vec<String>,
}

impl gate_sdk::Rule for Rule {
    type Options = Options;

    const NAME: &'static str = "owner-roots";
    const TITLE: &'static str =
        "Every accepted owner root holds source and no retired owner remains";
    const ASPECTS: &'static [&'static str] = &["owner-root", "shared-composition"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        {
            let o = run.options();
            super::folders::check_all_below_root("required-roots", &o.required_roots)?;
            super::folders::check_all_below_root("retired-paths", &o.retired_paths)?;
        }

        let layout = PluginOptions::read(run)?.layout;
        let source = project::source(run);
        let sources: Vec<&str> =
            tree::typescript_files(&source, &layout.source_root, &tree::UNSKIPPED).collect();
        let mut findings = Vec::new();

        for root in &run.options().required_roots {
            if !sources.iter().any(|path| below(path, root)) {
                findings.push((
                    "owner-root",
                    root.clone(),
                    format!(
                        "{root} must contain source owned by the accepted architecture boundary"
                    ),
                    "add the owner's source, or drop the root from required-roots",
                ));
            }
        }

        let prefix = crate::layout::folder_prefix(&layout.modules_root);
        let folders: BTreeSet<&str> = tree::files(&source, &layout.modules_root, &tree::UNSKIPPED)
            .filter_map(|path| path.strip_prefix(&prefix)?.split_once('/'))
            .map(|(folder, _)| folder)
            .collect();

        for folder in folders {
            if !layout.module_planes.iter().any(|plane| plane == folder) {
                let path = format!("{prefix}{folder}");
                findings.push((
                    "owner-root",
                    path.clone(),
                    format!("{path} is not an accepted architecture owner root"),
                    "move the folder into one of the module planes",
                ));
            }
        }

        let files = run.tree();

        for retired in &run.options().retired_paths {
            if files.contains(retired) || !files.under(retired).is_empty() {
                findings.push((
                    "shared-composition",
                    retired.clone(),
                    format!("{retired} is a stale Nest composition owner and must be removed"),
                    "remove the retired path",
                ));
            }
        }

        for (aspect, path, message, help) in findings {
            run.report()
                .finding(aspect, Location::file(path), message)
                .help(help);
        }

        Ok(())
    }
}

/// Whether `path` lies below the folder `folder`.
fn below(path: &str, folder: &str) -> bool {
    path.strip_prefix(folder)
        .is_some_and(|rest| rest.starts_with('/'))
}

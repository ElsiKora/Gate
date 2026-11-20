//! `repository/large-files`: no tracked file is large, unless the project declares it with its
//! job. The rule asks each file's size and reads no content.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Location, Rule, Run, Scope};

/// The rule.
#[derive(Debug, Default)]
pub struct LargeFiles;

/// A large file the project needs, with what it is for.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Extra {
    /// The tracked path, relative to the repository.
    pub path: String,
    /// What the file is for, for the reader of the configuration.
    pub job: String,
}

/// The options of `repository/large-files`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// A file over this many bytes is `large`.
    pub large_bytes: u64,
    /// A file over this many bytes is `huge`, and not also `large`.
    pub huge_bytes: u64,
    /// The large files the project needs, each with its job; they are not reported.
    pub extra: Vec<Extra>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            large_bytes: 1_048_576,
            huge_bytes: 5_242_880,
            extra: Vec::new(),
        }
    }
}

impl Rule for LargeFiles {
    type Options = Options;

    const NAME: &'static str = "large-files";
    const TITLE: &'static str = "No large file is tracked unless the project needs it";
    const ASPECTS: &'static [&'static str] = &["large", "huge", "extra-missing"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let options = run.options();
        let (large, huge) = (options.large_bytes, options.huge_bytes);
        if huge < large {
            return Err(Error::config(format!(
                "huge-bytes ({huge}) is below large-bytes ({large}); set huge-bytes to at least large-bytes"
            )));
        }
        let extra: Vec<String> = options.extra.iter().map(|item| item.path.clone()).collect();
        let tree = run.tree();
        let paths = tree.paths();
        let report = run.report();

        for path in &extra {
            if paths.binary_search(path).is_err() {
                report
                    .finding(
                        "extra-missing",
                        None::<Location>,
                        format!("extra names {path}, which is not tracked"),
                    )
                    .target(path.clone())
                    .help("remove the entry, or fix its path")
                    .key(path.clone());
            }
        }

        for path in &paths {
            if extra.contains(path) {
                continue;
            }
            let Some(size) = tree.size(path) else {
                continue;
            };
            let (aspect, limit) = if size > huge {
                ("huge", huge)
            } else if size > large {
                ("large", large)
            } else {
                continue;
            };
            report
                .finding(
                    aspect,
                    Location::file(path.clone()),
                    format!("{path} is {size} bytes, over {limit}"),
                )
                .target(path.clone())
                .help("move it to Git LFS or to storage, or, when the project needs it here, declare it in `extra` with its job")
                .key("size");
        }

        Ok(())
    }
}

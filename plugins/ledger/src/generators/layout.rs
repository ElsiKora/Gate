//! `ledger/layout`: seeds the minimum the ledger rules read, the manifest and the five shared files,
//! at the paths the project configures. `gate sync` writes a seed only where nothing exists, so the
//! ledger is the project's from the moment it is written, and an empty ledger is a complete state.

use std::fmt::Write as _;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::serde_json::{self, Value};
use gate_sdk::{Generate, Generator, Result};

use super::decision_counts::{BLOCK, render};
use crate::block;
use crate::settings::{Settings, check_path};
use crate::text::join;

/// The generator.
#[derive(Debug, Default)]
pub struct Layout;

/// The options of `ledger/layout`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The ledger manifest, relative to the project, in normal form: the file `ledger/manifest`
    /// reads (its option `manifest`).
    pub manifest: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            manifest: "docs/ledger/manifest.json".to_owned(),
        }
    }
}

impl Generator for Layout {
    type Options = Options;

    const NAME: &'static str = "layout";
    const TITLE: &'static str = "Seeds the ledger manifest and the shared ledger files";

    fn generate(&self, run: &mut Generate<'_, Options>) -> Result<()> {
        let settings = Settings::checked(run.plugin_options()?)?;
        let manifest = run.options().manifest.clone();
        check_path("[generate] ledger/layout manifest", &manifest)?;
        let shared = shared(&settings);
        let root = common_root(shared.iter().map(|(path, _)| path.as_str()));
        let tree = run.tree();

        // A tracked manifest names the ledger's own files: each one is seeded with only a title.
        let mut seeds: Vec<(String, String)> = Vec::new();
        if let Some(text) = tree.text(&manifest) {
            for path in canonical(&text) {
                let heading = title(&path);
                seeds.push((path, format!("# {heading}\n")));
            }
        }
        seeds.push((manifest, manifest_seed(&root, &shared)));
        for (path, title) in &shared {
            let mut text = format!("# {title}\n");
            if *path == settings.decisions {
                text.push_str(&decision_index_body());
            }
            seeds.push((path.clone(), text));
        }
        // Only a path that is not tracked is seeded: a tracked file is the project's, and another
        // generator may write it (`ledger/decision-counts` the index, `ledger/review-progress` a
        // roadmap), which `gate sync` refuses for two outputs of one path, seed or not.
        for (path, text) in seeds {
            if !tree.contains(&path) {
                run.seed(path, text);
            }
        }
        Ok(())
    }
}

/// The header of the decision index's table, the structure `ledger/decisions` reads its rows from.
const DECISION_TABLE: &str =
    "| ID | Status | Title | Canonical file |\n| --- | --- | --- | --- |\n";

/// The decision index below its title: the block of `ledger/decision-counts` holding the counts
/// of an empty index, written as that generator writes it, so its next run changes nothing, and
/// the header of the table.
fn decision_index_body() -> String {
    let empty =
        format!("\n<!-- gate:begin {BLOCK} -->\n<!-- gate:end {BLOCK} -->\n\n{DECISION_TABLE}");
    block::replace(&empty, BLOCK, &render(0, 0)).unwrap_or(empty)
}

/// The five shared files and their titles, in the manifest's order; a path two options name is
/// listed once.
fn shared(settings: &Settings) -> Vec<(String, &'static str)> {
    let mut shared: Vec<(String, &'static str)> = Vec::new();
    for (path, title) in [
        (&settings.decisions, "Decision index"),
        (&settings.implementation, "Implementation slices"),
        (&settings.verification, "Verification"),
        (&settings.follow_ups, "Follow-ups"),
        (&settings.blockers, "Production blockers"),
    ] {
        if !shared.iter().any(|(listed, _)| listed == path) {
            shared.push((path.clone(), title));
        }
    }
    shared
}

/// The deepest directory that holds every one of `paths`; `.` when it is the project root.
fn common_root<'p>(paths: impl Iterator<Item = &'p str>) -> String {
    let mut common: Option<Vec<&str>> = None;
    for path in paths {
        let mut directory: Vec<&str> = path.split('/').collect();
        directory.pop();
        common = Some(match common {
            None => directory,
            Some(common) => common
                .into_iter()
                .zip(directory)
                .take_while(|(left, right)| left == right)
                .map(|(left, _)| left)
                .collect(),
        });
    }
    let common = common.unwrap_or_default().join("/");
    if common.is_empty() {
        ".".to_owned()
    } else {
        common
    }
}

/// The seeded manifest: the shared files under their common root, no identifier family and no
/// forbidden reference, with its keys in the order the manifest page lists them.
fn manifest_seed(root: &str, shared: &[(String, &str)]) -> String {
    let quote = |text: &str| Value::String(text.to_owned()).to_string();
    let files: Vec<String> = shared
        .iter()
        .map(|(path, _)| {
            let relative = path
                .strip_prefix(root)
                .and_then(|rest| rest.strip_prefix('/'))
                .unwrap_or(path);
            format!("    {}", quote(relative))
        })
        .collect();
    let mut text = String::from("{\n");
    let _ = writeln!(text, "  \"canonicalRoot\": {},", quote(root));
    let _ = writeln!(text, "  \"canonicalFiles\": [\n{}\n  ],", files.join(",\n"));
    text.push_str("  \"idFamilies\": {},\n  \"forbiddenReferences\": []\n}\n");
    text
}

/// The project-relative paths of the `canonicalFiles` a manifest lists under its `canonicalRoot`;
/// none when the manifest is not that shape, which `ledger/manifest` reports. A path that leaves
/// the project is skipped.
fn canonical(text: &str) -> Vec<String> {
    let Ok(manifest) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let Some(root) = manifest.get("canonicalRoot").and_then(Value::as_str) else {
        return Vec::new();
    };
    let files = manifest
        .get("canonicalFiles")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    files
        .iter()
        .filter_map(Value::as_str)
        .filter(|file| !root.starts_with('/') && !file.starts_with('/'))
        .map(|file| join(&[root, file]))
        .filter(|path| {
            !path.is_empty() && !path.ends_with('/') && path.split('/').all(|part| part != "..")
        })
        .collect()
}

/// The title of a canonical file the manifest lists: its name without the extension, words
/// separated by spaces, the first letter in uppercase.
fn title(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let words = stem.replace(['-', '_'], " ");
    let mut characters = words.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::{canonical, common_root, title};

    #[test]
    fn the_root_is_the_deepest_directory_of_every_shared_file() {
        assert_eq!(
            common_root(["docs/ledger/a.md", "docs/ledger/b.md"].into_iter()),
            "docs/ledger"
        );
        assert_eq!(
            common_root(["docs/ledger/a.md", "docs/plans/b.md"].into_iter()),
            "docs"
        );
        assert_eq!(common_root(["a.md", "docs/b.md"].into_iter()), ".");
    }

    #[test]
    fn a_canonical_file_outside_the_project_is_not_seeded() {
        assert_eq!(
            canonical(
                r#"{"canonicalRoot": "docs/ledger", "canonicalFiles": ["a.md", "../../../x.md", "/etc/x", "plans/b.md"]}"#
            ),
            ["docs/ledger/a.md", "docs/ledger/plans/b.md"]
        );
        assert_eq!(canonical("not json"), Vec::<String>::new());
        assert_eq!(
            canonical(r#"{"canonicalFiles": ["a.md"]}"#),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_canonical_files_title_is_its_stem_in_words() {
        assert_eq!(title("docs/ledger/review_notes.md"), "Review notes");
        assert_eq!(title("docs/ledger/risk-register.md"), "Risk register");
        assert_eq!(title("NOTES"), "NOTES");
    }
}

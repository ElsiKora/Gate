//! `nestjs/repeated-segment`: a file name does not repeat a folder segment its path already
//! represents.
//!
//! Every tracked `<stem>.<kind>.ts` file below `layout.source-root` (no folder skipped, not
//! `.d.ts`, stems in `ignored-stems` left out) is judged. A folder segment of its path is repeated
//! when the segment's dash-separated words occur, contiguous and whole, among the stem's words. The
//! structural segments are never judged: the source root's own segments, a `layout.source-owners`
//! folder directly below the source root, and a `layout.module-planes` folder directly below
//! `layout.modules-root`. The first repeated segment is reported, once per file.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};

use crate::layout::{Layout, PluginOptions};
use crate::shared::tree;

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/repeated-segment`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// File stems that are never judged.
    pub ignored_stems: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            ignored_stems: vec!["index".to_owned()],
        }
    }
}

impl gate_sdk::Rule<crate::Nest> for Rule {
    type Options = Options;

    const NAME: &'static str = "repeated-segment";
    const TITLE: &'static str =
        "A file name does not repeat a folder segment its path already represents";
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            let structure = Structure::new(&layout, run.options().ignored_stems.clone());

            let paths: Vec<String> =
                tree::files(shared.source(), &layout.source_root, &tree::UNSKIPPED)
                    .map(str::to_owned)
                    .collect();

            for path in paths {
                if let Some(segment) = structure.repeated_segment(&path) {
                    run.report()
                        .finding(
                            "",
                            Location::file(path.as_str()),
                            format!(
                                "{path} repeats parent folder segment \"{segment}\" in its filename"
                            ),
                        )
                        .help(format!(
                            "drop \"{segment}\" from the file name; the folder already says it"
                        ));
                }
            }

            Ok(())
        })
    }
}

/// The segments of a folder path; none for the empty path.
fn segments(path: &str) -> Vec<&str> {
    if path.is_empty() {
        Vec::new()
    } else {
        path.split('/').collect()
    }
}

/// The structural folders of the layout and the stems that are never judged.
struct Structure<'l> {
    source_root: Vec<&'l str>,
    source_owners: &'l [String],
    modules_root: Vec<&'l str>,
    module_planes: &'l [String],
    ignored_stems: Vec<String>,
}

impl<'l> Structure<'l> {
    fn new(layout: &'l Layout, ignored_stems: Vec<String>) -> Self {
        Self {
            source_root: segments(&layout.source_root),
            source_owners: &layout.source_owners,
            modules_root: segments(&layout.modules_root),
            module_planes: &layout.module_planes,
            ignored_stems,
        }
    }

    /// Whether the segment at `index` of `folders` is structural: a segment of the source root, a
    /// source owner directly below it, or a module plane directly below the modules root.
    fn structural(&self, folders: &[&str], index: usize) -> bool {
        let segment = folders[index];

        index < self.source_root.len()
            || (index == self.source_root.len()
                && self.source_owners.iter().any(|owner| owner == segment))
            || (index == self.modules_root.len()
                && folders.starts_with(&self.modules_root)
                && self.module_planes.iter().any(|plane| plane == segment))
    }

    /// The first folder segment of `path` that its file stem repeats as whole dash-separated
    /// words, skipping the structural segments. Only `<stem>.<kind>.ts` files have a stem.
    fn repeated_segment<'p>(&self, path: &'p str) -> Option<&'p str> {
        let (directory, name) = path.rsplit_once('/')?;
        let base = name.strip_suffix(".ts")?;

        if name.ends_with(".d.ts") {
            return None;
        }

        let (stem, kind) = base.rsplit_once('.')?;

        if kind.is_empty() || self.ignored_stems.iter().any(|ignored| ignored == stem) {
            return None;
        }

        let folders: Vec<&str> = directory.split('/').collect();
        let stem_parts: Vec<&str> = stem.split('-').collect();

        folders.iter().enumerate().find_map(|(index, segment)| {
            let parts: Vec<&str> = segment.split('-').collect();
            let repeated = (0..stem_parts.len()).any(|start| {
                parts
                    .iter()
                    .enumerate()
                    .all(|(offset, part)| stem_parts.get(start + offset) == Some(part))
            });

            (!self.structural(&folders, index) && repeated).then_some(*segment)
        })
    }
}

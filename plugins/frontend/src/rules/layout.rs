//! `frontend/layout`: every file sits in the tree of the standard: code under `src/`, `src/` in
//! layers, a sliced layer in kebab-case slices with an `index.ts`, a slice in segments, the shared
//! layer in segments, and only the files the pack allows directly in `src/`.

use std::collections::BTreeSet;

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SOURCE_EXTENSIONS;
use gate_typescript::call::TypeScriptCall;

use crate::code::Project;
use crate::options::{PACK, Pack};
use crate::rules::{Found, report};
use crate::standard::{
    FileName, Layer, SHARED_SEGMENTS, SLICE_SEGMENTS, SOURCE_ROOT, kebab, name_of,
};

/// The rule.
#[derive(Debug, Default)]
pub struct Layout;

impl Rule<TypeScriptCall> for Layout {
    type Options = NoOptions;

    const NAME: &'static str = "layout";
    const TITLE: &'static str = "Every file sits in the tree of the frontend standard";
    const ASPECTS: &'static [&'static str] = &[
        "source-root",
        "layer",
        "slice",
        "segment",
        "shared",
        "root-file",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, NoOptions, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let found = check(&project);
            report(run, found);
            Ok(())
        })
    }
}

fn check(project: &Project<'_>) -> Vec<Found> {
    let pack = project.pack;
    let tracked: BTreeSet<&str> = project.paths.iter().copied().collect();
    let mut found = Vec::new();

    for path in &project.paths {
        let Some(rest) = path.strip_prefix("src/") else {
            outside(path, &mut found);
            continue;
        };
        let parts: Vec<&str> = rest.split('/').collect();

        if parts.len() == 1 {
            root_file(pack, path, parts[0], &mut found);
        } else if pack.library() {
            library(&parts, &mut found);
        } else {
            app(path, &parts, &tracked, &mut found);
        }
    }

    found
}

/// A path outside `src/`: code is reported at its top folder, except at the project root (the
/// toolchain's configuration), in a folder whose name starts with `.` (a tool's) and in `public/`.
fn outside(path: &str, found: &mut Vec<Found>) {
    let Some((top, _)) = path.split_once('/') else {
        return;
    };
    let extension = FileName::new(name_of(path)).extension;

    if top.starts_with('.') || top == "public" || !SOURCE_EXTENSIONS.contains(&extension) {
        return;
    }

    found.push(Found::at(
        "source-root",
        top,
        format!("{top}/ holds code outside {SOURCE_ROOT}/"),
        format!("move the code into the layers of {SOURCE_ROOT}/; configuration files stay at the project root"),
    ));
}

/// A file directly in `src/`.
fn root_file(pack: Pack, path: &str, name: &str, found: &mut Vec<Found>) {
    if name.starts_with('.') || pack.root_files().contains(&name) {
        return;
    }

    found.push(Found::at(
        "root-file",
        path,
        format!(
            "{path} sits directly in {SOURCE_ROOT}/, where the {} pack allows only {}",
            pack.name(),
            pack.root_files().join(", ")
        ),
        "move it into a layer",
    ));
}

/// A file of a library, whose source root is one shared layer.
fn library(parts: &[&str], found: &mut Vec<Found>) {
    let segment = parts[0];
    if !Pack::ReactLibrary.shared_segments().contains(&segment) {
        let folder = format!("{SOURCE_ROOT}/{segment}");
        found.push(Found::at(
            "shared",
            &folder,
            format!(
                "{folder}/ is not a segment of a library: {}",
                Pack::ReactLibrary.shared_segments().join(", ")
            ),
            "move its files into the segment of their kind",
        ));
    }
}

/// A file of an app, below a folder of `src/`.
fn app(path: &str, parts: &[&str], tracked: &BTreeSet<&str>, found: &mut Vec<Found>) {
    let Some(layer) = Layer::from_name(parts[0]) else {
        let folder = format!("{SOURCE_ROOT}/{}", parts[0]);
        found.push(Found::at(
            "layer",
            &folder,
            format!(
                "{folder}/ is not a layer: the layers are {}",
                Layer::ALL.map(Layer::name).join(", ")
            ),
            "move its files into the layer and slice of their job",
        ));
        return;
    };
    let layer_folder = format!("{SOURCE_ROOT}/{}", layer.name());

    if layer == Layer::App {
        return;
    }

    if parts.len() == 2 {
        let (aspect, holds) = if layer == Layer::Shared {
            ("shared", "segments")
        } else {
            ("slice", "slices")
        };
        found.push(Found::at(
            aspect,
            path,
            format!("{path} sits directly in {layer_folder}/, which holds {holds} only"),
            format!("move it into one of the {holds} of {layer_folder}/"),
        ));
        return;
    }

    let unit = parts[1];
    let unit_folder = format!("{layer_folder}/{unit}");

    if layer == Layer::Shared {
        if !SHARED_SEGMENTS.contains(&unit) {
            found.push(Found::at(
                "shared",
                &unit_folder,
                format!(
                    "{unit_folder}/ is not a segment of the shared layer: {}; shared holds no slices",
                    SHARED_SEGMENTS.join(", ")
                ),
                "move business code into an entity, feature or widget, and the rest into the segment of its kind",
            ));
        }
        return;
    }

    if !kebab(unit) {
        found.push(Found::at(
            "slice",
            &unit_folder,
            format!("slice {unit_folder}/ is not kebab-case"),
            "rename the slice folder in kebab-case",
        ));
    }

    if !tracked.contains(format!("{unit_folder}/index.ts").as_str()) {
        found.push(
            Found::at(
                "slice",
                &unit_folder,
                format!("slice {unit_folder}/ has no index.ts, its public surface"),
                "add index.ts and export from it what other slices may use",
            )
            .named("index.ts"),
        );
    }

    if parts.len() == 3 {
        if parts[2] != "index.ts" {
            found.push(Found::at(
                "segment",
                path,
                format!("{path} sits in the slice root, which holds index.ts and segments only"),
                format!("move it into a segment: {}", SLICE_SEGMENTS.join(", ")),
            ));
        }
        return;
    }

    let segment = parts[2];
    if !SLICE_SEGMENTS.contains(&segment) {
        let folder = format!("{unit_folder}/{segment}");
        found.push(Found::at(
            "segment",
            &folder,
            format!(
                "{folder}/ is not a segment: a slice holds {}",
                SLICE_SEGMENTS.join(", ")
            ),
            "move its files into the segment of their kind",
        ));
    }
}

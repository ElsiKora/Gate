//! `frontend/layout`: every file sits in the tree of the standard: code under `src/` (and, in
//! Next.js, the root `app/` beside a root `pages/README.md`), `src/` in layers, a sliced layer in
//! kebab-case slices or one level of groups, a slice in its public API and segments, the app and
//! shared layers in their segments, and only the files the pack allows directly in `src/`.

use std::collections::BTreeSet;

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SOURCE_EXTENSIONS;
use gate_typescript::call::TypeScriptCall;

use crate::code::Project;
use crate::options::{PACK, Pack};
use crate::rules::{Found, report};
use crate::standard::{
    CROSS_IMPORT, FileName, Groups, Layer, NEXT_APP, NEXT_PAGES_README, OPEN_SEGMENTS,
    SLICE_SEGMENTS, SOURCE_ROOT, VITE_ROUTE_TREE, kebab, name_of,
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
        "next-pages",
        "layer",
        "slice",
        "group",
        "segment",
        "reserved",
        "app",
        "shared",
        "cross-import",
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

/// The project's tree as this rule walks it, and what it found.
struct Tree<'t> {
    pack: Pack,
    tracked: BTreeSet<&'t str>,
    groups: &'t Groups,
    found: Vec<Found>,
}

fn check(project: &Project<'_, '_>) -> Vec<Found> {
    let mut tree = Tree {
        pack: project.pack,
        tracked: project.paths.iter().copied().collect(),
        groups: &project.groups,
        found: Vec::new(),
    };

    if tree.pack == Pack::Nextjs && !tree.tracked.contains(NEXT_PAGES_README) {
        tree.found.push(
            Found::at(
                "next-pages",
                "pages",
                format!("the project has no {NEXT_PAGES_README}"),
                format!(
                    "add {NEXT_PAGES_README}, which keeps Next.js from reading {SOURCE_ROOT}/pages as the Pages Router"
                ),
            )
            .named("README.md"),
        );
    }

    for path in &project.paths {
        let Some(rest) = path.strip_prefix("src/") else {
            tree.outside(path);
            continue;
        };
        let parts: Vec<&str> = rest.split('/').collect();

        if parts.len() == 1 {
            tree.root_file(path, parts[0]);
        } else if tree.pack.library() {
            tree.library(&parts);
        } else {
            tree.app(path, &parts);
        }
    }

    tree.found
}

impl Tree<'_> {
    /// A path outside `src/`: code is reported at its top folder, except at the project root (the
    /// toolchain's configuration), in a folder whose name starts with `.` (a tool's), in `public/`,
    /// and in Next.js the root `app/`; in Next.js the root `pages/` holds its `README.md` only.
    fn outside(&mut self, path: &str) {
        let Some((top, _)) = path.split_once('/') else {
            return;
        };
        let next = self.pack == Pack::Nextjs;

        if next && top == "pages" {
            if path != NEXT_PAGES_README {
                self.found.push(Found::at(
                    "next-pages",
                    path,
                    format!("{path} is in the root pages/, which holds only README.md"),
                    format!(
                        "move it to its layer in {SOURCE_ROOT}/; Next.js would read it as the Pages Router"
                    ),
                ));
            }
            return;
        }

        let extension = FileName::new(name_of(path)).extension;
        if top.starts_with('.')
            || top == "public"
            || (next && top == NEXT_APP)
            || !SOURCE_EXTENSIONS.contains(&extension)
        {
            return;
        }

        self.found.push(Found::at(
            "source-root",
            top,
            format!("{top}/ holds code outside {SOURCE_ROOT}/"),
            format!(
                "move the code into the layers of {SOURCE_ROOT}/; configuration files stay at the project root"
            ),
        ));
    }

    /// A file directly in `src/`.
    fn root_file(&mut self, path: &str, name: &str) {
        let pack = self.pack;
        if name.starts_with('.') || pack.root_files().contains(&name) {
            return;
        }
        let allowed = if pack.root_files().is_empty() {
            "no file".to_owned()
        } else {
            format!("only {}", pack.root_files().join(", "))
        };

        self.found.push(Found::at(
            "root-file",
            path,
            format!(
                "{path} sits directly in {SOURCE_ROOT}/, where the {} pack allows {allowed}",
                pack.name(),
            ),
            "move it into a layer",
        ));
    }

    /// A file of a library, whose source root is one shared layer.
    fn library(&mut self, parts: &[&str]) {
        let segment = parts[0];
        let folder = format!("{SOURCE_ROOT}/{segment}");
        if !self.pack.shared_segments().contains(&segment) {
            self.found.push(Found::at(
                "shared",
                &folder,
                format!(
                    "{folder}/ is not a segment of a library: {}",
                    self.pack.shared_segments().join(", ")
                ),
                "move its files into the segment of their kind",
            ));
            return;
        }
        self.reserved(&folder, &parts[1..]);
    }

    /// A file of an app, below a folder of `src/`.
    fn app(&mut self, path: &str, parts: &[&str]) {
        let Some(layer) = Layer::from_name(parts[0]) else {
            let folder = format!("{SOURCE_ROOT}/{}", parts[0]);
            self.found.push(Found::at(
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

        match layer {
            Layer::App => self.app_layer(path, parts),
            Layer::Shared => self.shared_layer(path, parts),
            _ => self.sliced_layer(path, layer, parts),
        }
    }

    /// A file of the app layer: segments only, none of them `ui`.
    fn app_layer(&mut self, path: &str, parts: &[&str]) {
        let pack = self.pack;
        if parts.len() == 2 {
            if !(pack == Pack::ReactVite && path == VITE_ROUTE_TREE) {
                self.found.push(Found::at(
                    "app",
                    path,
                    format!(
                        "{path} sits directly in {SOURCE_ROOT}/app/, which holds segments only"
                    ),
                    format!(
                        "move it into a segment of the app layer: {}",
                        pack.app_segments().join(", ")
                    ),
                ));
            }
            return;
        }

        let segment = parts[1];
        let folder = format!("{SOURCE_ROOT}/app/{segment}");
        if segment == "ui" {
            self.found.push(Found::at(
                "app",
                &folder,
                format!("{folder}/ is a ui segment in the app layer, which has none"),
                "move the components to the page, widget or shared ui that uses them, and the providers to app/providers",
            ));
        } else if !pack.app_segments().contains(&segment) {
            self.found.push(Found::at(
                "app",
                &folder,
                format!(
                    "{folder}/ is not a segment of the app layer: {}",
                    pack.app_segments().join(", ")
                ),
                "move its files into the segment of their kind",
            ));
        } else if segment != "routes" {
            self.reserved(&folder, &parts[2..]);
        }
    }

    /// A file of the shared layer: its segments, each but `ui` and `lib` behind an `index.ts`.
    fn shared_layer(&mut self, path: &str, parts: &[&str]) {
        let pack = self.pack;
        if parts.len() == 2 {
            self.found.push(Found::at(
                "shared",
                path,
                format!("{path} sits directly in {SOURCE_ROOT}/shared/, which holds segments only"),
                format!(
                    "move it into one of the segments of {SOURCE_ROOT}/shared/: {}",
                    pack.shared_segments().join(", ")
                ),
            ));
            return;
        }

        let segment = parts[1];
        let folder = format!("{SOURCE_ROOT}/shared/{segment}");
        if !pack.shared_segments().contains(&segment) {
            self.found.push(Found::at(
                "shared",
                &folder,
                format!(
                    "{folder}/ is not a segment of the shared layer: {}; shared holds no slices",
                    pack.shared_segments().join(", ")
                ),
                "move business code into an entity, feature or widget, and the rest into the segment of its kind",
            ));
            return;
        }
        if !OPEN_SEGMENTS.contains(&segment)
            && !self.tracked.contains(format!("{folder}/index.ts").as_str())
        {
            self.found.push(
                Found::at(
                    "shared",
                    &folder,
                    format!("{folder}/ has no index.ts, its public API"),
                    format!("add index.ts and import the segment as @/shared/{segment}"),
                )
                .named("index.ts"),
            );
        }
        self.reserved(&folder, &parts[2..]);
    }

    /// A file of a sliced layer: in a slice, or in a slice of a group.
    fn sliced_layer(&mut self, path: &str, layer: Layer, parts: &[&str]) {
        let layer_folder = format!("{SOURCE_ROOT}/{}", layer.name());
        if parts.len() == 2 {
            self.found.push(Found::at(
                "slice",
                path,
                format!("{path} sits directly in {layer_folder}/, which holds slices only"),
                format!("move it into one of the slices of {layer_folder}/"),
            ));
            return;
        }

        let unit = parts[1];
        let unit_folder = format!("{layer_folder}/{unit}");
        if SLICE_SEGMENTS.contains(&unit) {
            self.found.push(Found::at(
                "segment",
                &unit_folder,
                format!("{unit_folder}/ is a segment directly on a sliced layer"),
                format!("move its files into the {unit} segment of the slice they belong to"),
            ));
            return;
        }

        if !(layer.grouped() && self.groups.contains(&unit_folder)) {
            self.slice(layer, &unit_folder, &parts[2..]);
            return;
        }

        if parts.len() == 3 {
            self.found.push(Found::at(
                "group",
                path,
                format!("{path} sits in the group {unit_folder}/, which holds slices only"),
                "move it into one of the group's slices; a group has no index.ts",
            ));
            return;
        }
        let slice_folder = format!("{unit_folder}/{}", parts[2]);
        let nested = parts.len() > 4
            && !SLICE_SEGMENTS.contains(&parts[3])
            && parts[3] != CROSS_IMPORT
            && !self.pack.public_files().iter().any(|file| {
                self.tracked
                    .contains(format!("{slice_folder}/{file}").as_str())
            });
        if nested {
            self.found.push(Found::at(
                "group",
                &slice_folder,
                format!("{slice_folder}/ is a group inside the group {unit_folder}/"),
                "keep one level of groups: move its slices up into the group, or make it a slice",
            ));
            return;
        }
        self.slice(layer, &slice_folder, &parts[3..]);
    }

    /// A file of the slice `folder` of `layer`; `rest` is its path below the slice.
    fn slice(&mut self, layer: Layer, folder: &str, rest: &[&str]) {
        let pack = self.pack;
        let name = name_of(folder);

        if !kebab(name) {
            self.found.push(Found::at(
                "slice",
                folder,
                format!("slice {folder}/ is not kebab-case"),
                "rename the slice folder in kebab-case",
            ));
        }
        if !self.tracked.contains(format!("{folder}/index.ts").as_str()) {
            self.found.push(
                Found::at(
                    "slice",
                    folder,
                    format!("slice {folder}/ has no index.ts, its public API"),
                    "add index.ts and re-export from it what other slices may use",
                )
                .named("index.ts"),
            );
        }

        if rest.len() == 1 {
            let file = rest[0];
            let path = format!("{folder}/{file}");
            if file == "index.tsx" {
                self.found.push(Found::at(
                    "slice",
                    &path,
                    format!("{path} is a .tsx public API; a public API only re-exports"),
                    "rename it index.ts and move any component it declares into the ui segment",
                ));
            } else if !pack.public_files().contains(&file) {
                self.found.push(Found::at(
                    "segment",
                    &path,
                    format!(
                        "{path} sits in the slice root, which holds {} and segments only",
                        pack.public_files().join(", ")
                    ),
                    format!("move it into a segment: {}", SLICE_SEGMENTS.join(", ")),
                ));
            }
            return;
        }

        let segment = rest[0];
        let segment_folder = format!("{folder}/{segment}");
        if segment == CROSS_IMPORT {
            self.cross_import(layer, &segment_folder, &rest[1..]);
        } else if SLICE_SEGMENTS.contains(&segment) {
            self.reserved(&segment_folder, &rest[1..]);
        } else {
            self.found.push(Found::at(
                "segment",
                &segment_folder,
                format!(
                    "{segment_folder}/ is not a segment: a slice holds {}",
                    SLICE_SEGMENTS.join(", ")
                ),
                "move its files into the segment of their kind",
            ));
        }
    }

    /// A file of a slice's `@x` folder: only in an entity, and only `<entity>.ts` files.
    fn cross_import(&mut self, layer: Layer, folder: &str, rest: &[&str]) {
        if layer != Layer::Entities {
            self.found.push(Found::at(
                "cross-import",
                folder,
                format!(
                    "{folder}/ is an @x folder outside {SOURCE_ROOT}/entities/; only entities cross-import"
                ),
                "import the slice through its index.ts, or move the shared part down a layer",
            ));
            return;
        }
        let file = FileName::new(rest[0]);
        if rest.len() > 1 || file.extension != "ts" || !file.dotted.is_empty() || !kebab(file.stem)
        {
            let path = format!("{folder}/{}", rest.join("/"));
            self.found.push(Found::at(
                "cross-import",
                &path,
                format!("{path} is not an @x file: @x holds <entity>.ts files only"),
                "name the file after the entity that imports it, directly in @x",
            ));
        }
    }

    /// The folders of `rest`, below the segment `folder`, that take a segment's name.
    fn reserved(&mut self, folder: &str, rest: &[&str]) {
        let Some((_, folders)) = rest.split_last() else {
            return;
        };
        for (depth, name) in folders.iter().enumerate() {
            if SLICE_SEGMENTS.contains(name) {
                let path = format!("{folder}/{}", folders[..=depth].join("/"));
                self.found.push(Found::at(
                    "reserved",
                    &path,
                    format!("{path}/ inside a segment takes the segment name {name}"),
                    "rename the folder after what it holds; a segment name says the kind of a whole segment",
                ));
            }
        }
    }
}

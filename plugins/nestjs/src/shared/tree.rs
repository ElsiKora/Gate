//! The tracked source tree as the rules walk it: files and folders below a root outside the
//! skipped folders of `[languages.typescript] skip-folders`, the entries of one folder, and the
//! kinds of test file.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use gate_typescript::{Settings, Source};

use crate::layout::{Layout, folder_prefix};

/// The settings of a walk that skips no folder. The rules whose reference walk read every folder
/// walk with these through the same helpers: asset-storage, the advisory-lock-manifest markers, the
/// files below the modules root that automator-metadata reads for its inventories, error-logging,
/// entity-table-name, kind-layout's module areas and its area check, migrations, module-names,
/// owner-boundaries, owner-roots, prefix-nesting, repeated-segment, typeorm-relation and wrappers.
pub static UNSKIPPED: LazyLock<Settings> = LazyLock::new(|| Settings {
    skip_folders: Vec::new(),
    ..Settings::default()
});

/// The paths of `paths` without repeats, in their first order: walks over roots that contain one
/// another (a source root of `""`) read each file once.
pub fn distinct<'s>(paths: impl Iterator<Item = &'s str>) -> Vec<&'s str> {
    let mut seen = BTreeSet::new();

    paths.filter(|path| seen.insert(*path)).collect()
}

/// The tracked files below `root` (every file when `root` is empty), in path order.
fn under<'s>(source: &'s dyn Source, root: &str) -> impl Iterator<Item = &'s str> + use<'s> {
    let prefix = folder_prefix(root);

    source.paths().filter(move |path| path.starts_with(&prefix))
}

/// The part of `path` below `root`.
fn relative<'p>(path: &'p str, root: &str) -> &'p str {
    if root.is_empty() {
        path
    } else {
        &path[root.len() + 1..]
    }
}

/// The tracked files below `root` that `settings` does not skip ([`Settings::skips`], the
/// `.gitignore` way). Every walk of the plugin goes through this; one that skips no folder passes
/// [`UNSKIPPED`].
pub fn files<'s: 'b, 'b>(
    source: &'s dyn Source,
    root: &str,
    settings: &'b Settings,
) -> impl Iterator<Item = &'s str> + use<'s, 'b> {
    under(source, root).filter(move |path| !settings.skips(path))
}

/// The tracked `.ts` files, declarations included, of [`files`].
pub fn typescript_files<'s: 'b, 'b>(
    source: &'s dyn Source,
    root: &str,
    settings: &'b Settings,
) -> impl Iterator<Item = &'s str> + use<'s, 'b> {
    files(source, root, settings)
        .filter(|path| is_typescript(path.rsplit_once('/').map_or(*path, |(_, name)| name)))
}

/// Whether the file `name` has the extension `.ts` as Node's `extname` reads it: a name that is
/// only `.ts` has none.
#[must_use]
pub fn is_typescript(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|extension| extension == "ts")
}

/// Every folder below `root`, without `root` itself, that holds a file of [`files`].
#[must_use]
pub fn folders(source: &dyn Source, root: &str, settings: &Settings) -> BTreeSet<String> {
    let mut folders = BTreeSet::new();

    for path in files(source, root, settings) {
        let Some((within, _)) = relative(path, root).rsplit_once('/') else {
            continue;
        };
        let mut folder = root.to_owned();

        for segment in within.split('/') {
            if !folder.is_empty() {
                folder.push('/');
            }
            folder.push_str(segment);
            folders.insert(folder.clone());
        }
    }

    folders
}

/// The entries of `folder` that hold a file of [`files`]: each file and folder name, with `true`
/// for a folder.
#[must_use]
pub fn entries(source: &dyn Source, folder: &str, settings: &Settings) -> BTreeMap<String, bool> {
    let mut entries = BTreeMap::new();

    for path in files(source, folder, settings) {
        match relative(path, folder).split_once('/') {
            Some((name, _)) => entries.insert(name.to_owned(), true),
            None => entries.insert(relative(path, folder).to_owned(), false),
        };
    }

    entries
}

/// Whether a test file is test support: below one of the layout's test support folders, or named
/// `*.<kind>.ts` for one of its test support kinds, and neither a test scenario (a name ending in
/// one of the layout's scenario suffixes) nor a declaration file.
#[must_use]
pub fn is_test_support(layout: &Layout, path: &str) -> bool {
    if is_test_scenario(layout, path) || path.ends_with(".d.ts") {
        return false;
    }

    let (folders, name) = path.rsplit_once('/').unwrap_or(("", path));

    folders.split('/').any(|folder| {
        layout
            .test_support_folders
            .iter()
            .any(|support| support == folder)
    }) || name
        .strip_suffix(".ts")
        .and_then(|stem| stem.rsplit_once('.'))
        .is_some_and(|(_, kind)| layout.test_support_kinds.iter().any(|known| known == kind))
}

/// Whether a file is a test scenario: its name ends with one of the layout's scenario suffixes.
#[must_use]
pub fn is_test_scenario(layout: &Layout, path: &str) -> bool {
    layout
        .test_scenario_suffixes
        .iter()
        .any(|suffix| path.ends_with(suffix.as_str()))
}

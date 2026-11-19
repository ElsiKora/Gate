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

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gate_typescript::{Source, sorted_with_prefix};

    use super::{UNSKIPPED, entries, files, folders};

    /// A sorted source that counts the paths it hands out, however they are asked for.
    struct Counting {
        paths: Vec<String>,
        handed: Cell<usize>,
    }

    impl Counting {
        fn new(modules: usize) -> Self {
            let mut paths: Vec<String> = (0..modules)
                .flat_map(|module| {
                    ["index.ts", "a.ts", "b/index.ts", "b/c.ts"]
                        .map(|file| format!("src/m{module:03}/{file}"))
                })
                .collect();
            paths.sort_unstable();
            Self {
                paths,
                handed: Cell::new(0),
            }
        }

        fn count<'s>(
            &'s self,
            paths: impl Iterator<Item = &'s str> + 's,
        ) -> Box<dyn Iterator<Item = &'s str> + 's> {
            Box::new(paths.inspect(|_| self.handed.set(self.handed.get() + 1)))
        }
    }

    impl Source for Counting {
        fn paths(&self) -> Box<dyn Iterator<Item = &str> + '_> {
            self.count(self.paths.iter().map(String::as_str))
        }

        fn paths_with_prefix(&self, prefix: &str) -> Box<dyn Iterator<Item = &str> + '_> {
            self.count(sorted_with_prefix(&self.paths, prefix))
        }

        fn bytes(&self, _path: &str) -> gate_typescript::Result<Option<Rc<[u8]>>> {
            Ok(None)
        }
    }

    #[test]
    fn a_walk_below_one_folder_reads_only_the_paths_below_it() {
        let source = Counting::new(200);

        let found = entries(&source, "src/m007", &UNSKIPPED);
        assert_eq!(
            found.into_iter().collect::<Vec<_>>(),
            [
                ("a.ts".to_owned(), false),
                ("b".to_owned(), true),
                ("index.ts".to_owned(), false)
            ]
        );
        // Four files below the folder, and at most the one path that ends the range: not the
        // 800 tracked paths.
        assert!(
            source.handed.get() <= 5,
            "{} paths read",
            source.handed.get()
        );

        source.handed.set(0);
        let below: Vec<&str> = files(&source, "src/m007/b", &UNSKIPPED).collect();
        assert_eq!(below, ["src/m007/b/c.ts", "src/m007/b/index.ts"]);
        assert!(
            source.handed.get() <= 3,
            "{} paths read",
            source.handed.get()
        );

        // Every folder of the tree, each walked once, reads each path a bounded number of times.
        source.handed.set(0);
        for folder in folders(&source, "src", &UNSKIPPED) {
            let _ = entries(&source, &folder, &UNSKIPPED);
        }
        assert!(
            source.handed.get() <= 4 * 800,
            "{} paths read for 400 folders of 800 paths",
            source.handed.get()
        );
    }
}

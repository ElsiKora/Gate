//! The snapshot a rule reads.

use crate::host::TreeHost;

/// The tracked files of a call's scope: its project's subtree (paths relative to the project root)
/// or the whole repository, plus any extra paths the configuration grants. Anything else does not
/// exist for the rule.
#[derive(Clone, Copy)]
pub struct Tree<'h> {
    host: &'h dyn TreeHost,
}

impl std::fmt::Debug for Tree<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Tree").finish_non_exhaustive()
    }
}

impl<'h> Tree<'h> {
    /// The tree over a host.
    #[must_use]
    pub fn new(host: &'h dyn TreeHost) -> Self {
        Self { host }
    }

    /// Every tracked path, sorted.
    #[must_use]
    pub fn paths(&self) -> Vec<String> {
        self.host.list("")
    }

    /// The tracked paths below `directory` (without a trailing slash; empty for all), sorted.
    #[must_use]
    pub fn under(&self, directory: &str) -> Vec<String> {
        let directory = directory.trim_end_matches('/');
        if directory.is_empty() {
            return self.paths();
        }
        self.host.list(&format!("{directory}/"))
    }

    /// The tracked paths with one of `extensions` (without the dot, compared case-sensitively).
    #[must_use]
    pub fn with_extensions(&self, extensions: &[&str]) -> Vec<String> {
        self.paths()
            .into_iter()
            .filter(|path| {
                std::path::Path::new(path)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extensions.contains(&extension))
            })
            .collect()
    }

    /// Whether `path` is tracked and granted.
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.host.list(path).iter().any(|listed| listed == path)
    }

    /// The bytes of a tracked file; `None` when it is not tracked or not granted.
    #[must_use]
    pub fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.host.read(path)
    }

    /// The size in bytes of the blob git stores for a tracked file, without reading its contents;
    /// `None` when it is not tracked or not granted. In the working tree, a file that matches its
    /// index entry has the size of the index's blob (a file Git LFS moved has its pointer's size),
    /// and a file that differs from it its own size.
    #[must_use]
    pub fn size(&self, path: &str) -> Option<u64> {
        self.host.size(path)
    }

    /// The text of a tracked file, decoded as UTF-8 with replacement characters, as Node reads a
    /// file; `None` when it is not tracked or not granted.
    #[must_use]
    pub fn text(&self, path: &str) -> Option<String> {
        self.host
            .read(path)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }

    /// The paths the run's change selection names (`--staged`, `--changed`), when it has one; a
    /// rule may use it to narrow its work, never to widen what it reports.
    #[must_use]
    pub fn changed(&self) -> Option<Vec<String>> {
        self.host.changed()
    }
}

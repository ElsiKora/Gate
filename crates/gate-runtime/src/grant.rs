//! What one call may read, and how its paths map to repository paths.

/// The paths a call may read: its scope's root (a project's root, or the repository) and the extra
/// read paths its configuration grants.
///
/// A plugin addresses files relative to the root. A granted path outside the root is addressed
/// relatively too: from root `apps/api`, the repository path `docs/ledger/a.md` is
/// `../../docs/ledger/a.md`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Grant {
    root: String,
    reads: Vec<String>,
}

impl Grant {
    /// The whole repository.
    #[must_use]
    pub fn repository() -> Self {
        Self::default()
    }

    /// The subtree at `root` (repository-relative; empty for the repository) and the extra
    /// repository-relative paths or directories `reads`.
    #[must_use]
    pub fn new(root: &str, reads: &[String]) -> Self {
        Self {
            root: root.trim_matches('/').to_owned(),
            reads: reads
                .iter()
                .map(|path| path.trim_matches('/').to_owned())
                .collect(),
        }
    }

    /// The scope's root, repository-relative.
    #[must_use]
    pub fn root(&self) -> &str {
        &self.root
    }

    /// Whether the repository path `path` may be read.
    #[must_use]
    pub fn allows(&self, path: &str) -> bool {
        within(path, &self.root) || self.reads.iter().any(|read| within(path, read))
    }

    /// The repository path a plugin's path names, when the grant allows it.
    #[must_use]
    pub fn to_repository(&self, path: &str) -> Option<String> {
        self.resolve(path).filter(|path| self.allows(path))
    }

    /// The repository path a plugin's path names, granted or not; `None` when it is absolute, has a
    /// backslash, or leaves the repository. Finding locations are reported this way.
    #[must_use]
    pub fn resolve(&self, path: &str) -> Option<String> {
        if path.starts_with('/') || path.contains('\\') {
            return None;
        }
        let joined = if self.root.is_empty() {
            path.to_owned()
        } else {
            format!("{}/{path}", self.root)
        };
        normalize(&joined)
    }

    /// The repository prefix a plugin's listing prefix names: a prefix ending in `/` (or empty, for
    /// the root) stays a directory prefix. `None` when it leaves the repository.
    #[must_use]
    pub fn to_repository_prefix(&self, prefix: &str) -> Option<String> {
        let directory = prefix.is_empty() || prefix.ends_with('/');
        let path = self.resolve(prefix)?;
        Some(if directory && !path.is_empty() {
            format!("{path}/")
        } else {
            path
        })
    }

    /// The repository prefixes a listing of the plugin prefix `prefix` covers, one per granted path
    /// it reaches: a granted path the prefix reaches whole (the root as `root/`), or the
    /// repository prefix `prefix` names inside a granted path. Recorded in the read set, so a file
    /// added under any of them re-runs the call.
    #[must_use]
    pub fn listed(&self, prefix: &str) -> Vec<String> {
        let named = self.to_repository_prefix(prefix);
        let reads = if self.root.is_empty() {
            &[][..]
        } else {
            &self.reads[..]
        };
        let mut covered: Vec<String> = std::iter::once(&self.root)
            .chain(reads)
            .filter_map(|granted| {
                if self.to_plugin(granted).starts_with(prefix) {
                    return Some(if granted == &self.root && !granted.is_empty() {
                        format!("{granted}/")
                    } else {
                        granted.clone()
                    });
                }
                named
                    .clone()
                    .filter(|named| within(named.trim_end_matches('/'), granted))
            })
            .collect();
        covered.sort();
        covered.dedup();
        covered
    }

    /// The repository path of a file a generator writes at `path`, relative to the root: `None`
    /// when the path is absolute, has a backslash, names the root itself or leaves it. The caller
    /// refuses a path the core owns ([`is_core_path`]).
    #[must_use]
    pub fn to_output(&self, path: &str) -> Option<String> {
        if path.starts_with('/') || path.contains('\\') {
            return None;
        }
        let resolved = self.resolve(path)?;
        (resolved != self.root && within(&resolved, &self.root)).then_some(resolved)
    }

    /// The repository glob of a glob a generator owns, relative to the root (see
    /// [`is_relative_glob`]).
    #[must_use]
    pub fn to_repository_glob(&self, glob: &str) -> String {
        if self.root.is_empty() {
            glob.to_owned()
        } else {
            format!("{}/{glob}", self.root)
        }
    }

    /// How a plugin addresses the repository path `path`.
    #[must_use]
    pub fn to_plugin(&self, path: &str) -> String {
        if self.root.is_empty() {
            return path.to_owned();
        }
        if let Some(rest) = path
            .strip_prefix(&self.root)
            .and_then(|rest| rest.strip_prefix('/'))
        {
            return rest.to_owned();
        }
        let common = self
            .root
            .split('/')
            .zip(path.split('/'))
            .take_while(|(left, right)| left == right)
            .count();
        let up = self.root.split('/').count() - common;
        let mut parts = vec![".."; up];
        parts.extend(path.split('/').skip(common));
        parts.join("/")
    }

    /// The listing of the plugin prefix `prefix` over `paths`, the snapshot's tracked paths in
    /// byte order (`str` order), each once: the granted paths whose plugin form starts with
    /// `prefix`, in plugin form and sorted.
    ///
    /// A path inside the root is `<root>/<plugin form>`, so the listing is the range of `paths`
    /// that starts with `<root>/<prefix>`, found by a binary search; each path in it still passes
    /// the grant and prefix checks. Only a prefix that can match the `..` form of a read path
    /// outside the root lists by a scan of every path.
    #[must_use]
    pub fn list(&self, paths: &[String], prefix: &str) -> Vec<String> {
        self.counted(paths, prefix).0
    }

    /// [`Grant::list`], with the number of paths it compared: in the binary search and in the
    /// range, or every path for a scan.
    fn counted(&self, paths: &[String], prefix: &str) -> (Vec<String>, usize) {
        let Some(start) = self.range_start(prefix) else {
            return (self.scan(paths, prefix), paths.len());
        };
        let mut compared = 0;
        let mut below = |path: &String| {
            compared += 1;
            path.as_str() < start.as_str()
        };
        let first = paths.partition_point(&mut below);
        let listed = paths[first..]
            .iter()
            .inspect(|_| compared += 1)
            .take_while(|path| path.starts_with(start.as_str()))
            .filter(|path| self.allows(path))
            .map(|path| self.to_plugin(path))
            .filter(|path| path.starts_with(prefix))
            .collect();
        (listed, compared)
    }

    /// [`Grant::list`] by a scan of every path, the only way to find the `..` forms of read paths
    /// outside the root.
    fn scan(&self, paths: &[String], prefix: &str) -> Vec<String> {
        let mut listed: Vec<String> = paths
            .iter()
            .filter(|path| self.allows(path))
            .map(|path| self.to_plugin(path))
            .filter(|path| path.starts_with(prefix))
            .collect();
        listed.sort();
        listed
    }

    /// The repository string every path a listing of `prefix` answers starts with: `None` when
    /// `prefix` can match the `..` form of a read path outside the root (`""`, `.`, `..`, `../`
    /// and what follows it), which only a scan finds.
    fn range_start(&self, prefix: &str) -> Option<String> {
        if self.root.is_empty() {
            return Some(prefix.to_owned());
        }
        let outside = self.reads.iter().any(|read| !within(read, &self.root));
        if outside && ("../".starts_with(prefix) || prefix.starts_with("../")) {
            return None;
        }
        Some(format!("{}/{prefix}", self.root))
    }
}

/// Whether `path` is `directory` or below it; everything is below the empty directory.
fn within(path: &str, directory: &str) -> bool {
    directory.is_empty()
        || path == directory
        || path
            .strip_prefix(directory)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// The files at the repository root the core owns; a generator never writes or owns them.
pub const CORE_FILES: [&str; 4] = ["gate.toml", "gate.lock", "gate.exceptions.toml", "bin/gate"];
/// The configuration files the core reads in any directory.
const CONFIG_NAMES: [&str; 3] = ["gate.toml", "gate.lock", "gate.exceptions.toml"];
/// The directory of the hooks the core writes.
pub const HOOKS: &str = ".githooks";

/// Whether the repository path `path` is one the core owns: anything under a `.git` segment, a
/// `gate.toml`, `gate.lock` or `gate.exceptions.toml` in any directory, `bin/gate`, or anything
/// under `.githooks`. Compared without case, as a case-insensitive file system would.
#[must_use]
pub fn is_core_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let mut segments = lower.split('/');
    has_git_segment(&lower)
        || segments
            .next_back()
            .is_some_and(|name| CONFIG_NAMES.contains(&name))
        || lower == "bin/gate"
        || within(&lower, HOOKS)
}

/// Whether a path or glob has a `.git` segment, in any case.
#[must_use]
pub fn has_git_segment(path: &str) -> bool {
    path.split('/')
        .any(|segment| segment.eq_ignore_ascii_case(".git"))
}

/// Whether `glob` matches every path below its root: each of its segments is only `*`
/// (`*`, `**`, `**/*`).
#[must_use]
pub fn owns_everything(glob: &str) -> bool {
    glob.split('/')
        .all(|segment| !segment.is_empty() && segment.bytes().all(|byte| byte == b'*'))
}

/// Whether `glob` stays below the root it is relative to: not empty, not absolute, no backslash,
/// and no `.` or `..` segment.
#[must_use]
pub fn is_relative_glob(glob: &str) -> bool {
    !glob.is_empty()
        && !glob.starts_with('/')
        && !glob.contains('\\')
        && glob
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// `path` with `.` and empty segments dropped and `..` applied; `None` when it leaves the repository.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::{Grant, has_git_segment, is_core_path, is_relative_glob, owns_everything};

    /// Synthetic tracked paths, sorted, with siblings of `apps/api` that sort just before
    /// (`apps/api-old`, `apps/api.d`) and just after (`apps/api2`) its subtree.
    fn tracked() -> Vec<String> {
        let mut paths: Vec<String> = [
            ".github/workflows/ci.yml",
            "README.md",
            "apps/api-old/src/main.ts",
            "apps/api.d/x",
            "apps/api/..foo",
            "apps/api/.github/a.yml",
            "apps/api/a/b",
            "apps/api/package.json",
            "apps/api/package.json5",
            "apps/api/src/main.ts",
            "apps/api/src/server.ts",
            "apps/api/srcs/x.ts",
            "apps/api2/src/main.ts",
            "apps/web/src/main.ts",
            "docs/ledger/a.md",
            "docs/ledgers/b.md",
            "docs/other.md",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        paths.sort();
        paths
    }

    #[test]
    fn a_listing_answers_what_a_scan_of_every_path_answers() {
        let paths = tracked();
        let grants = [
            ("the repository", Grant::repository()),
            ("a nested root", Grant::new("apps/api", &[])),
            ("a parent root", Grant::new("apps", &[])),
            (
                "a root with a read path inside it",
                Grant::new("apps/api", &["apps/api/src".to_owned()]),
            ),
            (
                "a root with read paths outside it",
                Grant::new(
                    "apps/api",
                    &["docs/ledger".to_owned(), ".github".to_owned()],
                ),
            ),
        ];
        let prefixes = [
            "",
            "src",
            "src/",
            "sr",
            ".github/",
            ".",
            "..",
            "../",
            "../../docs/le",
            "../../.github/",
            "./x",
            "a/../b",
            "..f",
            "package.json",
            "api",
            "api2",
            "apps/api",
            "apps/api/",
            "../api2",
        ];
        for (name, grant) in &grants {
            for prefix in prefixes {
                assert_eq!(
                    grant.list(&paths, prefix),
                    grant.scan(&paths, prefix),
                    "{name}, prefix {prefix:?}"
                );
            }
        }

        // The cases the table must cover answer something, so equal answers are not empty ones.
        let nested = Grant::new("apps/api", &[]);
        assert_eq!(
            nested.list(&paths, "src"),
            ["src/main.ts", "src/server.ts", "srcs/x.ts"]
        );
        assert_eq!(nested.list(&paths, "").len(), 8);
        assert_eq!(Grant::repository().list(&paths, ""), paths);
        let outside = &grants[4].1;
        assert_eq!(
            outside.list(&paths, "../../docs/le"),
            ["../../docs/ledger/a.md"]
        );
        assert_eq!(outside.list(&paths, "").len(), 10);
    }

    #[test]
    fn a_probe_examines_the_paths_that_start_with_it_not_the_tree() {
        let mut paths: Vec<String> = (0..20_000)
            .map(|index| format!("apps/app{}/src/file{index}.ts", index % 40))
            .collect();
        paths.sort();
        let grant = Grant::new("apps/app7", &["docs/ledger".to_owned()]);
        let probes: Vec<String> = (0..1_000)
            .map(|index| format!("src/file{}.ts", 7 + 40 * (index % 500)))
            .collect();

        let (mut listed, mut scanned) = (0, 0);
        for probe in &probes {
            let (answer, steps) = grant.counted(&paths, probe);
            assert!(answer.contains(probe), "{probe}");
            listed += steps;
            scanned += paths.len();
        }
        println!("1000 probes of 20000 paths: listing {listed} steps, scan {scanned} paths");
        assert_eq!(scanned, 1_000 * 20_000);
        // A probe takes the binary search (15 steps) and the few paths that start with it.
        assert!(listed <= 1_000 * 32, "{listed}");
    }

    #[test]
    fn project_grant_maps_and_refuses() {
        let grant = Grant::new("apps/api", &["docs/ledger".to_owned()]);
        assert_eq!(
            grant.to_repository("src/main.ts").as_deref(),
            Some("apps/api/src/main.ts")
        );
        assert_eq!(
            grant.to_repository("../../docs/ledger/a.md").as_deref(),
            Some("docs/ledger/a.md")
        );
        assert_eq!(grant.to_repository("../web/src/main.ts"), None);
        assert_eq!(grant.to_repository("../../docs/ledgers/a.md"), None);
        assert_eq!(grant.to_repository("../../../etc/passwd"), None);
        assert_eq!(grant.to_plugin("apps/api/src/main.ts"), "src/main.ts");
        assert_eq!(
            grant.to_plugin("docs/ledger/a.md"),
            "../../docs/ledger/a.md"
        );
        assert_eq!(grant.to_plugin("apps/web/a.ts"), "../web/a.ts");
        assert_eq!(
            grant.resolve("../web/a.ts").as_deref(),
            Some("apps/web/a.ts")
        );
        assert_eq!(grant.resolve("../../../a.ts"), None);
        assert_eq!(grant.to_repository_prefix("").as_deref(), Some("apps/api/"));
        assert_eq!(
            grant.to_repository_prefix("../../docs/").as_deref(),
            Some("docs/")
        );
        assert_eq!(
            grant.to_repository_prefix("src/ma").as_deref(),
            Some("apps/api/src/ma")
        );
        assert_eq!(grant.to_repository_prefix("../../../"), None);
    }

    #[test]
    fn repository_grant_allows_everything_inside() {
        let grant = Grant::repository();
        assert_eq!(grant.to_repository("a/b.ts").as_deref(), Some("a/b.ts"));
        assert_eq!(grant.to_repository("../a"), None);
        assert_eq!(grant.to_plugin("a/b.ts"), "a/b.ts");
    }

    #[test]
    fn generator_output_stays_below_the_root() {
        let grant = Grant::new("apps/api", &["docs/ledger".to_owned()]);
        assert_eq!(
            grant.to_output("gen/a.md").as_deref(),
            Some("apps/api/gen/a.md")
        );
        assert_eq!(grant.to_output("./a.md").as_deref(), Some("apps/api/a.md"));
        for outside in [
            "../web/a.md",
            "../../docs/ledger/a.md",
            "/a.md",
            "a\\b",
            ".",
            "",
        ] {
            assert_eq!(grant.to_output(outside), None, "{outside:?}");
        }
        assert_eq!(
            Grant::repository().to_output("a.md").as_deref(),
            Some("a.md")
        );
    }

    #[test]
    fn owned_globs_are_relative_and_mapped_under_the_root() {
        for glob in ["gen/**", "AGENTS.md", ".claude/rules/*.md"] {
            assert!(is_relative_glob(glob), "{glob:?}");
        }
        for glob in [
            "",
            "/gen/**",
            "../gen/**",
            "gen/../x",
            "./gen",
            "gen//x",
            "gen\\x",
        ] {
            assert!(!is_relative_glob(glob), "{glob:?}");
        }
        let grant = Grant::new("apps/api", &[]);
        assert_eq!(grant.to_repository_glob("gen/**"), "apps/api/gen/**");
        assert_eq!(Grant::repository().to_repository_glob("gen/**"), "gen/**");
    }

    #[test]
    fn absolute_and_backslashed_paths_name_nothing() {
        let grant = Grant::new("apps/api", &[]);
        for path in [
            "/apps/api/src/main.ts",
            "/src/main.ts",
            "src\\main.ts",
            "..\\web\\a.ts",
        ] {
            assert_eq!(grant.resolve(path), None, "{path:?}");
            assert_eq!(grant.to_repository(path), None, "{path:?}");
        }
        assert_eq!(Grant::repository().to_repository("/gate.toml"), None);
    }

    #[test]
    fn a_listing_records_every_granted_path_it_reaches() {
        let grant = Grant::new(
            "apps/api",
            &["docs/ledger".to_owned(), "LICENSE".to_owned()],
        );
        assert_eq!(
            grant.listed(""),
            ["LICENSE", "apps/api/", "docs/ledger"],
            "the empty prefix lists the root and every extra read"
        );
        assert_eq!(grant.listed("src/"), ["apps/api/src/"]);
        assert_eq!(grant.listed("../../docs/"), ["docs/ledger"]);
        assert_eq!(grant.listed("../../docs/ledger/old/"), ["docs/ledger/old/"]);
        assert_eq!(grant.listed("../../"), ["LICENSE", "docs/ledger"]);
        assert_eq!(grant.listed("../web/"), Vec::<String>::new());
        assert_eq!(grant.listed("../../docs/ledgers/"), Vec::<String>::new());
        assert_eq!(Grant::repository().listed(""), [""]);
        assert_eq!(Grant::repository().listed("docs/"), ["docs/"]);
    }

    #[test]
    fn the_core_owns_git_its_configuration_its_launcher_and_its_hooks() {
        for path in [
            ".git/config",
            "apps/api/.GIT/hooks/x",
            "gate.toml",
            "apps/api/Gate.Toml",
            "gate.lock",
            "gate.exceptions.toml",
            "bin/gate",
            "BIN/Gate",
            ".githooks/pre-commit",
        ] {
            assert!(is_core_path(path), "{path:?}");
        }
        for path in [
            ".gitignore",
            "apps/api/bin/gate.md",
            "docs/gate.toml.md",
            ".github/x",
        ] {
            assert!(!is_core_path(path), "{path:?}");
        }
        assert!(has_git_segment("x/.Git/**"));
        assert!(!has_git_segment(".gitignore"));
        for glob in ["*", "**", "**/*", "*/**"] {
            assert!(owns_everything(glob), "{glob:?}");
        }
        for glob in ["gen/**", "*.md", "**/*.md"] {
            assert!(!owns_everything(glob), "{glob:?}");
        }
    }
}

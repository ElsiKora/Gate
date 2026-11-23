//! The tracked files a run looks at, read from one explicit source.
//!
//! Only git-tracked paths exist in a snapshot: untracked and ignored files, nested repositories and
//! submodule contents are never seen. Every path has a content id, its git blob id, so a later cache
//! can key on contents without reading them.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use crate::error::{Error, Result};
use crate::git::{CatFile, Git, ObjectSizes, split_nul};

/// Where a snapshot's contents come from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Source {
    /// The working-tree contents of the tracked files (the default).
    WorkTree,
    /// The index (`--staged`).
    Index,
    /// The tree of one commit (`--rev`); holds the revision as given.
    Commit(String),
}

impl fmt::Display for Source {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkTree => formatter.write_str("tree"),
            Self::Index => formatter.write_str("staged"),
            Self::Commit(revision) => write!(formatter, "rev {revision}"),
        }
    }
}

/// A git blob id: the hex object id git gives the file's contents.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BlobId(String);

impl BlobId {
    /// The hex id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BlobId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A commit's listing, by the revision asked: the commit it names and its paths' blob ids.
type Listing = (String, Arc<BTreeMap<String, BlobId>>);

/// What the snapshots of one run's commits share: the listing of each commit, the content of each
/// blob read, by its id, and one `git cat-file` reader. A blob's content never changes, so a file
/// that several commits hold is read once.
#[derive(Debug, Default)]
pub(crate) struct Shared {
    /// By revision as asked: the commit it names and its listing.
    listings: Mutex<HashMap<String, Listing>>,
    contents: Mutex<HashMap<BlobId, Arc<[u8]>>>,
    reader: Mutex<Option<CatFile>>,
    /// The size of each blob asked, by its id, and the reader that answers it.
    sizes: Mutex<(HashMap<BlobId, u64>, Option<ObjectSizes>)>,
}

/// An immutable set of tracked paths, their blob ids, and their contents read on demand.
///
/// A snapshot is `Send + Sync`: reads from several threads share one git object reader and one
/// cache of contents.
pub struct Snapshot {
    git: Git,
    source: Source,
    commit: Option<String>,
    entries: Arc<BTreeMap<String, BlobId>>,
    /// In the working tree, the paths whose file matches its index entry (unchanged since git
    /// wrote or staged it), with the index's blob id: their size is the size of that blob, the
    /// bytes git stores, as after a checkout through a filter that stores a pointer.
    clean: Arc<BTreeMap<String, BlobId>>,
    /// In the working tree, the regular files git converts (a filter, line endings): one that is not
    /// in `clean` has the size of the blob git would store for it, not its own.
    filtered: Arc<BTreeSet<String>>,
    /// The reader and the contents of the index or a commit, by blob id.
    shared: Arc<Shared>,
    /// The contents of working-tree files, by path.
    contents: Mutex<HashMap<String, Arc<[u8]>>>,
    /// The working tree's directories that are symbolic links, as far as reads asked.
    directories: Directories,
    /// The paths asked for while recording, tracked or not; `None` once taken.
    asked: Mutex<Option<BTreeSet<String>>>,
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Snapshot")
            .field("source", &self.source)
            .field("commit", &self.commit)
            .field("files", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl Snapshot {
    /// Lists the tracked files of `source` and their blob ids.
    ///
    /// In the working tree, a tracked file that was deleted is absent, and a file whose contents
    /// differ from the index is hashed the way `git add` would hash it.
    ///
    /// # Errors
    /// Fails when git cannot list the files, the revision names no commit, or the index has an
    /// unmerged entry and the source is the index.
    pub fn open(git: &Git, source: Source) -> Result<Self> {
        Self::open_shared(git, source, &Arc::default())
    }

    /// [`Snapshot::open`] sharing `shared` with the run's other snapshots: a commit listed before
    /// is not listed again, and a blob read before is not read again.
    ///
    /// # Errors
    /// As [`Snapshot::open`].
    pub(crate) fn open_shared(git: &Git, source: Source, shared: &Arc<Shared>) -> Result<Self> {
        let mut clean = Arc::default();
        let mut filtered = Arc::default();
        let (entries, commit) = match &source {
            Source::WorkTree => {
                let (entries, unchanged, converted) = work_tree_entries(git)?;
                clean = Arc::new(unchanged);
                filtered = Arc::new(converted);
                (Arc::new(entries), git.head()?)
            }
            Source::Index => (Arc::new(index_entries(git)?), git.head()?),
            Source::Commit(revision) => {
                let listed = lock(&shared.listings).get(revision).cloned();
                let (commit, entries) = if let Some(listed) = listed {
                    listed
                } else {
                    let commit = git.resolve_commit(revision)?.ok_or_else(|| {
                        Error::environment(format!(
                            "--rev {revision} names no commit; pass a commit id, branch or tag"
                        ))
                    })?;
                    let entries = Arc::new(commit_entries(git, &commit)?);
                    lock(&shared.listings)
                        .insert(revision.clone(), (commit.clone(), Arc::clone(&entries)));
                    (commit, entries)
                };
                (entries, Some(commit))
            }
        };

        Ok(Self {
            git: git.clone(),
            source,
            commit,
            entries,
            clean,
            filtered,
            shared: Arc::clone(shared),
            contents: Mutex::new(HashMap::new()),
            directories: Directories::default(),
            asked: Mutex::new(Some(BTreeSet::new())),
        })
    }

    /// The git work tree the snapshot was taken from.
    #[must_use]
    pub fn git(&self) -> &Git {
        &self.git
    }

    /// Where the contents come from.
    #[must_use]
    pub fn source(&self) -> &Source {
        &self.source
    }

    /// The commit the snapshot belongs to: the revision's commit, or `HEAD` (`None` on an unborn
    /// branch).
    #[must_use]
    pub fn commit(&self) -> Option<&str> {
        self.commit.as_deref()
    }

    /// Every tracked path, sorted, relative to the repository root with `/` separators.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// The number of tracked paths.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the snapshot has no path.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether `path` is tracked.
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.entries.contains_key(path)
    }

    /// Tracked paths below `directory` (`""` for every path), sorted.
    pub fn under<'a>(&'a self, directory: &str) -> impl Iterator<Item = &'a str> + 'a {
        let prefix = directory_prefix(directory);
        self.entries
            .range(prefix.clone()..)
            .map(|(path, _)| path.as_str())
            .take_while(move |path| path.starts_with(&prefix))
    }

    /// The blob id of a tracked path.
    #[must_use]
    pub fn blob_id(&self, path: &str) -> Option<&BlobId> {
        self.entries.get(path)
    }

    /// The contents of a tracked path; `None` when the path is not tracked. A symbolic link reads
    /// as its target.
    ///
    /// # Errors
    /// Fails when the contents cannot be read.
    pub fn read(&self, path: &str) -> Result<Option<Arc<[u8]>>> {
        if let Some(asked) = lock(&self.asked).as_mut() {
            asked.insert(path.to_owned());
        }
        let Some(blob) = self.entries.get(path) else {
            return Ok(None);
        };

        if matches!(self.source, Source::WorkTree) {
            if let Some(cached) = lock(&self.contents).get(path) {
                return Ok(Some(Arc::clone(cached)));
            }
            let contents: Arc<[u8]> =
                Arc::from(read_work_tree(&self.git, &self.directories, path)?);
            lock(&self.contents).insert(path.to_owned(), Arc::clone(&contents));
            return Ok(Some(contents));
        }

        if let Some(cached) = lock(&self.shared.contents).get(blob) {
            return Ok(Some(Arc::clone(cached)));
        }
        let bytes = {
            let mut reader = lock(&self.shared.reader);
            if reader.is_none() {
                *reader = Some(self.git.cat_file()?);
            }
            reader
                .as_mut()
                .ok_or_else(|| Error::environment("git cat-file is unavailable"))?
                .read(blob.as_str())?
                .ok_or_else(|| {
                    Error::environment(format!(
                        "git has no object {blob} for {path}; the repository is damaged, run git fsck"
                    ))
                })?
        };
        let contents: Arc<[u8]> = Arc::from(bytes);
        lock(&self.shared.contents).insert(blob.clone(), Arc::clone(&contents));
        Ok(Some(contents))
    }

    /// The size in bytes of a tracked path, without reading its contents when they are not read
    /// yet; `None` when the path is not tracked. It is the size of the blob git stores: in the
    /// working tree, of the index's blob for a file that matches its index entry (so a file a
    /// filter such as Git LFS stores as a pointer has the pointer's size), and of the file itself
    /// for a file that differs from it. A symbolic link has the size of its target's path.
    ///
    /// # Errors
    /// Fails when the size cannot be read.
    pub fn size(&self, path: &str) -> Result<Option<u64>> {
        let Some(blob) = self.entries.get(path) else {
            return Ok(None);
        };

        if matches!(self.source, Source::WorkTree) {
            if let Some(stored) = self.clean.get(path) {
                return self.blob_size(stored, path).map(Some);
            }
            if self.filtered.contains(path) {
                // Listed by `git diff-files` (changed, or only touched): the blob git would store.
                return self.git.stored_size(path).map(Some);
            }
            if let Some(cached) = lock(&self.contents).get(path) {
                return Ok(Some(length(cached)));
            }
            return work_tree_size(&self.git, &self.directories, path).map(Some);
        }

        self.blob_size(blob, path).map(Some)
    }

    /// The size of `blob`, the object of `path`, from the contents already read or from git.
    fn blob_size(&self, blob: &BlobId, path: &str) -> Result<u64> {
        if let Some(cached) = lock(&self.shared.contents).get(blob) {
            return Ok(length(cached));
        }
        let mut sizes = lock(&self.shared.sizes);
        let (known, reader) = &mut *sizes;
        if let Some(size) = known.get(blob) {
            return Ok(*size);
        }
        if reader.is_none() {
            *reader = Some(self.git.object_sizes()?);
        }
        let size = reader
            .as_mut()
            .ok_or_else(|| Error::environment("git cat-file is unavailable"))?
            .size(blob.as_str())?
            .ok_or_else(|| {
                Error::environment(format!(
                    "git has no object {blob} for {path}; the repository is damaged, run git fsck"
                ))
            })?;
        known.insert(blob.clone(), size);
        Ok(size)
    }

    /// Every path [`Snapshot::read`] was asked for since the snapshot was opened, tracked or not, and
    /// stops recording: the workspace takes the paths its configuration was read from.
    pub fn take_asked(&self) -> BTreeSet<String> {
        lock(&self.asked).take().unwrap_or_default()
    }

    /// Starts recording the paths [`Snapshot::read`] is asked for again, from none.
    pub fn record(&self) {
        *lock(&self.asked) = Some(BTreeSet::new());
    }

    /// The contents of a tracked path as UTF-8 text; `None` when the path is not tracked.
    ///
    /// # Errors
    /// Fails when the contents cannot be read or are not UTF-8.
    pub fn text(&self, path: &str) -> Result<Option<String>> {
        match self.read(path)? {
            None => Ok(None),
            Some(bytes) => String::from_utf8(bytes.to_vec())
                .map(Some)
                .map_err(|_| Error::config(format!("{path} is not UTF-8 text"))),
        }
    }
}

/// `"a/b"` becomes `"a/b/"`; `""` stays `""`.
pub(crate) fn directory_prefix(directory: &str) -> String {
    let trimmed = directory.trim_end_matches('/');

    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}/")
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // A panic while holding the lock leaves only a cache behind, which stays consistent.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One `git ls-files --stage` or `git ls-tree` record: mode, object id, stage, path.
struct Record<'a> {
    mode: &'a str,
    oid: &'a str,
    stage: &'a str,
    path: &'a str,
}

fn parse_stage_record(record: &str) -> Result<Record<'_>> {
    let (meta, path) = record
        .split_once('\t')
        .ok_or_else(|| Error::environment(format!("unexpected git index record {record:?}")))?;
    let mut fields = meta.split(' ');
    let (Some(mode), Some(oid), Some(stage)) = (fields.next(), fields.next(), fields.next()) else {
        return Err(Error::environment(format!(
            "unexpected git index record {record:?}"
        )));
    };
    Ok(Record {
        mode,
        oid,
        stage,
        path,
    })
}

const GITLINK: &str = "160000";
const SYMLINK: &str = "120000";

fn index_entries(git: &Git) -> Result<BTreeMap<String, BlobId>> {
    let mut entries = BTreeMap::new();

    for record in split_nul(&git.output(&["ls-files", "-z", "--stage"])?)? {
        let record = parse_stage_record(&record)?;

        if record.stage != "0" {
            return Err(Error::environment(format!(
                "the index has an unmerged entry for {}; resolve the conflict and stage the file",
                record.path
            )));
        }

        if record.mode != GITLINK {
            entries.insert(record.path.to_owned(), BlobId(record.oid.to_owned()));
        }
    }

    Ok(entries)
}

/// The working tree's entries; the paths among them whose file matches its index entry, with the
/// index's blob id; and the regular files among them git converts.
type WorkTreeListing = (
    BTreeMap<String, BlobId>,
    BTreeMap<String, BlobId>,
    BTreeSet<String>,
);

/// The working tree's entries, its clean paths and its converted regular files.
fn work_tree_entries(git: &Git) -> Result<WorkTreeListing> {
    let mut entries = BTreeMap::new();
    let mut symlinks = BTreeSet::new();
    let mut to_hash = BTreeSet::new();

    for record in split_nul(&git.output(&["ls-files", "-z", "--stage"])?)? {
        let record = parse_stage_record(&record)?;

        if record.mode == GITLINK {
            continue;
        }

        if record.mode == SYMLINK {
            symlinks.insert(record.path.to_owned());
        }

        if record.stage != "0" {
            to_hash.insert(record.path.to_owned());
        }

        entries.insert(record.path.to_owned(), BlobId(record.oid.to_owned()));
    }

    // Paths whose working-tree file differs from the index, or is gone.
    for path in git.paths(&["diff-files", "-z", "--name-only", "--no-renames"])? {
        to_hash.insert(path);
    }
    let clean: BTreeMap<String, BlobId> = entries
        .iter()
        .filter(|(path, _)| !to_hash.contains(*path))
        .map(|(path, blob)| (path.clone(), blob.clone()))
        .collect();
    // A file git converts (line endings, a filter) can change bytes while git sees no change:
    // its id is always the hash of the bytes a plugin reads.
    let converted = converted(git, &entries)?;
    to_hash.extend(converted.iter().cloned());

    let mut regular = Vec::new();
    let directories = Directories::default();

    for path in to_hash {
        if directories.behind_symlink(git.root(), &path) {
            // A directory on the way is a symbolic link: the file is not in the repository.
            entries.remove(&path);
            continue;
        }
        let Ok(metadata) = git.root().join(&path).symlink_metadata() else {
            entries.remove(&path);
            continue;
        };

        if metadata.is_dir() {
            // A tracked file replaced by a directory is not a file of this snapshot.
            entries.remove(&path);
        } else if metadata.file_type().is_symlink() {
            symlinks.insert(path.clone());
            let target = read_work_tree(git, &directories, &path)?;
            let oid =
                git.output_with_input(&["hash-object", "--no-filters", "--stdin"], &target)?;
            entries.insert(path, BlobId(trimmed_oid(&oid)?));
        } else {
            symlinks.remove(&path);
            regular.push(path);
        }
    }

    for chunk in regular.chunks(256) {
        let mut arguments = vec!["hash-object", "--no-filters", "--"];
        arguments.extend(chunk.iter().map(String::as_str));
        let output = git.output(&arguments)?;
        let text = String::from_utf8(output)
            .map_err(|_| Error::environment("git hash-object printed invalid UTF-8"))?;
        let oids: Vec<&str> = text.lines().collect();

        if oids.len() != chunk.len() {
            return Err(Error::environment(format!(
                "git hash-object printed {} ids for {} paths",
                oids.len(),
                chunk.len()
            )));
        }

        for (path, oid) in chunk.iter().zip(oids) {
            entries.insert(path.clone(), BlobId(oid.to_owned()));
        }
    }

    let filtered = converted
        .into_iter()
        .filter(|path| entries.contains_key(path) && !symlinks.contains(path))
        .collect();
    Ok((entries, clean, filtered))
}

/// The tracked paths whose working-tree bytes git may convert on the way to or from the index:
/// every path when `core.autocrlf` is on, else the paths an attribute converts (`text`, `eol`,
/// `crlf`, `filter`, `ident`, `working-tree-encoding`). For these, the index id may name other
/// bytes than the file holds, and an edit of line endings alone leaves git's view unchanged.
fn converted(git: &Git, entries: &BTreeMap<String, BlobId>) -> Result<Vec<String>> {
    let autocrlf = git
        .try_text(&["config", "--get", "core.autocrlf"])?
        .map(|value| value.to_ascii_lowercase());
    if matches!(
        autocrlf.as_deref(),
        Some("true" | "yes" | "on" | "1" | "input")
    ) {
        return Ok(entries.keys().cloned().collect());
    }
    let attributes = entries
        .keys()
        .any(|path| path == ".gitattributes" || path.ends_with("/.gitattributes"))
        || git
            .try_text(&["config", "--get", "core.attributesFile"])?
            .is_some()
        || git
            .try_text(&["rev-parse", "--git-path", "info/attributes"])?
            .is_some_and(|path| git.root().join(path).is_file());
    if !attributes {
        return Ok(Vec::new());
    }
    let mut input = Vec::new();
    for path in entries.keys() {
        input.extend_from_slice(path.as_bytes());
        input.push(0);
    }
    let output = git.output_with_input(
        &[
            "check-attr",
            "-z",
            "--stdin",
            "text",
            "eol",
            "crlf",
            "filter",
            "ident",
            "working-tree-encoding",
        ],
        &input,
    )?;
    let fields = split_nul(&output)?;
    let mut converted = BTreeSet::new();
    for triple in fields.chunks(3) {
        if let [path, attribute, value] = triple {
            let converts = match value.as_str() {
                "unspecified" | "unset" => false,
                // `-text`, `-crlf` say "never convert"; a set `filter` or `ident` converts.
                "set" => attribute != "eol",
                _ => true,
            };
            if converts {
                converted.insert(path.clone());
            }
        }
    }
    Ok(converted.into_iter().collect())
}

fn commit_entries(git: &Git, commit: &str) -> Result<BTreeMap<String, BlobId>> {
    let mut entries = BTreeMap::new();

    for record in split_nul(&git.output(&["ls-tree", "-r", "-z", "--full-tree", commit])?)? {
        let (meta, path) = record
            .split_once('\t')
            .ok_or_else(|| Error::environment(format!("unexpected git tree record {record:?}")))?;
        let fields: Vec<&str> = meta.split(' ').collect();

        if let (Some(&"blob"), Some(oid)) = (fields.get(1), fields.get(2)) {
            entries.insert(path.to_owned(), BlobId((*oid).to_owned()));
        }
    }

    Ok(entries)
}

/// Which directories of a working tree are symbolic links, each asked of the file system once:
/// the snapshot is one moment of the tree, so a directory's answer holds for its life.
#[derive(Debug, Default)]
struct Directories(Mutex<HashMap<String, bool>>);

impl Directories {
    /// Whether a directory on the way from `root` to `path` is a symbolic link. A directory that
    /// cannot be read is not one, as before the answers were kept.
    fn behind_symlink(&self, root: &std::path::Path, path: &str) -> bool {
        path.match_indices('/').any(|(end, _)| {
            let directory = &path[..end];
            if let Some(&linked) = lock(&self.0).get(directory) {
                return linked;
            }
            let linked = root
                .join(directory)
                .symlink_metadata()
                .is_ok_and(|metadata| metadata.file_type().is_symlink());
            lock(&self.0).insert(directory.to_owned(), linked);
            linked
        })
    }
}

fn read_work_tree(git: &Git, directories: &Directories, path: &str) -> Result<Vec<u8>> {
    if directories.behind_symlink(git.root(), path) {
        return Err(Error::environment(format!(
            "cannot read {path}: a directory on its way is a symbolic link, which leads outside the tracked tree; restore the directory or git rm the path"
        )));
    }
    let full = git.root().join(path);
    let metadata = full.symlink_metadata().map_err(|error| {
        Error::environment(format!("cannot read {path} from the working tree: {error}"))
    })?;

    if metadata.file_type().is_symlink() {
        Ok(std::fs::read_link(&full)?
            .to_string_lossy()
            .as_bytes()
            .to_vec())
    } else {
        std::fs::read(&full).map_err(|error| {
            Error::environment(format!("cannot read {path} from the working tree: {error}"))
        })
    }
}

fn length(contents: &[u8]) -> u64 {
    u64::try_from(contents.len()).unwrap_or(u64::MAX)
}

/// The size of a working-tree file, as [`read_work_tree`] would read it.
fn work_tree_size(git: &Git, directories: &Directories, path: &str) -> Result<u64> {
    if directories.behind_symlink(git.root(), path) {
        return Err(Error::environment(format!(
            "cannot read {path}: a directory on its way is a symbolic link, which leads outside the tracked tree; restore the directory or git rm the path"
        )));
    }
    let full = git.root().join(path);
    let metadata = full.symlink_metadata().map_err(|error| {
        Error::environment(format!("cannot read {path} from the working tree: {error}"))
    })?;
    if metadata.file_type().is_symlink() {
        let target = std::fs::read_link(&full)?;
        Ok(length(target.to_string_lossy().as_bytes()))
    } else {
        Ok(metadata.len())
    }
}

fn trimmed_oid(output: &[u8]) -> Result<String> {
    std::str::from_utf8(output)
        .map(|text| text.trim().to_owned())
        .map_err(|_| Error::environment("git hash-object printed invalid UTF-8"))
}

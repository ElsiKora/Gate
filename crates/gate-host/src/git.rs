//! The one module that runs the `git` binary. Every other module asks git through [`Git`].

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;

use crate::error::{Error, Result};

/// The all-zero object id git uses for "no object" (a new branch in a pre-push hook).
pub const ZERO_OID: &str = "0000000000000000000000000000000000000000";

/// A git work tree addressed by its top-level directory.
#[derive(Clone, Debug)]
pub struct Git {
    root: PathBuf,
}

impl Git {
    /// Finds the work tree that contains `start`.
    ///
    /// # Errors
    /// Fails when git cannot start or `start` is not inside a git work tree.
    pub fn discover(start: &Path) -> Result<Self> {
        let output = Command::new("git")
            .arg("-C")
            .arg(start)
            .args(["rev-parse", "--show-toplevel"])
            .env("LC_ALL", "C")
            .output()
            .map_err(|error| {
                Error::environment(format!(
                    "cannot run git ({error}); install git and put it on PATH"
                ))
            })?;

        if !output.status.success() {
            return Err(Error::environment(format!(
                "{} is not inside a git work tree; run gate from a git repository",
                start.display()
            )));
        }

        Ok(Self {
            root: PathBuf::from(decode(output.stdout)?.trim_end()),
        })
    }

    /// The top-level directory of the work tree.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn command(&self) -> Command {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(&self.root)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("LC_ALL", "C");
        command
    }

    /// Runs git and returns its standard output.
    ///
    /// # Errors
    /// Fails when git cannot start or exits with a non-zero status.
    pub fn output(&self, arguments: &[&str]) -> Result<Vec<u8>> {
        let output = self.command().args(arguments).output()?;

        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(Error::environment(format!(
                "git {} failed: {}",
                arguments.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }

    /// Runs git with `input` on its standard input and returns its standard output.
    ///
    /// # Errors
    /// Fails when git cannot start or exits with a non-zero status.
    pub fn output_with_input(&self, arguments: &[&str], input: &[u8]) -> Result<Vec<u8>> {
        let mut child = self
            .command()
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| Error::environment("git has no standard input"))?;
            stdin.write_all(input)?;
        }
        let output = child.wait_with_output()?;

        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(Error::environment(format!(
                "git {} failed: {}",
                arguments.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }

    /// Runs git and returns its trimmed output, or `None` on a non-zero exit.
    ///
    /// # Errors
    /// Fails when git cannot start or prints invalid UTF-8.
    pub fn try_text(&self, arguments: &[&str]) -> Result<Option<String>> {
        let output = self
            .command()
            .args(arguments)
            .stderr(Stdio::null())
            .output()?;

        if output.status.success() {
            Ok(Some(decode(output.stdout)?.trim_end().to_owned()))
        } else {
            Ok(None)
        }
    }

    /// The full object id of the commit `revision` names, or `None` when it names no commit.
    ///
    /// # Errors
    /// Fails when git cannot start.
    pub fn resolve_commit(&self, revision: &str) -> Result<Option<String>> {
        self.try_text(&[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ])
    }

    /// The tree of git's merge of two commits (`git merge-tree --write-tree`), or `None` when the
    /// merge conflicts.
    ///
    /// # Errors
    /// Fails when git cannot start or fails for another reason than a conflict.
    pub fn merge_tree(&self, first: &str, second: &str) -> Result<Option<String>> {
        let output = self
            .command()
            .args([
                "merge-tree",
                "--write-tree",
                "--end-of-options",
                first,
                second,
            ])
            .output()?;
        // Exit 0 prints the merged tree first; exit 1 is a conflict.
        match output.status.code() {
            Some(0) => Ok(decode(output.stdout)?.lines().next().map(str::to_owned)),
            Some(1) => Ok(None),
            _ => Err(Error::environment(format!(
                "git merge-tree {first} {second} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))),
        }
    }

    /// The commit `HEAD` names, or `None` on an unborn branch.
    ///
    /// # Errors
    /// Fails when git cannot start.
    pub fn head(&self) -> Result<Option<String>> {
        self.resolve_commit("HEAD")
    }

    /// The branch `HEAD` is on, without `refs/heads/`, or `None` on a detached `HEAD`. Read from
    /// the full ref: `symbolic-ref --short` answers `heads/<name>` when a tag has the same name.
    ///
    /// # Errors
    /// Fails when git cannot start.
    pub fn branch(&self) -> Result<Option<String>> {
        Ok(self
            .try_text(&["symbolic-ref", "--quiet", "HEAD"])?
            .map(|reference| {
                reference
                    .strip_prefix("refs/heads/")
                    .map_or_else(|| reference.clone(), str::to_owned)
            }))
    }

    /// The best common ancestor of two commits, or `None` when they share no history.
    ///
    /// # Errors
    /// Fails when git cannot start.
    pub fn merge_base(&self, left: &str, right: &str) -> Result<Option<String>> {
        self.try_text(&["merge-base", "--end-of-options", left, right])
    }

    /// Paths listed by a NUL-separated git command (`-z`), in git's order.
    ///
    /// # Errors
    /// Fails when git fails or lists a path that is not UTF-8.
    pub fn paths(&self, arguments: &[&str]) -> Result<Vec<String>> {
        split_nul(&self.output(arguments)?)
    }

    /// Starts a `git cat-file --batch` reader.
    ///
    /// # Errors
    /// Fails when git cannot start.
    pub fn cat_file(&self) -> Result<CatFile> {
        let mut child = self
            .command()
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::environment("git cat-file has no standard input"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::environment("git cat-file has no standard output"))?;

        Ok(CatFile {
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout),
        })
    }
}

/// A persistent reader of git objects by id.
#[derive(Debug)]
pub struct CatFile {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl CatFile {
    /// Reads one object by id; `None` when it does not exist.
    ///
    /// # Errors
    /// Fails when the reader protocol breaks.
    pub fn read(&mut self, object: &str) -> Result<Option<Vec<u8>>> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| Error::environment("git cat-file is closed"))?;
        writeln!(stdin, "{object}")?;
        stdin.flush()?;

        let mut header = String::new();
        self.stdout.read_line(&mut header)?;

        if header.is_empty() || header.ends_with(" missing\n") {
            return Ok(None);
        }

        let size = header
            .trim_end()
            .rsplit(' ')
            .next()
            .and_then(|value| value.parse::<usize>().ok())
            .ok_or_else(|| {
                Error::environment(format!("unexpected git cat-file header {header:?}"))
            })?;
        let mut content = vec![0; size];
        self.stdout.read_exact(&mut content)?;
        let mut newline = [0; 1];
        self.stdout.read_exact(&mut newline)?;

        Ok(Some(content))
    }

    /// Reads the object `name` names (an id or any revision, such as `<commit>^{tree}`): its id,
    /// its type and its content; `None` when it names nothing.
    ///
    /// # Errors
    /// Fails when the reader protocol breaks.
    pub fn object(&mut self, name: &str) -> Result<Option<Object>> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| Error::environment("git cat-file is closed"))?;
        writeln!(stdin, "{name}")?;
        stdin.flush()?;
        answer(&mut self.stdout, name)
    }

    /// Reads the objects `names` name in one round trip: every name is written while the answers
    /// are read, so neither pipe fills while the other waits.
    ///
    /// # Errors
    /// Fails when the reader protocol breaks.
    pub fn objects(&mut self, names: &[String]) -> Result<Vec<Option<Object>>> {
        if let [name] = names {
            return Ok(vec![self.object(name)?]);
        }
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| Error::environment("git cat-file is closed"))?;
        let stdout = &mut self.stdout;
        std::thread::scope(|scope| {
            let writer = scope.spawn(move || -> std::io::Result<()> {
                for name in names {
                    writeln!(stdin, "{name}")?;
                }
                stdin.flush()
            });
            let answers = names
                .iter()
                .map(|name| answer(stdout, name))
                .collect::<Result<Vec<_>>>();
            writer
                .join()
                .map_err(|_| Error::environment("the git cat-file writer panicked"))??;
            answers
        })
    }

    /// Reads the blob `spec` names (`<commit>:<path>`); `None` when it names nothing.
    ///
    /// # Errors
    /// Fails when the reader protocol breaks, or when `spec` names an object that is not a blob.
    pub fn read_blob(&mut self, spec: &str) -> Result<Option<Vec<u8>>> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| Error::environment("git cat-file is closed"))?;
        writeln!(stdin, "{spec}")?;
        stdin.flush()?;

        let mut header = String::new();
        self.stdout.read_line(&mut header)?;
        if header.is_empty() || header.ends_with(" missing\n") {
            return Ok(None);
        }
        let mut fields = header.trim_end().rsplitn(3, ' ');
        let size = fields.next().and_then(|value| value.parse::<usize>().ok());
        let kind = fields.next();
        let (Some(size), Some(kind)) = (size, kind) else {
            return Err(Error::environment(format!(
                "unexpected git cat-file header {header:?} for {spec:?}"
            )));
        };
        let mut content = vec![0; size];
        self.stdout.read_exact(&mut content)?;
        let mut newline = [0; 1];
        self.stdout.read_exact(&mut newline)?;
        if kind != "blob" {
            return Err(Error::environment(format!(
                "{spec:?} names a {kind}, not a file"
            )));
        }
        Ok(Some(content))
    }
}

impl Drop for CatFile {
    fn drop(&mut self) {
        drop(self.stdin.take());
        // The reader exits when its input closes; its status carries nothing to report.
        let _status = self.child.wait();
    }
}

fn decode(bytes: Vec<u8>) -> Result<String> {
    String::from_utf8(bytes).map_err(|_| Error::environment("git printed invalid UTF-8"))
}

/// Splits NUL-separated git output into UTF-8 strings.
///
/// # Errors
/// Fails when a part is not UTF-8.
pub(crate) fn split_nul(bytes: &[u8]) -> Result<Vec<String>> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| {
            String::from_utf8(part.to_vec()).map_err(|_| {
                Error::environment(
                    "git listed a path that is not UTF-8; rename it, gate reads UTF-8 paths only",
                )
            })
        })
        .collect()
}

/// Reads the answer of a `git cat-file --batch` reader to `name`: the object, or `None` when the
/// name names nothing.
///
/// # Errors
/// Fails when the reader protocol breaks.
fn answer(stdout: &mut BufReader<ChildStdout>, name: &str) -> Result<Option<Object>> {
    let mut header = String::new();
    stdout.read_line(&mut header)?;
    if header.is_empty() || header.ends_with(" missing\n") {
        return Ok(None);
    }
    let mut fields = header.trim_end().splitn(3, ' ');
    let (Some(id), Some(kind), Some(size)) = (
        fields.next(),
        fields.next(),
        fields.next().and_then(|value| value.parse::<usize>().ok()),
    ) else {
        return Err(Error::environment(format!(
            "unexpected git cat-file header {header:?} for {name:?}"
        )));
    };
    let mut content = vec![0; size];
    stdout.read_exact(&mut content)?;
    let mut newline = [0; 1];
    stdout.read_exact(&mut newline)?;
    Ok(Some(Object {
        id: id.to_owned(),
        kind: kind.to_owned(),
        content,
    }))
}

/// One object a [`CatFile`] read.
#[derive(Clone, Debug)]
pub struct Object {
    /// The full object id.
    pub id: String,
    /// `blob`, `tree`, `commit` or `tag`.
    pub kind: String,
    /// The raw content.
    pub content: Vec<u8>,
}

/// The mode of a tree entry that is a tree.
const TREE_MODE: &[u8] = b"40000";

/// The commits whose ancestry [`Objects::is_ancestor`] reads before it leaves the question to git.
const ANCESTRY_LIMIT: usize = 200_000;

/// One entry of a tree object.
struct Entry {
    name: Vec<u8>,
    mode: Vec<u8>,
    id: String,
}

impl Entry {
    fn is_tree(&self) -> bool {
        self.mode == TREE_MODE
    }

    /// Git's order of tree entries: by name, a tree's name followed by `/`.
    fn order(&self, other: &Self) -> Ordering {
        let key = |entry: &Self| {
            let mut key = entry.name.clone();
            if entry.is_tree() {
                key.push(b'/');
            }
            key
        };
        key(self).cmp(&key(other))
    }
}

/// A commit of the ancestry [`Objects`] has read.
struct Node {
    parents: Vec<String>,
    /// One more than the highest generation of its parents; 1 for a root commit.
    generation: Option<u32>,
}

/// The git questions about commits and trees that a run asks many times, answered through one
/// `git cat-file --batch` reader instead of one git process per question.
pub struct Objects {
    git: Git,
    reader: CatFile,
    commits: HashMap<String, Node>,
    /// Whether the ancestry of the first commit asked about was loaded with one `git rev-list`.
    ancestry_loaded: bool,
    /// The entries of the trees read, by id: consecutive commits share most of their trees.
    trees: HashMap<String, Arc<Vec<Entry>>>,
    /// The tree id of each commit asked about by its full id.
    tree_of: HashMap<String, String>,
    /// Set when an ancestry could not be read in full (a shallow clone, a missing object, more
    /// than [`ANCESTRY_LIMIT`] commits): git answers `is-ancestor` from then on.
    ancestry_by_git: bool,
}

impl std::fmt::Debug for Objects {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Objects")
            .field("commits", &self.commits.len())
            .field("ancestry_by_git", &self.ancestry_by_git)
            .finish_non_exhaustive()
    }
}

impl Objects {
    /// Starts the reader of `git`.
    ///
    /// # Errors
    /// Fails when git cannot start.
    pub fn new(git: &Git) -> Result<Self> {
        Ok(Self {
            git: git.clone(),
            reader: git.cat_file()?,
            commits: HashMap::new(),
            ancestry_by_git: false,
            ancestry_loaded: false,
            trees: HashMap::new(),
            tree_of: HashMap::new(),
        })
    }

    /// The blob `<commit>:<path>` names; `None` when it names nothing.
    ///
    /// # Errors
    /// Fails when the reader protocol breaks, or the path names something that is not a file.
    pub fn read_at(&mut self, commit: &str, path: &str) -> Result<Option<Vec<u8>>> {
        let spec = format!("{commit}:{path}");
        if spec.contains('\n') {
            // The reader takes one name per line: git answers a name with a line break.
            let Some(object) = self.git.try_text(&[
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                &spec,
            ])?
            else {
                return Ok(None);
            };
            return self.git.output(&["cat-file", "blob", &object]).map(Some);
        }
        self.reader.read_blob(&spec)
    }

    /// The id of the tree of `commit`; `None` when it names no commit or tree.
    ///
    /// # Errors
    /// Fails when the reader protocol breaks.
    pub fn tree(&mut self, commit: &str) -> Result<Option<String>> {
        Ok(self.trees_of(&[commit])?.pop().flatten())
    }

    /// The tree ids of `commits`, the unknown ones read in one round trip; `None` for a name that
    /// names no commit or tree. The root trees' entries are kept, as the answer carries them.
    fn trees_of(&mut self, commits: &[&str]) -> Result<Vec<Option<String>>> {
        let wanted: Vec<String> = commits
            .iter()
            .filter(|commit| !self.tree_of.contains_key(**commit) && !commit.contains('\n'))
            .map(|commit| format!("{commit}^{{tree}}"))
            .collect();
        let mut answers = if wanted.is_empty() {
            Vec::new()
        } else {
            self.reader.objects(&wanted)?
        }
        .into_iter();
        let mut ids = Vec::with_capacity(commits.len());
        for commit in commits {
            if let Some(id) = self.tree_of.get(*commit) {
                ids.push(Some(id.clone()));
                continue;
            }
            if commit.contains('\n') {
                ids.push(None);
                continue;
            }
            let Some(object) = answers
                .next()
                .flatten()
                .filter(|object| object.kind == "tree")
            else {
                ids.push(None);
                continue;
            };
            let entries = parse_tree(&object)?;
            self.trees.insert(object.id.clone(), Arc::new(entries));
            if is_object_id(commit) {
                self.tree_of.insert((*commit).to_owned(), object.id.clone());
            }
            ids.push(Some(object.id));
        }
        Ok(ids)
    }

    /// The paths that differ between the trees of `old` and `new`, as `git diff --name-only
    /// --no-renames -z old new` lists them, in the same order.
    ///
    /// # Errors
    /// Fails when either names no commit or tree, a tree does not read, or a path is not UTF-8.
    pub fn changed_paths(&mut self, old: &str, new: &str) -> Result<Vec<String>> {
        let trees = self.trees_of(&[old, new])?;
        let tree = |index: usize, name: &str| {
            trees.get(index).cloned().flatten().ok_or_else(|| {
                Error::environment(format!("{name:?} names no commit or tree; fetch it"))
            })
        };
        let (old, new) = (tree(0, old)?, tree(1, new)?);

        // One level of both trees at a time, every tree of a level read in one round trip.
        let mut paths: Vec<Vec<u8>> = Vec::new();
        let mut level = vec![(Some(old), Some(new), Vec::new())];
        while !level.is_empty() {
            let mut wanted: Vec<String> = level
                .iter()
                .filter(|(old, new, _)| old != new)
                .flat_map(|(old, new, _)| old.iter().chain(new.iter()).cloned())
                .collect();
            wanted.sort_unstable();
            wanted.dedup();
            let entries = self.entries(&wanted)?;
            let mut next = Vec::new();
            for (old, new, prefix) in level {
                if old == new {
                    continue;
                }
                let side = |id: &Option<String>| {
                    id.as_ref()
                        .and_then(|id| entries.get(id))
                        .map_or(&[][..], |entries| entries.as_slice())
                };
                diff_level(side(&old), side(&new), &prefix, &mut paths, &mut next);
            }
            level = next;
        }
        // Git lists a diff in tree order, which is the byte order of the full paths: a tree's
        // name is always followed by `/` within them.
        paths.sort_unstable();
        paths
            .into_iter()
            .map(|path| {
                String::from_utf8(path).map_err(|_| {
                    Error::environment(
                        "git listed a path that is not UTF-8; rename it, gate reads UTF-8 paths only",
                    )
                })
            })
            .collect()
    }

    /// The entries of each of the trees `ids`, by id; the ones not read before in one round trip.
    fn entries(&mut self, ids: &[String]) -> Result<HashMap<String, Arc<Vec<Entry>>>> {
        let missing: Vec<String> = ids
            .iter()
            .filter(|id| !self.trees.contains_key(*id))
            .cloned()
            .collect();
        if !missing.is_empty() {
            let objects = self.reader.objects(&missing)?;
            for (id, object) in missing.iter().zip(objects) {
                let object = object
                    .filter(|object| object.kind == "tree")
                    .ok_or_else(|| Error::environment(format!("git has no tree {id}; fetch it")))?;
                self.trees
                    .insert(id.clone(), Arc::new(parse_tree(&object)?));
            }
        }
        Ok(ids
            .iter()
            .filter_map(|id| Some((id.clone(), Arc::clone(self.trees.get(id)?))))
            .collect())
    }

    /// Whether `ancestor` is `descendant` or one of its ancestors, as `git merge-base
    /// --is-ancestor` answers: `false` when either names no commit.
    ///
    /// # Errors
    /// Fails when the reader protocol breaks or git cannot start.
    pub fn is_ancestor(&mut self, ancestor: &str, descendant: &str) -> Result<bool> {
        if !self.ancestry_by_git
            && let Some(answer) = self.ancestry(ancestor, descendant)?
        {
            return Ok(answer);
        }
        Ok(self
            .git
            .try_text(&[
                "merge-base",
                "--is-ancestor",
                "--end-of-options",
                ancestor,
                descendant,
            ])?
            .is_some())
    }

    /// [`Objects::is_ancestor`] from the commits read; `None` when git is to answer.
    fn ancestry(&mut self, ancestor: &str, descendant: &str) -> Result<Option<bool>> {
        let (Some(ancestor), Some(descendant)) = (self.commit(ancestor)?, self.commit(descendant)?)
        else {
            return Ok(None);
        };
        if ancestor == descendant {
            return Ok(Some(true));
        }
        if !self.ancestry_loaded {
            self.ancestry_loaded = true;
            self.load_ancestry(&descendant)?;
        }
        let (Some(low), Some(high)) = (self.generation(&ancestor)?, self.generation(&descendant)?)
        else {
            self.ancestry_by_git = true;
            return Ok(None);
        };
        if low >= high {
            return Ok(Some(false));
        }
        // Every commit on a path down to the ancestor has a higher generation than it.
        let mut seen = HashSet::from([descendant.clone()]);
        let mut stack = vec![descendant];
        while let Some(id) = stack.pop() {
            let parents = self
                .commits
                .get(&id)
                .map(|node| node.parents.clone())
                .unwrap_or_default();
            for parent in parents {
                if parent == ancestor {
                    return Ok(Some(true));
                }
                let generation = self.commits.get(&parent).and_then(|node| node.generation);
                if generation.is_some_and(|generation| generation > low)
                    && seen.insert(parent.clone())
                {
                    stack.push(parent);
                }
            }
        }
        Ok(Some(false))
    }

    /// Reads the ancestry of `commit` with one `git rev-list --parents`, so that its generations
    /// need no round trip per commit; more than [`ANCESTRY_LIMIT`] commits leaves `is-ancestor`
    /// to git.
    fn load_ancestry(&mut self, commit: &str) -> Result<()> {
        let limit = format!("--max-count={}", ANCESTRY_LIMIT + 1);
        let output = self.git.output(&[
            "rev-list",
            "--parents",
            &limit,
            "--end-of-options",
            commit,
            "--",
        ])?;
        let text = String::from_utf8_lossy(&output);
        if text.lines().count() > ANCESTRY_LIMIT {
            self.ancestry_by_git = true;
            return Ok(());
        }
        for line in text.lines() {
            let mut ids = line.split(' ').map(str::to_owned);
            let Some(id) = ids.next() else { continue };
            self.commits.entry(id).or_insert_with(|| Node {
                parents: ids.collect(),
                generation: None,
            });
        }
        Ok(())
    }

    /// The id of the commit `name` names, its node read; `None` when it names no commit.
    fn commit(&mut self, name: &str) -> Result<Option<String>> {
        if self.commits.contains_key(name) {
            return Ok(Some(name.to_owned()));
        }
        if name.contains('\n') {
            return Ok(None);
        }
        let Some(object) = self.reader.object(&format!("{name}^{{commit}}"))? else {
            return Ok(None);
        };
        if object.kind != "commit" {
            return Ok(None);
        }
        let id = object.id.clone();
        self.insert(object);
        Ok(Some(id))
    }

    fn insert(&mut self, object: Object) {
        let parents = object
            .content
            .split(|byte| *byte == b'\n')
            .take_while(|line| !line.is_empty())
            .filter_map(|line| line.strip_prefix(b"parent "))
            .map(|id| String::from_utf8_lossy(id).into_owned())
            .collect();
        self.commits.insert(
            object.id,
            Node {
                parents,
                generation: None,
            },
        );
    }

    /// The generation of the commit `id`, every ancestor's computed on the way; `None` when an
    /// ancestor does not read (a shallow clone's boundary) or the ancestry passes
    /// [`ANCESTRY_LIMIT`].
    fn generation(&mut self, id: &str) -> Result<Option<u32>> {
        let mut stack = vec![id.to_owned()];
        while let Some(top) = stack.last().cloned() {
            if self.commits.len() > ANCESTRY_LIMIT {
                return Ok(None);
            }
            if !self.commits.contains_key(&top) {
                match self.reader.object(&top)? {
                    Some(object) if object.kind == "commit" => self.insert(object),
                    _ => return Ok(None),
                }
            }
            let Some(node) = self.commits.get(&top) else {
                return Ok(None);
            };
            if node.generation.is_some() {
                stack.pop();
                continue;
            }
            let mut highest = 0;
            let mut pending = None;
            for parent in &node.parents {
                if let Some(generation) = self.commits.get(parent).and_then(|node| node.generation)
                {
                    highest = highest.max(generation);
                } else {
                    pending = Some(parent.clone());
                    break;
                }
            }
            if let Some(parent) = pending {
                stack.push(parent);
            } else {
                if let Some(node) = self.commits.get_mut(&top) {
                    node.generation = Some(highest + 1);
                }
                stack.pop();
            }
        }
        Ok(self.commits.get(id).and_then(|node| node.generation))
    }
}

/// The entries of a tree object.
fn parse_tree(object: &Object) -> Result<Vec<Entry>> {
    let tree = &object.id;
    let raw = tree.len() / 2;
    let mut entries = Vec::new();
    let mut rest = object.content.as_slice();
    while !rest.is_empty() {
        let broken = || Error::environment(format!("git tree {tree} does not parse"));
        let space = rest
            .iter()
            .position(|byte| *byte == b' ')
            .ok_or_else(broken)?;
        let nul = rest.iter().position(|byte| *byte == 0).ok_or_else(broken)?;
        if nul < space || rest.len() < nul + 1 + raw {
            return Err(broken());
        }
        let id = rest[nul + 1..nul + 1 + raw].iter().fold(
            String::with_capacity(raw * 2),
            |mut hex, byte| {
                use std::fmt::Write as _;
                let _infallible = write!(hex, "{byte:02x}");
                hex
            },
        );
        entries.push(Entry {
            mode: rest[..space].to_vec(),
            name: rest[space + 1..nul].to_vec(),
            id,
        });
        rest = &rest[nul + 1 + raw..];
    }
    Ok(entries)
}

/// The tree pairs below one level of a diff, to read next: the old side's tree, the new side's,
/// and the path prefix of their entries.
type Level = Vec<(Option<String>, Option<String>, Vec<u8>)>;

/// Diffs the entries of one tree level: a file that differs is listed, a tree that differs or
/// that only one side has goes to `next`.
fn diff_level(
    old: &[Entry],
    new: &[Entry],
    prefix: &[u8],
    paths: &mut Vec<Vec<u8>>,
    next: &mut Level,
) {
    let path = |entry: &Entry| {
        let mut path = prefix.to_vec();
        path.extend_from_slice(&entry.name);
        path
    };
    let nested = |entry: &Entry| {
        let mut nested = path(entry);
        nested.push(b'/');
        nested
    };
    let (mut left, mut right) = (old.iter().peekable(), new.iter().peekable());
    loop {
        let order = match (left.peek(), right.peek()) {
            (None, None) => return,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(before), Some(after)) => before.order(after),
        };
        match order {
            Ordering::Less => {
                if let Some(entry) = left.next() {
                    if entry.is_tree() {
                        next.push((Some(entry.id.clone()), None, nested(entry)));
                    } else {
                        paths.push(path(entry));
                    }
                }
            }
            Ordering::Greater => {
                if let Some(entry) = right.next() {
                    if entry.is_tree() {
                        next.push((None, Some(entry.id.clone()), nested(entry)));
                    } else {
                        paths.push(path(entry));
                    }
                }
            }
            Ordering::Equal => {
                if let (Some(before), Some(after)) = (left.next(), right.next()) {
                    if before.is_tree() {
                        if before.id != after.id {
                            next.push((
                                Some(before.id.clone()),
                                Some(after.id.clone()),
                                nested(before),
                            ));
                        }
                    } else if before.id != after.id || before.mode != after.mode {
                        paths.push(path(before));
                    }
                }
            }
        }
    }
}

/// Whether `name` is a full object id, which names the same object for good.
pub(crate) fn is_object_id(name: &str) -> bool {
    matches!(name.len(), 40 | 64) && name.bytes().all(|byte| byte.is_ascii_hexdigit())
}

//! The one module that runs the `git` binary. Every other module asks git through [`Git`].

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

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

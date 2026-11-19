//! Answers to git questions about full commit ids (architecture §9). Such a question answers the
//! same for good, so a run reads back what earlier runs asked and asks git only what is new; a
//! history rule that reruns because main moved by one commit asks about that commit only.
//!
//! A run writes the answers it learned to one new file under `git/answers/<FORMAT>/`, named by
//! the digest of its content, and reads every file there on its first question. When the files
//! pass [`MERGE_AT`], a run writes them all to one file and removes the ones it read. A file that
//! cannot be read or does not parse is skipped: its questions are asked again.
//!
//! A `read-at` answer is the id of the blob a path names at a commit; the blob's content is stored
//! once by its id under `git/blobs/`, so a later run reads it without git.

use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::path::PathBuf;

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::{Cache, Sha256Sum};
use crate::error::{Error, Result};

/// Raised when an answer written before would be read differently.
const FORMAT: u32 = 1;

/// How many answer files make a run merge them into one.
const MERGE_AT: usize = 16;

/// A question: its kind and its arguments, every argument a full object id or a fixed value.
pub(crate) type Question = (&'static str, Vec<String>);

/// What a question answered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Answer {
    /// `changed-paths`.
    Paths(Vec<String>),
    /// `tree`, and `merge-tree` (`None` when the parents conflict).
    Tree(Option<String>),
    /// `is-ancestor`.
    Holds(bool),
    /// `read-at`: the id of the blob the path names (`None` when it names nothing).
    Blob(Option<String>),
}

/// The answers known to a run: read from the cache, and learned in the run.
#[derive(Debug, Default)]
pub(crate) struct Answers {
    /// Read from the cache.
    stored: HashMap<Question, Answer>,
    /// Learned in this run.
    learned: HashMap<Question, Answer>,
    /// The stored answers this run used.
    used: HashSet<Question>,
    /// The files read, removed when this run merges them.
    read: Vec<PathBuf>,
}

impl Answers {
    /// Every answer stored in `cache`.
    pub(crate) fn load(cache: &Cache) -> Self {
        let mut answers = Self::default();
        // The files sit two levels down, as every answer of the cache does: `<2 hex>/<digest>`.
        let files = std::fs::read_dir(directory(cache))
            .into_iter()
            .flatten()
            .filter_map(std::result::Result::ok)
            .filter_map(|shard| std::fs::read_dir(shard.path()).ok())
            .flatten()
            .filter_map(std::result::Result::ok);
        for file in files {
            let path = file.path();
            let Some(parsed) = std::fs::read(&path).ok().and_then(|bytes| parse(&bytes)) else {
                continue;
            };
            answers.stored.extend(parsed);
            answers.read.push(path);
        }
        answers
    }

    /// The answer to `question`, when known, and whether this is the first use in this run of
    /// an answer from the cache.
    pub(crate) fn get(&mut self, question: &Question) -> Option<(Answer, bool)> {
        if let Some(answer) = self.stored.get(question) {
            let first_use = self.used.insert(question.clone());
            return Some((answer.clone(), first_use));
        }
        self.learned
            .get(question)
            .map(|answer| (answer.clone(), false))
    }

    /// Records what `question` answered.
    pub(crate) fn learn(&mut self, question: Question, answer: Answer) {
        if !self.stored.contains_key(&question) {
            self.learned.insert(question, answer);
        }
    }

    /// Writes what this run learned to `cache`; merges the stored files when they are many.
    ///
    /// # Errors
    /// When the file cannot be written or moved into place.
    pub(crate) fn store(&self, cache: &Cache) -> Result<()> {
        if self.learned.is_empty() {
            return Ok(());
        }
        let merge = self.read.len() + 1 > MERGE_AT;
        let mut all: Vec<(&Question, &Answer)> = if merge {
            self.stored.iter().chain(&self.learned).collect()
        } else {
            self.learned.iter().collect()
        };
        all.sort_by(|left, right| left.0.cmp(right.0));
        let lines: Vec<String> = all
            .into_iter()
            .map(|(question, answer)| line(question, answer))
            .collect();
        let mut content = lines.join("\n");
        content.push('\n');
        let digest = Sha256Sum::from_digest(&Sha256::digest(content.as_bytes()));
        let mut temporary = cache.temporary()?;
        temporary
            .file()
            .write_all(content.as_bytes())
            .map_err(|error| {
                Error::environment(format!(
                    "cannot write {}: {error}",
                    temporary.path().display()
                ))
            })?;
        cache.insert_answer(temporary, &kind(), &digest)?;
        if merge {
            for file in &self.read {
                // Another run may have merged and removed it already.
                let _gone = std::fs::remove_file(file);
            }
        }
        Ok(())
    }
}

/// The kind of [`Cache::answer`] the files are stored under.
fn kind() -> String {
    format!("answers/{FORMAT}")
}

fn directory(cache: &Cache) -> PathBuf {
    cache.git().join(kind())
}

fn line(question: &Question, answer: &Answer) -> String {
    let value = match answer {
        Answer::Paths(paths) => json!({ "paths": paths }),
        Answer::Tree(tree) => json!({ "tree": tree }),
        Answer::Holds(holds) => json!({ "holds": holds }),
        Answer::Blob(blob) => json!({ "blob": blob }),
    };
    json!({ "kind": question.0, "of": question.1, "answer": value }).to_string()
}

/// The answers of one file; `None` when any line does not parse.
fn parse(bytes: &[u8]) -> Option<Vec<(Question, Answer)>> {
    let text = std::str::from_utf8(bytes).ok()?;
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let value: Value = serde_json::from_str(line).ok()?;
            let kind = KINDS
                .iter()
                .copied()
                .find(|kind| value.get("kind").and_then(Value::as_str) == Some(*kind))?;
            let of = value
                .get("of")?
                .as_array()?
                .iter()
                .map(|argument| argument.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()?;
            let answer = value.get("answer")?;
            let answer = if let Some(paths) = answer.get("paths") {
                Answer::Paths(
                    paths
                        .as_array()?
                        .iter()
                        .map(|path| path.as_str().map(str::to_owned))
                        .collect::<Option<Vec<_>>>()?,
                )
            } else if let Some(tree) = answer.get("tree") {
                Answer::Tree(match tree {
                    Value::Null => None,
                    Value::String(tree) => Some(tree.clone()),
                    _ => return None,
                })
            } else if let Some(blob) = answer.get("blob") {
                Answer::Blob(match blob {
                    Value::Null => None,
                    Value::String(blob) => Some(blob.clone()),
                    _ => return None,
                })
            } else {
                Answer::Holds(answer.get("holds")?.as_bool()?)
            };
            well_formed(kind, &of, &answer).then_some(((kind, of), answer))
        })
        .collect()
}

/// Whether `answer` is the kind of answer a `kind` question about `of` gets, every argument that
/// names an object a full id, and a blob a full id: a line written otherwise was not written by
/// this core, and its file is skipped.
fn well_formed(kind: &str, of: &[String], answer: &Answer) -> bool {
    let id = |text: &String| crate::git::is_object_id(text);
    match (kind, of, answer) {
        ("changed-paths", [parent, commit], Answer::Paths(_)) => id(parent) && id(commit),
        ("tree", [commit], Answer::Tree(tree)) => id(commit) && tree.as_ref().is_none_or(id),
        ("merge-tree", [first, second, _version], Answer::Tree(tree)) => {
            id(first) && id(second) && tree.as_ref().is_none_or(id)
        }
        ("is-ancestor", [ancestor, descendant], Answer::Holds(_)) => id(ancestor) && id(descendant),
        ("read-at", [commit, _path], Answer::Blob(blob)) => {
            id(commit) && blob.as_ref().is_none_or(id)
        }
        _ => false,
    }
}

/// The kinds of question stored.
pub(crate) const KINDS: [&str; 5] = [
    "changed-paths",
    "tree",
    "merge-tree",
    "is-ancestor",
    "read-at",
];

/// Where the content of the blob `id` is stored.
fn blob_file(cache: &Cache, id: &str) -> PathBuf {
    cache.answer(
        "blobs",
        &Sha256Sum::from_digest(&Sha256::digest(id.as_bytes())),
    )
}

/// The content of the blob `id` an earlier run stored; `None` when none is, or it cannot be read.
pub(crate) fn blob(cache: &Cache, id: &str) -> Option<Vec<u8>> {
    std::fs::read(blob_file(cache, id)).ok()
}

/// Stores the content of the blob `id`, unless a run stored it already.
///
/// # Errors
/// Fails when the cache cannot be written.
pub(crate) fn store_blob(cache: &Cache, id: &str, content: &[u8]) -> Result<()> {
    let file = blob_file(cache, id);
    if file.exists() {
        return Ok(());
    }
    let mut temporary = cache.temporary()?;
    temporary.file().write_all(content).map_err(|error| {
        Error::environment(format!(
            "cannot write {}: {error}",
            temporary.path().display()
        ))
    })?;
    cache.insert_answer(
        temporary,
        "blobs",
        &Sha256Sum::from_digest(&Sha256::digest(id.as_bytes())),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_read_back_as_they_were_written() {
        let answers = [
            (
                ("changed-paths", vec!["a".repeat(40), "b".repeat(40)]),
                Answer::Paths(vec!["docs/x.md".to_owned(), "line\nbreak".to_owned()]),
            ),
            (
                ("tree", vec!["c".repeat(40)]),
                Answer::Tree(Some("d".repeat(40))),
            ),
            (
                (
                    "merge-tree",
                    vec![
                        "e".repeat(40),
                        "f".repeat(40),
                        "git version 2.50.1".to_owned(),
                    ],
                ),
                Answer::Tree(None),
            ),
            (
                ("is-ancestor", vec!["1".repeat(40), "2".repeat(40)]),
                Answer::Holds(false),
            ),
            (
                ("read-at", vec!["3".repeat(40), "docs/a b.md".to_owned()]),
                Answer::Blob(Some("4".repeat(40))),
            ),
            (
                ("read-at", vec!["3".repeat(40), "absent".to_owned()]),
                Answer::Blob(None),
            ),
        ];
        let text: Vec<String> = answers
            .iter()
            .map(|(question, answer)| line(question, answer))
            .collect();
        let parsed = parse(text.join("\n").as_bytes()).expect("parses");
        assert_eq!(parsed, answers.to_vec());

        // A line of the wrong shape for its kind makes its file a miss.
        let commit = "5".repeat(40);
        let blob = |id: Value| json!({ "blob": id });
        for (kind, of, answer) in [
            ("read-at", json!([commit, "a.md"]), json!({ "holds": true })),
            ("read-at", json!([commit]), blob(Value::Null)),
            ("read-at", json!([commit, "a.md", "b"]), blob(Value::Null)),
            ("read-at", json!(["HEAD", "a.md"]), blob(Value::Null)),
            ("read-at", json!([commit, "a.md"]), blob(json!("abc"))),
            ("tree", json!([commit]), blob(Value::Null)),
            (
                "is-ancestor",
                json!([commit, commit]),
                json!({ "paths": [] }),
            ),
        ] {
            let wrong = json!({ "kind": kind, "of": of, "answer": answer });
            let file = format!("{}\n{wrong}\n", text[0]);
            assert_eq!(parse(file.as_bytes()), None, "{wrong}");
        }
        assert_eq!(
            parse(b"{\"kind\": \"tree\"}\n"),
            None,
            "a broken line fails its file"
        );
    }
}

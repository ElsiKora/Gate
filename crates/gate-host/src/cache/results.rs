//! The result cache: a plugin call's findings, stored with what the call read, and reused while
//! every recorded input answers as it did (architecture §9).
//!
//! The key is the core version, the plugin's sha256, the call's request (rules, options, norms,
//! project, plugin and language settings) and its grant. The entry holds the findings and the read
//! set, plus what the read set alone cannot say again: for each listed prefix, a digest of the
//! tracked paths below it; for a call that asked for the change selection, a digest of it. A
//! stored result is reused when each listed prefix lists the same paths, each read path has the
//! same blob id (or is still untracked), the change selection is the same, and each git question
//! that goes through a ref, or asks `sync-state`, answers what it was recorded with. A question
//! about commit ids never changes and is not asked again, except an `is-ancestor` no and a
//! `read-at` none, which a fetch of the commit can change.
//!
//! A generator's output is stored the same way, under the core version, the plugin's sha256, the
//! generator, its options and its grant, with the files it wrote beside its findings: it reads
//! only the tree, so the same checks prove it.
//!
//! An entry that cannot be read or does not parse is a miss, never an error: the call runs and
//! stores its result again.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;

use gate_runtime::{GitQuery, Grant, History, ReadSet};
use gate_sdk::{CheckRequest, FileLocation, Finding, GeneratedFile, Location, Span};
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};

use super::{Cache, Sha256Sum};
use crate::error::{Error, Result};

/// Raised when an entry written before would be read differently.
const FORMAT: u32 = 2;

/// A stored call: its findings, its read set, and the digests that check the read set again.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Stored {
    pub(crate) findings: Vec<Finding>,
    /// For a generator, the files it wrote, at repository paths.
    pub(crate) files: Vec<GeneratedFile>,
    pub(crate) read_set: ReadSet,
    /// For each listed prefix, the digest of the tracked paths that start with it.
    listed: BTreeMap<String, String>,
    /// For a call that asked for the change selection, the digest of it.
    changed: Option<String>,
}

/// What a run's snapshot answers about its tracked paths, for checking a stored read set.
pub(crate) struct Inputs<'a> {
    /// Every tracked path, sorted.
    pub(crate) paths: &'a [String],
    /// The blob id of a tracked path.
    pub(crate) blob: &'a dyn Fn(&str) -> Option<String>,
    /// The paths the run's change selection names, when it has one.
    pub(crate) changed: Option<&'a [String]>,
}

/// The key of a call's result.
pub(crate) fn key(plugin_sha256: &str, request: &CheckRequest, grant: &Grant) -> Sha256Sum {
    let mut hasher = Sha256::new();
    hasher.update(format!(
        "gate-result\0{FORMAT}\0{}\0{plugin_sha256}\0{request:?}\0{grant:?}\0",
        env!("CARGO_PKG_VERSION")
    ));
    Sha256Sum::from_digest(&hasher.finalize())
}

impl Stored {
    /// A call's outcome with the digests of `inputs`, ready to store.
    pub(crate) fn new(findings: Vec<Finding>, read_set: ReadSet, inputs: &Inputs<'_>) -> Self {
        let listed = read_set
            .listed
            .iter()
            .map(|prefix| (prefix.clone(), listing(inputs.paths, prefix)))
            .collect();
        let changed = read_set.changed.then(|| selection(inputs.changed));
        Self {
            findings,
            files: Vec::new(),
            read_set,
            listed,
            changed,
        }
    }

    /// A generator's outcome: [`Stored::new`] with the files it wrote.
    pub(crate) fn generated(
        files: Vec<GeneratedFile>,
        findings: Vec<Finding>,
        read_set: ReadSet,
        inputs: &Inputs<'_>,
    ) -> Self {
        Self {
            files,
            ..Self::new(findings, read_set, inputs)
        }
    }

    /// The first recorded input that does not answer now as it did when the call ran, in words;
    /// `None` when every one does, so the stored result holds. A git question that fails now
    /// counts as changed: the call runs again and reports the failure itself.
    pub(crate) fn changed_input(
        &self,
        inputs: &Inputs<'_>,
        history: Option<&dyn History>,
    ) -> Option<String> {
        if self.read_set.listed.len() != self.listed.len() {
            return Some("its listings".to_owned());
        }
        for (prefix, digest) in &self.listed {
            if !self.read_set.listed.contains(prefix) || listing(inputs.paths, prefix) != *digest {
                return Some(format!("the listing of {prefix:?}"));
            }
        }
        for (path, blob) in &self.read_set.read {
            if (inputs.blob)(path) != *blob {
                return Some(format!("the file {path:?}"));
            }
        }
        if self.changed != self.read_set.changed.then(|| selection(inputs.changed)) {
            return Some("the change selection".to_owned());
        }
        for (query, recorded) in &self.read_set.git {
            if query.lasting(recorded) {
                continue;
            }
            let holds = history
                .is_some_and(|history| query.depends_on(history).is_ok_and(|now| now == *recorded));
            if !holds {
                return Some(format!("git {}", query.to_words().join(" ")));
            }
        }
        None
    }
}

/// The digest of the tracked paths that start with `prefix`: every path a listing that covers it
/// could have returned, and possibly more, so a change below it never goes unnoticed.
fn listing(paths: &[String], prefix: &str) -> String {
    let start = paths.partition_point(|path| path.as_str() < prefix);
    let mut hasher = Sha256::new();
    for path in paths[start..]
        .iter()
        .take_while(|path| path.starts_with(prefix))
    {
        hasher.update(path.as_bytes());
        hasher.update([0]);
    }
    super::hex(&hasher.finalize())
}

/// The digest of a run's change selection; `None` (every path) differs from every list.
fn selection(changed: Option<&[String]>) -> String {
    let Some(changed) = changed else {
        return "everything".to_owned();
    };
    let mut hasher = Sha256::new();
    for path in changed {
        hasher.update(path.as_bytes());
        hasher.update([0]);
    }
    super::hex(&hasher.finalize())
}

/// The key of a generator's output: like [`key`], with the generator and its options (JSON) for
/// the request.
pub(crate) fn generate_key(
    plugin_sha256: &str,
    generator: &str,
    options: &str,
    grant: &Grant,
) -> Sha256Sum {
    let mut hasher = Sha256::new();
    hasher.update(format!(
        "gate-generate\0{FORMAT}\0{}\0{plugin_sha256}\0{generator}\0{options}\0{grant:?}\0",
        env!("CARGO_PKG_VERSION")
    ));
    Sha256Sum::from_digest(&hasher.finalize())
}

/// The stored result under `key`; `None` when there is none or it cannot be read back.
pub(crate) fn load(cache: &Cache, key: &Sha256Sum) -> Option<Stored> {
    let bytes = std::fs::read(cache.result(key)).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    decode(&value)
}

/// Stores `stored` under `key`.
///
/// # Errors
/// Fails when the cache cannot be written.
pub(crate) fn store(cache: &Cache, key: &Sha256Sum, stored: &Stored) -> Result<()> {
    let mut temporary = cache.temporary()?;
    let bytes = encode(stored).to_string();
    temporary
        .file()
        .write_all(bytes.as_bytes())
        .map_err(|error| {
            Error::environment(format!(
                "cannot write {}: {error}",
                temporary.path().display()
            ))
        })?;
    cache.insert_result(temporary, key)?;
    Ok(())
}

fn encode(stored: &Stored) -> Value {
    let findings: Vec<Value> = stored.findings.iter().map(encode_finding).collect();
    let read: Map<String, Value> = stored
        .read_set
        .read
        .iter()
        .map(|(path, blob)| (path.clone(), json!(blob)))
        .collect();
    let git: Vec<Value> = stored
        .read_set
        .git
        .iter()
        .map(|(query, recorded)| json!({ "query": query.to_words(), "depends-on": recorded }))
        .collect();
    let files: Vec<Value> = stored
        .files
        .iter()
        .map(|file| match std::str::from_utf8(&file.contents) {
            Ok(text) => json!({ "path": file.path, "text": text }),
            Err(_) => json!({ "path": file.path, "bytes": file.contents }),
        })
        .collect();
    json!({
        "format": FORMAT,
        "findings": findings,
        "files": files,
        "listed": stored.listed,
        "read": read,
        "changed": stored.changed,
        "git": git,
    })
}

fn decode(value: &Value) -> Option<Stored> {
    if value.get("format")?.as_u64()? != u64::from(FORMAT) {
        return None;
    }
    let findings = value
        .get("findings")?
        .as_array()?
        .iter()
        .map(decode_finding)
        .collect::<Option<Vec<_>>>()?;
    let files = value
        .get("files")?
        .as_array()?
        .iter()
        .map(decode_file)
        .collect::<Option<Vec<_>>>()?;
    let listed: BTreeMap<String, String> = value
        .get("listed")?
        .as_object()?
        .iter()
        .map(|(prefix, digest)| Some((prefix.clone(), digest.as_str()?.to_owned())))
        .collect::<Option<_>>()?;
    let read = value
        .get("read")?
        .as_object()?
        .iter()
        .map(|(path, blob)| Some((path.clone(), optional(blob)?)))
        .collect::<Option<BTreeMap<_, _>>>()?;
    let changed = optional(value.get("changed")?)?;
    let git = value
        .get("git")?
        .as_array()?
        .iter()
        .map(|entry| {
            let query = GitQuery::from_words(&strings(entry.get("query")?)?)?;
            Some((query, strings(entry.get("depends-on")?)?))
        })
        .collect::<Option<BTreeMap<_, _>>>()?;
    let read_set = ReadSet {
        listed: listed.keys().cloned().collect::<BTreeSet<_>>(),
        read,
        changed: changed.is_some(),
        git,
    };
    Some(Stored {
        findings,
        files,
        read_set,
        listed,
        changed,
    })
}

fn decode_file(value: &Value) -> Option<GeneratedFile> {
    let path = value.get("path")?.as_str()?.to_owned();
    let contents = match (value.get("text"), value.get("bytes")) {
        (Some(text), None) => text.as_str()?.as_bytes().to_vec(),
        (None, Some(bytes)) => bytes
            .as_array()?
            .iter()
            .map(|byte| u8::try_from(byte.as_u64()?).ok())
            .collect::<Option<_>>()?,
        _ => return None,
    };
    Some(GeneratedFile { path, contents })
}

fn encode_finding(finding: &Finding) -> Value {
    let location = match &finding.location {
        None => Value::Null,
        Some(Location::Commit(commit)) => json!({ "commit": commit }),
        Some(Location::File(file)) => json!({
            "path": file.path,
            "span": file.span.map(|span| [span.line, span.column, span.end_line, span.end_column]),
        }),
    };
    json!({
        "rule": finding.rule,
        "aspect": finding.aspect,
        "message": finding.message,
        "location": location,
        "target": finding.target,
        "help": finding.help,
        "key": finding.key,
    })
}

fn decode_finding(value: &Value) -> Option<Finding> {
    let text = |key: &str| Some(value.get(key)?.as_str()?.to_owned());
    let location = match value.get("location")? {
        Value::Null => None,
        location => Some(if let Some(commit) = location.get("commit") {
            Location::Commit(commit.as_str()?.to_owned())
        } else {
            let span = match location.get("span")? {
                Value::Null => None,
                span => {
                    let numbers = span
                        .as_array()?
                        .iter()
                        .map(|number| u32::try_from(number.as_u64()?).ok())
                        .collect::<Option<Vec<_>>>()?;
                    let [line, column, end_line, end_column] = numbers[..] else {
                        return None;
                    };
                    Some(Span {
                        line,
                        column,
                        end_line,
                        end_column,
                    })
                }
            };
            Location::File(FileLocation {
                path: location.get("path")?.as_str()?.to_owned(),
                span,
            })
        }),
    };
    Some(Finding {
        rule: text("rule")?,
        aspect: text("aspect")?,
        message: text("message")?,
        location,
        target: optional(value.get("target")?)?,
        help: optional(value.get("help")?)?,
        key: optional(value.get("key")?)?,
    })
}

/// A JSON string or null; `None` for anything else.
#[expect(
    clippy::option_option,
    reason = "a stored optional string: null, a string, or not one, which fails the entry"
)]
fn optional(value: &Value) -> Option<Option<String>> {
    match value {
        Value::Null => Some(None),
        Value::String(text) => Some(Some(text.clone())),
        _ => None,
    }
}

fn strings(value: &Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|item| Some(item.as_str()?.to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(location: Option<Location>) -> Finding {
        Finding {
            rule: "rule".to_owned(),
            aspect: String::new(),
            message: "a message".to_owned(),
            location,
            target: Some("t".to_owned()),
            help: None,
            key: None,
        }
    }

    #[test]
    fn an_entry_reads_back_as_it_was_written() {
        let mut read_set = ReadSet::default();
        read_set.listed.insert("apps/a/".to_owned());
        read_set
            .read
            .insert("apps/a/x.ts".to_owned(), Some("b1".to_owned()));
        read_set.read.insert("apps/a/missing".to_owned(), None);
        read_set.changed = true;
        read_set.git.insert(GitQuery::Head, vec!["c".repeat(40)]);
        let paths = vec!["apps/a/x.ts".to_owned(), "apps/b/y.ts".to_owned()];
        let inputs = Inputs {
            paths: &paths,
            blob: &|_| None,
            changed: None,
        };
        let stored = Stored::new(
            vec![
                finding(None),
                finding(Some(Location::Commit("c".repeat(40)))),
                finding(Some(Location::file("apps/a/x.ts"))),
                finding(Some(Location::span(
                    "apps/a/x.ts",
                    Span {
                        line: 1,
                        column: 2,
                        end_line: 3,
                        end_column: 4,
                    },
                ))),
                Finding {
                    key: Some("Thing.one".to_owned()),
                    ..finding(Some(Location::file("apps/a/x.ts")))
                },
            ],
            read_set,
            &inputs,
        );
        assert_eq!(decode(&encode(&stored)), Some(stored.clone()));

        let generated = Stored::generated(
            vec![
                GeneratedFile {
                    path: "apps/a/gen.md".to_owned(),
                    contents: b"text\n".to_vec(),
                },
                GeneratedFile {
                    path: "apps/a/gen.bin".to_owned(),
                    contents: vec![0, 0xff, 0xfe],
                },
            ],
            stored.findings.clone(),
            stored.read_set.clone(),
            &inputs,
        );
        assert_eq!(decode(&encode(&generated)), Some(generated));
    }

    #[test]
    fn a_listing_digest_covers_exactly_the_paths_below_its_prefix() {
        let paths: Vec<String> = ["a/1", "a/2", "ab/3", "b/4"]
            .iter()
            .map(|path| (*path).to_owned())
            .collect();
        let fewer: Vec<String> = ["a/1", "a/2", "b/4"]
            .iter()
            .map(|path| (*path).to_owned())
            .collect();
        let other: Vec<String> = ["a/1", "a/2", "ab/3", "b/5"]
            .iter()
            .map(|path| (*path).to_owned())
            .collect();
        assert_eq!(listing(&paths, "a/"), listing(&fewer, "a/"));
        assert_ne!(listing(&paths, "a"), listing(&fewer, "a"));
        assert_eq!(listing(&paths, "a/"), listing(&other, "a/"));
        assert_ne!(listing(&paths, ""), listing(&other, ""));
    }
}

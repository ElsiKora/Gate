//! Ledger documents and collections: an entry file and the Markdown files of its same-stem owner
//! directory, read through the run's tree, and the findings a rule collects while it reads them.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gate_sdk::{Location, Report, Tree};
use regex::{Captures, Regex};

use crate::text::{LineIndex, blank_fenced, extension};

/// Where a finding points: a path of the tree, optionally with a line.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct At {
    pub path: String,
    pub line: Option<u32>,
}

impl At {
    /// A whole file.
    #[must_use]
    pub fn file(path: &str) -> Self {
        Self {
            path: path.to_owned(),
            line: None,
        }
    }

    /// One line of a file.
    #[must_use]
    pub fn line(path: &str, line: u32) -> Self {
        Self {
            path: path.to_owned(),
            line: Some(line),
        }
    }

    /// The same file, `lines` further down.
    #[must_use]
    pub fn below(&self, lines: u32) -> Self {
        Self {
            path: self.path.clone(),
            line: self.line.map(|line| line.saturating_add(lines)),
        }
    }

    fn location(&self) -> Location {
        match self.line {
            None => Location::file(self.path.clone()),
            Some(line) => Location::line(
                self.path.clone(),
                usize::try_from(line).unwrap_or(usize::MAX),
            ),
        }
    }
}

/// One collected finding: its aspect, location, message and optional stable key.
#[derive(Debug)]
struct Collected {
    aspect: String,
    at: At,
    message: String,
    key: Option<String>,
}

/// The findings of one rule run, kept in order without repeats.
#[derive(Debug, Default)]
pub struct Findings {
    seen: HashSet<(String, At, String)>,
    findings: Vec<Collected>,
}

impl Findings {
    /// Adds one finding of `aspect`, once.
    pub fn emit(&mut self, aspect: &str, at: &At, message: String) {
        self.collect(aspect, at, None, message);
    }

    /// Adds one finding of `aspect` with a stable `key` (what the finding is about, never a line or
    /// the message), once.
    pub fn emit_keyed(&mut self, aspect: &str, at: &At, key: String, message: String) {
        self.collect(aspect, at, Some(key), message);
    }

    fn collect(&mut self, aspect: &str, at: &At, key: Option<String>, message: String) {
        let finding = (aspect.to_owned(), at.clone(), message);

        if self.seen.insert(finding.clone()) {
            let (aspect, at, message) = finding;
            self.findings.push(Collected {
                aspect,
                at,
                message,
                key,
            });
        }
    }

    /// Writes the findings whose aspect `keep` accepts to `report`, under `aspect` as `rename` maps
    /// it.
    pub fn write(
        self,
        report: &mut Report,
        keep: impl Fn(&str) -> bool,
        rename: impl Fn(&str) -> &str,
    ) {
        for Collected {
            aspect,
            at,
            message,
            key,
        } in self.findings
        {
            if keep(&aspect) {
                let finding = report.finding(rename(&aspect), at.location(), message);

                if let Some(key) = key {
                    finding.key(key);
                }
            }
        }
    }

    /// Writes every finding to `report` under its own aspect.
    pub fn write_all(self, report: &mut Report) {
        self.write(report, |_| true, |aspect| aspect);
    }
}

/// One ledger file with its fenced code blanked out, offsets preserved.
#[derive(Debug)]
pub struct Document {
    pub path: String,
    pub content: String,
    pub parseable: String,
    lines: LineIndex,
}

impl Document {
    fn new(path: &str, content: String) -> Self {
        let parseable = blank_fenced(&content, false);
        let lines = LineIndex::new(&content);
        Self {
            path: path.to_owned(),
            content,
            parseable,
            lines,
        }
    }

    /// The location of a byte offset.
    #[must_use]
    pub fn locate(&self, offset: usize) -> At {
        At::line(&self.path, self.lines.line(offset))
    }
}

/// One regular-expression match in one document.
#[derive(Clone, Debug)]
pub struct Match {
    pub document: Rc<Document>,
    pub start: usize,
    pub groups: Vec<Option<String>>,
}

impl Match {
    fn new(document: &Rc<Document>, captures: &Captures<'_>) -> Self {
        Self {
            document: Rc::clone(document),
            start: captures.get(0).map_or(0, |whole| whole.start()),
            groups: captures
                .iter()
                .map(|group| group.map(|group| group.as_str().to_owned()))
                .collect(),
        }
    }

    /// Capture group `index`, or the empty string.
    #[must_use]
    pub fn group(&self, index: usize) -> &str {
        self.groups
            .get(index)
            .and_then(Option::as_deref)
            .unwrap_or("")
    }

    /// Where the match starts.
    #[must_use]
    pub fn locate(&self) -> At {
        self.document.locate(self.start)
    }

    /// The identity of the match: its document and offset.
    #[must_use]
    pub fn key(&self) -> (String, usize) {
        (self.document.path.clone(), self.start)
    }
}

/// Lazily read ledger documents and collections of one tree.
pub struct Catalog<'h> {
    tree: Tree<'h>,
    documents: RefCell<HashMap<String, Option<Rc<Document>>>>,
}

impl std::fmt::Debug for Catalog<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Catalog").finish_non_exhaustive()
    }
}

impl<'h> Catalog<'h> {
    /// A catalog over `tree`.
    #[must_use]
    pub fn new(tree: Tree<'h>) -> Self {
        Self {
            tree,
            documents: RefCell::new(HashMap::new()),
        }
    }

    /// The tree the catalog reads.
    #[must_use]
    pub fn tree(&self) -> Tree<'h> {
        self.tree
    }

    /// One tracked file, read as lossy UTF-8; `None` when it is not tracked.
    #[must_use]
    pub fn document(&self, path: &str) -> Option<Rc<Document>> {
        if let Some(cached) = self.documents.borrow().get(path) {
            return cached.clone();
        }

        let document = self
            .tree
            .text(path)
            .map(|content| Rc::new(Document::new(path, content)));
        self.documents
            .borrow_mut()
            .insert(path.to_owned(), document.clone());
        document
    }

    /// The text of one tracked file, or the empty string when it is not tracked.
    #[must_use]
    pub fn read(&self, path: &str) -> String {
        self.document(path)
            .map(|document| document.content.clone())
            .unwrap_or_default()
    }

    /// The tracked Markdown files below `directory`, in path order.
    #[must_use]
    pub fn owner_files(&self, directory: &str) -> Vec<String> {
        self.tree
            .under(directory)
            .into_iter()
            .filter(|path| extension(path) == ".md")
            .collect()
    }

    /// The entry file followed by the Markdown files of its same-stem owner directory.
    #[must_use]
    pub fn collection(&self, entry: &str) -> Vec<Rc<Document>> {
        let owner = entry.strip_suffix(extension(entry)).unwrap_or(entry);
        let mut paths = vec![entry.to_owned()];
        paths.extend(self.owner_files(owner));
        self.read_all(&paths)
    }

    fn read_all(&self, paths: &[String]) -> Vec<Rc<Document>> {
        paths
            .iter()
            .filter_map(|path| self.document(path))
            .collect()
    }

    /// Every match of `pattern` in the parseable text of the collection of `entry`.
    #[must_use]
    pub fn matches(&self, entry: &str, pattern: &Regex) -> Vec<Match> {
        find_all(&self.collection(entry), pattern)
    }

    /// Every match of `pattern` in the Markdown files below `directory`.
    #[must_use]
    pub fn directory_matches(&self, directory: &str, pattern: &Regex) -> Vec<Match> {
        find_all(&self.read_all(&self.owner_files(directory)), pattern)
    }
}

fn find_all(documents: &[Rc<Document>], pattern: &Regex) -> Vec<Match> {
    documents
        .iter()
        .flat_map(|document| {
            pattern
                .captures_iter(&document.parseable)
                .map(|captures| Match::new(document, &captures))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The original text of each record, from its match to the next `boundary` line; the first
/// definition of an identifier wins.
#[must_use]
pub fn sections(records: &[Match], boundary: &Regex) -> HashMap<String, String> {
    let mut result = HashMap::new();

    for record in records {
        let id = record.group(1).to_owned();

        if result.contains_key(&id) {
            continue;
        }

        let Document {
            content, parseable, ..
        } = record.document.as_ref();
        let end = match content[record.start..].find('\n') {
            None => content.len(),
            Some(relative) => {
                let next_line = record.start + relative + 1;
                boundary
                    .find(&parseable[next_line..])
                    .map_or(content.len(), |found| next_line + found.start())
            }
        };
        result.insert(id, content[record.start..end].to_owned());
    }

    result
}

/// The location of the first match of every identifier (capture group 1).
#[must_use]
pub fn first_locations(matches: &[Match]) -> HashMap<String, At> {
    let mut locations = HashMap::new();

    for found in matches {
        locations
            .entry(found.group(1).to_owned())
            .or_insert_with(|| found.locate());
    }

    locations
}

//! `docs/layout`: the project's `docs/` follows the standard of its kind. It reads the tracked
//! paths under `docs/` and the text of the indexes; never inside a delegated entry.

use std::collections::{BTreeMap, BTreeSet};

use gate_sdk::{Location, NoOptions, Report, Result, Rule, Run, Tree};

use crate::page;
use crate::settings::{Extra, Settings};
use crate::standard::{self, Cell, ENTRIES, INDEXES, Kind, SEED_MARKER};

/// The rule.
#[derive(Debug, Default)]
pub struct Layout;

impl Rule for Layout {
    type Options = NoOptions;

    const NAME: &'static str = "layout";
    const TITLE: &'static str =
        "The project's docs/ follows the documentation standard of its kind";
    const ASPECTS: &'static [&'static str] = &[
        "missing",
        "unknown",
        "extra-missing",
        "delegated-missing",
        "entry-index",
        "index",
        "sections",
        "unfilled",
    ];

    fn check(&self, run: &mut Run<'_, NoOptions>) -> Result<()> {
        let settings = Settings::read(run.plugin_options())?;
        let tree = run.tree();
        let book = Book::read(tree);
        let report = run.report();
        missing(report, &settings, &book);
        unknown(report, &settings, &book);
        extras(report, &settings, &book);
        delegated(report, &settings, &book);
        entry_indexes(report, &settings, &book);
        index(report, &settings, &book, tree);
        sections(report, settings.kind, &book, tree);
        unfilled(report, settings.kind, tree);
        Ok(())
    }
}

/// One top-level name under `docs/`: a tracked file of that name, tracked files below it, or both.
#[derive(Debug, Default)]
struct Top {
    file: bool,
    children: BTreeSet<String>,
}

impl Top {
    fn directory(&self) -> bool {
        !self.children.is_empty()
    }

    fn has_index(&self, name: &str) -> bool {
        self.children.contains(&format!("docs/{name}/README.md"))
    }
}

/// The top-level names under `docs/`.
struct Book {
    tops: BTreeMap<String, Top>,
}

impl Book {
    fn read(tree: Tree<'_>) -> Self {
        let mut tops: BTreeMap<String, Top> = BTreeMap::new();
        for path in tree.under("docs") {
            let Some(rest) = path.strip_prefix("docs/") else {
                continue;
            };
            if let Some((name, _)) = rest.split_once('/') {
                tops.entry(name.to_owned())
                    .or_default()
                    .children
                    .insert(path.clone());
            } else {
                tops.entry(rest.to_owned()).or_default().file = true;
            }
        }
        Self { tops }
    }

    /// Whether the index of the entry `name` (a directory's `README.md`, or the file) is tracked.
    fn has(&self, name: &str, directory: bool) -> bool {
        self.tops.get(name).is_some_and(|top| {
            if directory {
                top.has_index(name)
            } else {
                top.file
            }
        })
    }

    /// Whether `name` is tracked in the form given: as a directory, or as a file.
    fn present(&self, name: &str, directory: bool) -> bool {
        self.tops
            .get(name)
            .is_some_and(|top| if directory { top.directory() } else { top.file })
    }
}

fn shown(name: &str, directory: bool) -> String {
    if directory {
        format!("docs/{name}/")
    } else {
        format!("docs/{name}")
    }
}

fn missing(report: &mut Report, settings: &Settings, book: &Book) {
    let kind = settings.kind;
    for entry in ENTRIES.iter().filter(|entry| entry.required(kind)) {
        if book.has(entry.name, entry.directory) {
            continue;
        }
        let message = if entry.directory {
            format!(
                "{} is a required entry of {} and has no README.md; run gate sync to lay its index down, then fill it",
                entry.path(),
                kind.noun()
            )
        } else {
            format!(
                "{} is a required entry of {}; run gate sync to lay it down, then fill it",
                entry.path(),
                kind.noun()
            )
        };
        report
            .finding("missing", Location::file(entry.index()), message)
            .target(entry.path())
            .key(entry.name)
            .help(format!("{}: {}", entry.path(), entry.job));
    }
}

/// Why the top-level `name` is not an entry of the project, if it is not.
fn not_an_entry(name: &str, top: &Top, settings: &Settings) -> Option<String> {
    let kind = settings.kind;
    let directory = top.directory();
    let path = shown(name, directory);
    if let Some(entry) = standard::entry(name) {
        return match entry.cell(kind) {
            Cell::Absent(reason) => Some(format!(
                "{path} is not an entry of {}: {reason}",
                kind.noun()
            )),
            Cell::Same if entry.directory && top.file => Some(format!(
                "docs/{name} is a file; the entry is the directory {}, with its README.md",
                entry.path()
            )),
            Cell::Same if !entry.directory && directory => Some(format!(
                "{path} is a directory; the entry is the Markdown file {}",
                entry.path()
            )),
            Cell::Same => None,
        };
    }
    if let Some(extra) = settings.extra(name) {
        return (extra.directory() != directory || (extra.directory() && top.file)).then(|| {
            format!(
                "{path} is declared as {}; track it in that form",
                shown(name, extra.directory())
            )
        });
    }
    if let Some(reserved) = standard::reserved(name) {
        return Some(format!(
            "{path} is reserved: its content goes to {}",
            reserved.goes_to
        ));
    }
    if let Some(entry) = name
        .strip_suffix(".md")
        .and_then(standard::entry)
        .filter(|entry| entry.directory && entry.in_kind(kind))
    {
        return Some(format!(
            "{path} is not an entry: the entry is {}, with its README.md",
            entry.path()
        ));
    }
    Some(format!(
        "{path} is not an entry of {}; move its content into an entry of the standard, or declare it in the [plugins.docs] options' extra, with its job",
        kind.noun()
    ))
}

fn unknown(report: &mut Report, settings: &Settings, book: &Book) {
    for (name, top) in &book.tops {
        if let Some(message) = not_an_entry(name, top, settings) {
            report
                .finding("unknown", Location::file(format!("docs/{name}")), message)
                .target(shown(name, top.directory()))
                .key(name.clone());
        }
    }
}

fn extras(report: &mut Report, settings: &Settings, book: &Book) {
    for extra in &settings.extra {
        let directory = extra.directory();
        if book.present(&extra.name, directory) {
            continue;
        }
        let path = shown(&extra.name, directory);
        report
            .finding(
                "extra-missing",
                Location::file(format!("docs/{}", extra.name)),
                format!(
                    "{path} is declared in the [plugins.docs] options' extra and is not tracked; add it, or remove the declaration"
                ),
            )
            .target(path)
            .key(extra.name.clone());
    }
}

fn delegated(report: &mut Report, settings: &Settings, book: &Book) {
    for entry in ENTRIES
        .iter()
        .filter(|entry| settings.delegates(entry.name))
    {
        if book.present(entry.name, true) {
            continue;
        }
        let owner = entry.delegated.unwrap_or(entry.name);
        report
            .finding(
                "delegated-missing",
                Location::file(format!("docs/{}", entry.name)),
                format!(
                    "{} is listed in the [plugins.docs] options' delegated and is not tracked; run gate sync to let the {owner} plugin lay it down, or remove it from delegated",
                    entry.path()
                ),
            )
            .target(entry.path())
            .key(entry.name);
    }
}

fn entry_indexes(report: &mut Report, settings: &Settings, book: &Book) {
    let kind = settings.kind;
    for (name, top) in &book.tops {
        if !top.directory() || top.has_index(name) {
            continue;
        }
        let optional = standard::entry(name)
            .is_some_and(|entry| entry.optional && entry.directory && entry.in_kind(kind));
        let declared = settings.extra(name).is_some_and(Extra::directory);
        if !optional && !declared {
            continue;
        }
        report
            .finding(
                "entry-index",
                Location::file(format!("docs/{name}/README.md")),
                format!(
                    "docs/{name}/ has no README.md, the index every entry has; add one that links its pages"
                ),
            )
            .target(format!("docs/{name}/"))
            .key(name.clone());
    }
}

/// The rank of `name` in the index's order: the standard's entries, then the extensions as
/// declared; `None` for a name that is not an entry of the project in the form tracked. A delegated
/// entry the options list has its rank whether or not it is tracked.
fn rank(name: &str, settings: &Settings, book: &Book) -> Option<usize> {
    if !settings.delegates(name) {
        let top = book.tops.get(name)?;
        if not_an_entry(name, top, settings).is_some() || name == "README.md" {
            return None;
        }
    }
    ENTRIES
        .iter()
        .position(|entry| entry.name == name)
        .or_else(|| {
            settings
                .extra
                .iter()
                .position(|extra| extra.name == name)
                .map(|position| ENTRIES.len() + position)
        })
}

/// The top-level entry a link of `docs/README.md` names, if it is a local link into `docs/`.
fn linked(destination: &str) -> Option<String> {
    if destination.contains(':') || destination.starts_with(['/', '#']) {
        return None;
    }
    let path = destination.split(['#', '?']).next()?;
    let path = path.trim_start_matches("./");
    let name = path.split('/').next()?;
    (!name.is_empty() && name != "..").then(|| name.to_owned())
}

fn index(report: &mut Report, settings: &Settings, book: &Book, tree: Tree<'_>) {
    const PATH: &str = "docs/README.md";
    let Some(text) = tree.text(PATH) else {
        return;
    };
    if !page::sections(&text)
        .iter()
        .any(|heading| heading.title == "Entries")
    {
        return;
    }
    let mut order: Vec<(String, usize)> = Vec::new();
    for name in page::links_under(&text, "Entries")
        .iter()
        .map(String::as_str)
        .filter_map(linked)
    {
        if let Some(rank) = rank(&name, settings, book)
            && order.iter().all(|(seen, _)| *seen != name)
        {
            order.push((name, rank));
        }
    }
    let candidates: BTreeSet<&str> = book
        .tops
        .keys()
        .chain(&settings.delegated)
        .map(String::as_str)
        .collect();
    for name in candidates {
        if rank(name, settings, book).is_some() && order.iter().all(|(seen, _)| *seen != name) {
            let directory = book.tops.get(name).is_none_or(Top::directory);
            let path = shown(name, directory);
            report
                .finding(
                    "index",
                    Location::file(PATH),
                    format!("{PATH} does not link {path} in its Entries table; add a row for it"),
                )
                .target(path)
                .key(format!("link {name}"));
        }
    }
    for pair in order.windows(2) {
        let ((before, before_rank), (after, after_rank)) = (&pair[0], &pair[1]);
        if after_rank < before_rank {
            report
                .finding(
                    "index",
                    Location::file(PATH),
                    format!(
                        "{PATH} lists {before} before {after}; list the entries in the standard order"
                    ),
                )
                .target(format!("docs/{after}"))
                .key(format!("order {after}"));
        }
    }
}

fn sections(report: &mut Report, kind: Kind, book: &Book, tree: Tree<'_>) {
    for index in INDEXES {
        let Some(entry) = standard::entry(index.entry) else {
            continue;
        };
        if !entry.in_kind(kind) || !book.has(entry.name, entry.directory) {
            continue;
        }
        let path = entry.index();
        let Some(text) = tree.text(&path) else {
            continue;
        };
        let headings = page::sections(&text);
        let required = index.titles(kind);
        let mut absent = false;
        for (position, title) in required.iter().enumerate() {
            if headings.iter().any(|heading| heading.title == *title) {
                continue;
            }
            absent = true;
            let place = position.checked_sub(1).map_or_else(
                || "first".to_owned(),
                |previous| format!("after \"{}\"", required[previous]),
            );
            report
                .finding(
                    "sections",
                    Location::file(&path),
                    format!(
                        "{path} has no section \"{title}\"; in {} it comes {place}",
                        kind.noun()
                    ),
                )
                .target(path.clone())
                .key(format!("{path}#{title}"));
        }
        let found: Vec<&page::Heading> = headings
            .iter()
            .filter(|heading| required.contains(&heading.title.as_str()))
            .collect();
        let titles: Vec<&str> = found.iter().map(|heading| heading.title.as_str()).collect();
        if !absent && titles != required {
            let line = found
                .iter()
                .zip(&required)
                .find(|(heading, title)| heading.title != **title)
                .map_or(1, |(heading, _)| heading.line);
            report
                .finding(
                    "sections",
                    Location::line(&path, line),
                    format!(
                        "{path} has its required sections out of order; in {} they are: {}",
                        kind.noun(),
                        required.join(", ")
                    ),
                )
                .target(path.clone())
                .key(format!("{path}#order"));
        }
    }
}

fn unfilled(report: &mut Report, kind: Kind, tree: Tree<'_>) {
    for (entry, _) in standard::required_indexes(kind) {
        let path = entry.index();
        let Some(text) = tree.text(&path) else {
            continue;
        };
        if let Some(line) = text.lines().position(|line| line.trim() == SEED_MARKER) {
            report
                .finding(
                    "unfilled",
                    Location::line(&path, line + 1),
                    format!(
                        "{path} still holds the seed's marker line; replace the seeded text with the page's own, then delete the line"
                    ),
                )
                .target(path.clone())
                .key(path);
        }
    }
}

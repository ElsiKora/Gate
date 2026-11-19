//! `ledger/reuses`: every open implementation record says which shared mechanisms it reuses, from
//! the catalog of shared mechanisms.

use std::collections::{BTreeMap, BTreeSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Rule, Run, Tree};
use regex::Regex;

use crate::catalog::{At, Catalog, Findings};
use crate::index::DecisionIndex;
use crate::records::{Ledger, Record, compile};
use crate::settings::{Settings, check_path};
use crate::text::{blank_fenced, pattern};

/// The rule.
#[derive(Debug, Default)]
pub struct Reuses;

/// The options of `ledger/reuses`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The catalog of shared mechanisms; empty turns the rule off.
    pub catalog: String,
    /// Directories whose implementation records the rule skips, with or without a trailing `/`.
    pub exclude: Vec<String>,
    /// The least number of words a `none` reason has.
    pub reason_words: usize,
    /// A `none` reason this pattern matches is too generic.
    pub generic_reasons: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            catalog: String::new(),
            exclude: Vec::new(),
            reason_words: 4,
            generic_reasons: r"(?i)^(?:not applicable|n/?a|none|nothing|no reuse|nothing to reuse|no shared mechanism|no catalog entry applies)$".to_owned(),
        }
    }
}

impl Rule for Reuses {
    type Options = Options;

    const NAME: &'static str = "reuses";
    const TITLE: &'static str =
        "Every open implementation record names the shared mechanisms it reuses";
    const ASPECTS: &'static [&'static str] = &[
        "catalog-entry",
        "reuses-missing",
        "reuses-planner",
        "reuses-unknown",
    ];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let settings = Settings::read(run)?;
        let options = run.options();
        let tree = run.tree();
        let generic = pattern(&options.generic_reasons).map_err(|error| {
            Error::config(format!(
                "ledger/reuses option generic-reasons is not a valid pattern: {error}; fix it"
            ))
        })?;
        if !options.catalog.is_empty() {
            check_path("ledger/reuses option catalog", &options.catalog)?;
        }
        for excluded in &options.exclude {
            check_path(
                "ledger/reuses option exclude",
                excluded.strip_suffix('/').unwrap_or(excluded),
            )?;
        }

        if options.catalog.is_empty() || !tree.contains(&settings.implementation) {
            return Ok(());
        }

        let ledger = Ledger::read(&Catalog::new(tree), &settings)?;
        let patterns = Patterns::new(generic)?;
        let mut findings = Findings::default();
        let decisions = DecisionIndex::load(&tree, &settings.decisions)?;
        let scope = Scope {
            path: &options.catalog,
            mechanisms: catalog(
                &CatalogInput {
                    tree: &tree,
                    path: &options.catalog,
                    ledger: &ledger,
                    settings: &settings,
                    decisions: decisions.as_ref(),
                },
                &patterns,
                &mut findings,
            ),
            patterns: &patterns,
            reason_words: options.reason_words,
        };

        for record in &ledger.implementation {
            let open = record
                .status
                .as_deref()
                .is_some_and(|status| settings.is_open(status));
            let excluded = options.exclude.iter().any(|directory| {
                record
                    .location
                    .path
                    .strip_prefix(directory.trim_end_matches('/'))
                    .is_some_and(|rest| rest.starts_with('/'))
            });

            if open && !excluded {
                scope.reuses(record, &mut findings);
            }
        }

        findings.write_all(run.report());
        Ok(())
    }
}

struct Patterns {
    entry: Regex,
    field: Regex,
    backticked: Regex,
    keys: Regex,
    none: Regex,
    generic: Regex,
    reuses: Regex,
    kebab: Regex,
}

impl Patterns {
    fn new(generic: Regex) -> gate_sdk::Result<Self> {
        Ok(Self {
            entry: compile(r"^- `([^`]*)`: (.*)$")?,
            field: compile(r"^  - (Decisions|Path|Planned by): (.*)$")?,
            backticked: compile(r"`([^`]+)`")?,
            keys: compile(r"^`[^`]+`(?:, `[^`]+`)*\.?$")?,
            none: compile(r"^none; (.+?)\.?$")?,
            generic,
            reuses: compile(r"^  - Reuses: ?(.*)$")?,
            kebab: compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")?,
        })
    }
}

/// One catalog entry while it is read.
struct Entry {
    key: String,
    line: u32,
    fields: BTreeMap<String, Vec<String>>,
}

/// What the catalog is checked against.
struct CatalogInput<'i, 'h> {
    tree: &'i Tree<'h>,
    path: &'i str,
    ledger: &'i Ledger,
    settings: &'i Settings,
    decisions: Option<&'i DecisionIndex>,
}

/// Reads the catalog, reports its malformed entries and returns each key with the records that
/// plan it, none for a mechanism that has a path.
fn catalog(
    input: &CatalogInput<'_, '_>,
    patterns: &Patterns,
    findings: &mut Findings,
) -> BTreeMap<String, Vec<String>> {
    let path = input.path;
    let Some(text) = input.tree.text(path) else {
        findings.emit(
            "catalog-entry",
            &At::file(path),
            "the catalog of shared mechanisms is missing; the Reuses fields name its keys"
                .to_owned(),
        );
        return BTreeMap::new();
    };
    let mut mechanisms = BTreeMap::new();

    for entry in &entries(&blank_fenced(&text, false), patterns) {
        let mut problems: Vec<String> = Vec::new();
        let mut planners = Vec::new();

        if !patterns.kebab.is_match(&entry.key) {
            problems.push("its key is not lowercase kebab-case".to_owned());
        }

        match single(&entry.fields, "Decisions") {
            Some(value) => {
                let named = listed(value, patterns);

                if named.is_empty() {
                    problems.push("its Decisions name no decision".to_owned());
                }

                for decision in named {
                    if !input
                        .decisions
                        .is_none_or(|index| index.is_accepted(&decision))
                    {
                        problems.push(format!("{decision} is not an accepted decision"));
                    }
                }
            }
            None => problems.push("it needs exactly one Decisions field".to_owned()),
        }

        match (
            single(&entry.fields, "Path"),
            single(&entry.fields, "Planned by"),
        ) {
            (Some(value), None) => {
                let paths = listed(value, patterns);

                if paths.is_empty() {
                    problems.push("its Path names no path".to_owned());
                }

                for path in paths {
                    if !tracked(input.tree, &path) {
                        problems.push(format!("{path} is not tracked"));
                    }
                }
            }
            (None, Some(value)) => {
                planners = listed(value, patterns);

                if planners.is_empty() {
                    problems.push("its Planned by names no record".to_owned());
                }

                for record in &planners {
                    if !record.starts_with("I-")
                        || !input
                            .ledger
                            .status(record)
                            .is_some_and(|status| input.settings.is_open(status))
                    {
                        problems.push(format!(
                            "{record} is no planned or in-progress implementation record; once the mechanism lands, name its Path"
                        ));
                    }
                }
            }
            _ => problems.push("it needs exactly one Path or Planned by field".to_owned()),
        }

        if mechanisms.contains_key(&entry.key) {
            problems.push("its key repeats an earlier entry".to_owned());
        } else {
            mechanisms.insert(entry.key.clone(), planners);
        }

        if !problems.is_empty() {
            findings.emit(
                "catalog-entry",
                &At::line(path, entry.line),
                format!(
                    "the shared mechanism {}: {}",
                    entry.key,
                    problems.join("; ")
                ),
            );
        }
    }

    mechanisms
}

/// The catalog entries with the fields that follow each directly, outside fenced code.
fn entries(text: &str, patterns: &Patterns) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    // Whether the lines so far continue the entry above: its fields follow it directly.
    let mut open = false;

    for (index, line) in text.lines().enumerate() {
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);

        if let Some(captures) = patterns.entry.captures(line) {
            entries.push(Entry {
                key: captures[1].to_owned(),
                line: number,
                fields: BTreeMap::new(),
            });
            open = true;
        } else if let Some(captures) = patterns.field.captures(line)
            && open
            && let Some(entry) = entries.last_mut()
        {
            entry
                .fields
                .entry(captures[1].to_owned())
                .or_default()
                .push(captures[2].to_owned());
        } else if !line.trim().is_empty() {
            open = false;
        }
    }

    entries
}

/// The only value of a field, `None` when it is missing or repeated.
fn single<'f>(fields: &'f BTreeMap<String, Vec<String>>, name: &str) -> Option<&'f str> {
    match fields.get(name).map(Vec::as_slice) {
        Some([value]) => Some(value),
        _ => None,
    }
}

fn listed(value: &str, patterns: &Patterns) -> Vec<String> {
    patterns
        .backticked
        .captures_iter(value)
        .map(|captures| captures[1].to_owned())
        .collect()
}

/// Whether a path is a tracked file or, ending with `/`, a directory with a tracked file.
fn tracked(tree: &Tree<'_>, path: &str) -> bool {
    if path.ends_with('/') {
        tree.paths().iter().any(|tracked| tracked.starts_with(path))
    } else {
        tree.contains(path)
    }
}

/// The catalog the `Reuses` fields are read against.
struct Scope<'s> {
    path: &'s str,
    /// Every catalog key with the records that plan it.
    mechanisms: BTreeMap<String, Vec<String>>,
    patterns: &'s Patterns,
    reason_words: usize,
}

impl Scope<'_> {
    /// The one `Reuses` field of a record with its location, reporting a missing or repeated one.
    fn field<'r>(
        &self,
        record: &'r Record,
        prefix: &str,
        findings: &mut Findings,
    ) -> Option<(At, &'r str)> {
        let found: Vec<(usize, &str)> = record
            .section
            .lines()
            .enumerate()
            .filter_map(|(index, line)| {
                self.patterns
                    .reuses
                    .captures(line)
                    .and_then(|captures| captures.get(1))
                    .map(|value| (index, value.as_str().trim()))
            })
            .collect();
        let at = |index: usize| record.location.below(u32::try_from(index).unwrap_or(0));

        match found.as_slice() {
            [] => {
                findings.emit(
                    "reuses-missing",
                    &record.location,
                    format!(
                        "{prefix} is open but has no Reuses field; name the catalog keys of {} it reuses, or write `none; <why no catalog entry applies>`",
                        self.path
                    ),
                );
                None
            }
            [(index, value)] => Some((at(*index), *value)),
            [_, (index, _), ..] => {
                findings.emit(
                    "reuses-missing",
                    &at(*index),
                    format!("{prefix} repeats its Reuses field; keep one"),
                );
                None
            }
        }
    }

    /// Checks the `Reuses` field of one open record.
    fn reuses(&self, record: &Record, findings: &mut Findings) {
        let catalog = self.path;
        let words = self.reason_words;
        let prefix = format!("{} {}", record.location.path, record.id);
        let Some((location, value)) = self.field(record, &prefix, findings) else {
            return;
        };
        let mut named = BTreeSet::new();

        if let Some(captures) = self.patterns.none.captures(value) {
            let reason = captures[1].trim();

            if reason.split_whitespace().count() < words || self.patterns.generic.is_match(reason) {
                findings.emit(
                    "reuses-missing",
                    &location,
                    format!(
                        "{prefix} gives no specific reason for reusing nothing; say why no entry of {catalog} applies, in at least {words} words"
                    ),
                );
            }
        } else if self.patterns.keys.is_match(value) {
            for key in listed(value, self.patterns) {
                if named.contains(&key) {
                    findings.emit(
                        "reuses-missing",
                        &location,
                        format!("{prefix} names {key} twice in its Reuses field"),
                    );
                } else if !self.mechanisms.contains_key(&key) {
                    findings.emit(
                        "reuses-unknown",
                        &location,
                        format!("{prefix} reuses {key}, which {catalog} does not list"),
                    );
                }

                named.insert(key);
            }
        } else {
            findings.emit(
                "reuses-missing",
                &location,
                format!(
                    "{prefix} has the Reuses value {value:?}; write backticked catalog keys separated by commas, or `none; <reason>`"
                ),
            );
            return;
        }

        for (key, planners) in &self.mechanisms {
            if !planners.contains(&record.id) {
                continue;
            }

            if planners.len() == 1 && named.contains(key) {
                findings.emit(
                    "reuses-planner",
                    &location,
                    format!(
                        "{prefix} names {key}, which it alone plans in {catalog}; it builds the mechanism, so drop the key"
                    ),
                );
            } else if planners.len() > 1 && !named.contains(key) {
                findings.emit(
                    "reuses-planner",
                    &location,
                    format!(
                        "{prefix} is one of the records {catalog} names as planning {key}; name the key, since whichever lands first builds it"
                    ),
                );
            }
        }
    }
}

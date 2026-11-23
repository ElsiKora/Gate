//! `ledger/decisions`: ADRs have their sections and linked decisions, and the decision index agrees
//! with the ADR coverage table.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use regex::Regex;

use crate::catalog::{At, Catalog, Findings};
use crate::index::{Line, Table};
use crate::input::{self, INPUT, Input};
use crate::records::compile;
use crate::settings::{Settings, check_path};
use crate::text::{blank_fenced, join};

/// The rule.
#[derive(Debug, Default)]
pub struct Decisions;

/// The options of `ledger/decisions`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The directory the canonical file of a coverage row is relative to.
    pub ledger_root: String,
    /// The directory of the ADRs, `NNNN-title.md`.
    pub adr_directory: String,
    /// The ADR coverage table; while it is not tracked, the rule reports it as `#input`.
    pub coverage: String,
    /// The headings every ADR has, each exactly once, outside fenced code and comments.
    pub adr_sections: Vec<String>,
    /// The statuses a decision row may have.
    pub statuses: Vec<String>,
    /// The file name of the ADR template, which is not an ADR.
    pub adr_template: String,
    /// The start of the ADR line that links the decision identifiers.
    pub linked_ids_prefix: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            ledger_root: "docs/ledger".to_owned(),
            adr_directory: "docs/adr".to_owned(),
            coverage: "docs/adr/decision-coverage.md".to_owned(),
            adr_sections: [
                "## Context",
                "## Decision",
                "## Consequences",
                "## Alternatives Considered",
                "## Verification",
            ]
            .map(str::to_owned)
            .to_vec(),
            statuses: ["accepted", "deferred", "proposed", "rejected", "superseded"]
                .map(str::to_owned)
                .to_vec(),
            adr_template: "0000-template.md".to_owned(),
            linked_ids_prefix: "- Linked ledger IDs:".to_owned(),
        }
    }
}

impl Rule<crate::LedgerCall> for Decisions {
    type Options = Options;

    const NAME: &'static str = "decisions";
    const TITLE: &'static str = "ADRs have their sections and linked decisions, and the decision index agrees with the ADR coverage";
    const ASPECTS: &'static [&'static str] = &[
        "adr-linked-ids",
        "adr-sections",
        "decision-canonical-file",
        "decision-coverage",
        "decision-status",
        INPUT,
        "record-shape",
        "record-unique",
    ];

    fn check(&self, run: &mut Run<'_, Options, crate::LedgerCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
        let settings = Settings::read(run)?;
        let options = run.options();
        let tree = run.tree();
        for (name, path) in [
            ("ledger-root", &options.ledger_root),
            ("adr-directory", &options.adr_directory),
            ("coverage", &options.coverage),
        ] {
            check_path(&format!("ledger/decisions option {name}"), path)?;
        }

        let inputs = [
            Input::shared("decisions", &settings.decisions),
            Input::own("coverage", &options.coverage),
        ];
        if !input::tracked(run.report(), &tree, Self::NAME, &inputs) {
            return Ok(());
        }

        let options = run.options();
        let catalog = shared.catalog();
        let patterns = Patterns::new()?;
        let mut findings = Findings::default();
        let index_path = settings.decisions.as_str();
        let coverage_path = options.coverage.as_str();
        let reader = Reader {
            patterns: &patterns,
            statuses: &options.statuses,
        };
        let index = reader.rows(&catalog.read(index_path), index_path, false, &mut findings);
        let coverage = reader.rows(
            &catalog.read(coverage_path),
            coverage_path,
            true,
            &mut findings,
        );
        let linked = linked_ids(catalog, options, &patterns, &mut findings);
        let mut ids: Vec<&String> = Vec::new();

        for id in index.order.iter().chain(&coverage.order) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }

        for id in ids {
            let (Some(indexed), Some(covered)) = (index.rows.get(id), coverage.rows.get(id)) else {
                let missing = if index.rows.contains_key(id) {
                    coverage_path
                } else {
                    index_path
                };
                findings.emit_keyed(
                    "decision-coverage",
                    &At::file(missing),
                    format!("{id}#missing"),
                    format!("{missing} is missing decision {id}"),
                );
                continue;
            };

            if indexed.status != covered.status || indexed.canonical != covered.canonical {
                findings.emit(
                    "decision-coverage",
                    &covered.location,
                    format!(
                        "{id} has inconsistent status or canonical file in decision index/ADR coverage"
                    ),
                );
            }

            if !tree.contains(&join(&[&options.ledger_root, &covered.canonical])) {
                findings.emit_keyed(
                    "decision-canonical-file",
                    &covered.location,
                    id.clone(),
                    format!(
                        "{id} references missing canonical file {}",
                        covered.canonical
                    ),
                );
            }

            for reference in &covered.references {
                let adr = join(&[&options.adr_directory, reference]);

                match linked.get(&adr) {
                    None => findings.emit_keyed(
                        "decision-coverage",
                        &covered.location,
                        format!("{id}/{reference}#missing-adr"),
                        format!("{id} references missing ADR {adr}"),
                    ),
                    Some(linked) if !linked.contains(id) => findings.emit_keyed(
                        "decision-coverage",
                        &At::file(&adr),
                        format!("{id}#unlinked"),
                        format!("{adr} must link {id}"),
                    ),
                    Some(_) => {}
                }
            }
        }

        findings.write_all(run.report());
        Ok(())
        })
    }
}

/// The decision identifiers each numbered ADR links, after reporting its missing sections.
fn linked_ids(
    catalog: &Catalog<'_>,
    options: &Options,
    patterns: &Patterns,
    findings: &mut Findings,
) -> HashMap<String, BTreeSet<String>> {
    let mut linked: HashMap<String, BTreeSet<String>> = HashMap::new();
    let template = format!("/{}", options.adr_template);

    for path in catalog.tree().under(&options.adr_directory) {
        if !patterns.adr_file.is_match(&path) || path.ends_with(&template) {
            continue;
        }

        let blanked = blank_fenced(&catalog.read(&path), true);
        let lines: Vec<&str> = blanked.split('\n').collect();

        for section in &options.adr_sections {
            if lines.iter().filter(|line| **line == section).count() != 1 {
                findings.emit_keyed(
                    "adr-sections",
                    &At::file(&path),
                    section.clone(),
                    format!("{path} must contain exactly one {section}"),
                );
            }
        }

        let linked_line = lines
            .iter()
            .find(|line| line.starts_with(options.linked_ids_prefix.as_str()))
            .copied()
            .unwrap_or("");
        let ids: BTreeSet<String> = patterns
            .linked_id
            .captures_iter(linked_line)
            .map(|captures| captures[1].to_owned())
            .collect();

        if ids.is_empty() {
            findings.emit_keyed(
                "adr-linked-ids",
                &At::file(&path),
                "linked-ids".to_owned(),
                format!("{path} must link ledger decision IDs"),
            );
        }

        linked.insert(path, ids);
    }

    linked
}

struct Patterns {
    table: Table,
    references: Regex,
    quoted: Regex,
    adr_file: Regex,
    linked_id: Regex,
}

impl Patterns {
    fn new() -> gate_sdk::Result<Self> {
        Ok(Self {
            table: Table::new()?,
            references: compile(r"^`[^`]+`(?:,\s*`[^`]+`)*$")?,
            quoted: compile(r"`([^`]+)`")?,
            adr_file: compile(r"/[0-9]{4}-.+\.md$")?,
            linked_id: compile(r"`(D-[^`]+)`")?,
        })
    }
}

#[derive(Debug)]
struct Row {
    status: String,
    canonical: String,
    location: At,
    references: Vec<String>,
}

/// Rows by identifier, with the first-seen order of the identifiers.
#[derive(Debug, Default)]
struct Rows {
    rows: BTreeMap<String, Row>,
    order: Vec<String>,
}

struct Reader<'r> {
    patterns: &'r Patterns,
    statuses: &'r [String],
}

impl Reader<'_> {
    fn rows(&self, content: &str, path: &str, coverage: bool, findings: &mut Findings) -> Rows {
        let patterns = self.patterns;
        let mut rows = Rows::default();
        let middle = |cell: &str| {
            if coverage {
                patterns.references.is_match(cell)
            } else {
                !cell.is_empty()
            }
        };

        for line in patterns.table.lines(content, middle) {
            let row = match line {
                Line::Row(row) => row,
                Line::Malformed(number, text) => {
                    malformed(findings, &At::line(path, number), path, &text);
                    continue;
                }
            };
            let location = At::line(path, row.line);
            let id = row.id;

            if !self.statuses.contains(&row.status) {
                findings.emit_keyed(
                    "decision-status",
                    &location,
                    id.clone(),
                    format!(
                        "{path} contains invalid decision status {} for {id}",
                        row.status
                    ),
                );
            }

            if rows.rows.contains_key(&id) {
                findings.emit_keyed(
                    "record-unique",
                    &location,
                    id.clone(),
                    format!("{path} contains duplicate decision {id}"),
                );
            } else {
                rows.order.push(id.clone());
            }

            rows.rows.insert(
                id,
                Row {
                    status: row.status,
                    canonical: row.canonical,
                    location,
                    references: patterns
                        .quoted
                        .captures_iter(&row.middle)
                        .map(|captures| captures[1].to_owned())
                        .collect(),
                },
            );
        }

        rows
    }
}

fn malformed(findings: &mut Findings, location: &At, path: &str, line: &str) {
    findings.emit_keyed(
        "record-shape",
        location,
        line.to_owned(),
        format!("{path} contains malformed decision row: {line}"),
    );
}

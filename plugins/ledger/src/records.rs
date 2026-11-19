//! The ledger's records: plan headings, implementation, verification, follow-up and production
//! blocker items, their fields, and the analysis `ledger/records` and `ledger/record-status` report
//! from.
//!
//! A record is a line of its collection with fenced code blanked: `### `I-<prefix>-…`: title` in the
//! plan, ``- `I-…`: title`` and the like in the other collections. It runs to the next heading (plan)
//! or the next top-level list item (others). Its fields are the `  - Name: value` lines (plan:
//! `- **Name:** value`).

use std::collections::{HashMap, HashSet};

use regex::Regex;

use crate::catalog::{At, Catalog, Findings, Match, first_locations, sections};
use crate::settings::{Parity, Settings};
use crate::text::{blank_fenced, pattern, trim};

/// The grammar of a record identifier after its family letter.
pub const ID: &str = "[A-Z0-9]+(?:-[A-Z0-9]+)*";

/// The default `pending-values` of `ledger/records`.
pub const PENDING_VALUES: [&str; 16] = [
    "",
    "later",
    "n/a",
    "none",
    "not available",
    "not provided",
    "not recorded",
    "not run",
    "not yet available",
    "not yet provided",
    "not yet recorded",
    "pending",
    "placeholder",
    "tbd",
    "todo",
    "unknown",
];

/// The default `pending-patterns` of `ledger/records`.
pub const PENDING_PATTERNS: [&str; 4] = [
    r"^(?:pending|not run|not available|not provided|not recorded)(?:\s|$)",
    r"^(?:placeholder|tbd|todo)(?:[\s:]|$)",
    r"^(?:to|will) be (?:available|provided|recorded)(?:\s|$)",
    r"^(?:commit(?: hash| oid)?|evidence|notes?|verification(?: notes)?)[\s:]+pending(?:\s|$)",
];

/// Compiles a pattern of the plugin; the plugin's own patterns always compile.
pub fn compile(source: &str) -> gate_sdk::Result<Regex> {
    pattern(source)
        .map_err(|error| gate_sdk::Error::new(format!("pattern {source} is invalid: {error}")))
}

type Fields = HashMap<String, Vec<String>>;

/// The field lines of a record section, by name, values trimmed, in order.
fn fields(section: &str, field: &Regex) -> Fields {
    let mut fields: Fields = HashMap::new();

    for captures in field.captures_iter(&blank_fenced(section, false)) {
        fields
            .entry(captures[1].to_owned())
            .or_default()
            .push(trim(captures.get(2).map_or("", |value| value.as_str())).to_owned());
    }

    fields
}

/// The record grammar shared by every reader.
struct Grammar {
    implementation: Regex,
    verification: Regex,
    plan: Option<Regex>,
    heading_boundary: Regex,
    item_boundary: Regex,
    item_field: Regex,
    plan_field: Regex,
    leading_backticked: Regex,
    follow_up_status: Regex,
}

impl Grammar {
    fn new(parity: Option<&Parity>) -> gate_sdk::Result<Self> {
        Ok(Self {
            implementation: compile(&format!(r"(?mR)^- `(I-{ID})`: (.*)$"))?,
            verification: compile(&format!(r"(?mR)^- `(V-{ID})`: (.*)$"))?,
            plan: parity
                .map(|parity| {
                    compile(&format!(
                        r"(?mR)^### (I-{}-{ID}): (.*)$",
                        crate::text::literal(&parity.prefix)
                    ))
                })
                .transpose()?,
            heading_boundary: compile(r"(?mR)^### ")?,
            item_boundary: compile(r"(?mR)^- ")?,
            item_field: compile(r"(?mR)^  - ([^:\n]+): ?(.*)$")?,
            plan_field: compile(r"(?mR)^- \*\*([^*:\n]+):\*\* ?(.*)$")?,
            leading_backticked: compile(r"^`([^`]+)`")?,
            follow_up_status: compile(&format!(r"(?mR)^- `(F-{ID})`: Status `([^`]+)`"))?,
        })
    }
}

/// One ledger record as the record rules read it.
#[derive(Debug)]
pub struct Record {
    pub id: String,
    /// The location of its first definition in its collection.
    pub location: At,
    /// The record from its record line to the next record, with fenced code blanked so offsets and
    /// lines match the document.
    pub section: String,
    fields: Fields,
    /// The leading backticked value of the first `Status` field.
    pub status: Option<String>,
}

impl Record {
    /// The first value of a field.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name)?.first().map(String::as_str)
    }
}

/// The records of the ledger, each at its first definition in its collection and in document
/// order: every `I-*` item of the implementation collection, every `V-*` item of the verification
/// collection and, with parity, every plan heading.
#[derive(Debug)]
pub struct Ledger {
    pub implementation: Vec<Record>,
    pub verification: Vec<Record>,
    pub plan: Vec<Record>,
    /// The status of every follow-up record, from its line ``- `F-…`: Status `<status>` ``.
    pub follow_ups: HashMap<String, String>,
}

impl Ledger {
    /// Reads the records of the ledger collections.
    ///
    /// # Errors
    /// Never for a readable tree; a pattern of the plugin that does not compile.
    pub fn read(catalog: &Catalog<'_>, settings: &Settings) -> gate_sdk::Result<Self> {
        let grammar = Grammar::new(settings.parity.as_ref())?;
        let plan = match (&settings.parity, &grammar.plan) {
            (Some(parity), Some(plan)) => read_records(
                &catalog.matches(&parity.plan, plan),
                &grammar.heading_boundary,
                &grammar.plan_field,
                &grammar,
            ),
            _ => Vec::new(),
        };

        Ok(Self {
            implementation: read_records(
                &catalog.matches(&settings.implementation, &grammar.implementation),
                &grammar.item_boundary,
                &grammar.item_field,
                &grammar,
            ),
            verification: read_records(
                &catalog.matches(&settings.verification, &grammar.verification),
                &grammar.item_boundary,
                &grammar.item_field,
                &grammar,
            ),
            plan,
            follow_ups: follow_up_statuses(catalog, settings, &grammar),
        })
    }

    /// Every record of the three collections.
    pub fn all(&self) -> impl Iterator<Item = &Record> {
        self.implementation
            .iter()
            .chain(&self.verification)
            .chain(&self.plan)
    }

    /// The status of an implementation or verification record, by its identifier.
    #[must_use]
    pub fn status(&self, id: &str) -> Option<&str> {
        let records = if id.starts_with("V-") {
            &self.verification
        } else {
            &self.implementation
        };

        records
            .iter()
            .find(|record| record.id == id)
            .and_then(|record| record.status.as_deref())
    }
}

fn follow_up_statuses(
    catalog: &Catalog<'_>,
    settings: &Settings,
    grammar: &Grammar,
) -> HashMap<String, String> {
    catalog
        .matches(&settings.follow_ups, &grammar.follow_up_status)
        .iter()
        .map(|found| (found.group(1).to_owned(), trim(found.group(2)).to_owned()))
        .collect()
}

fn read_records(
    matches: &[Match],
    boundary: &Regex,
    field: &Regex,
    grammar: &Grammar,
) -> Vec<Record> {
    let texts = sections(matches, boundary);
    let mut seen: HashSet<String> = HashSet::new();

    matches
        .iter()
        .filter(|found| seen.insert(found.group(1).to_owned()))
        .map(|found| {
            let id = found.group(1).to_owned();
            let section = blank_fenced(texts.get(&id).map_or("", String::as_str), false);
            let fields = fields(&section, field);
            let status = fields
                .get("Status")
                .and_then(|values| values.first())
                .and_then(|value| grammar.leading_backticked.captures(value))
                .map(|captures| trim(&captures[1]).to_owned())
                .filter(|status| !status.is_empty());

            Record {
                id,
                location: found.locate(),
                section,
                fields,
                status,
            }
        })
        .collect()
}

/// What the analysis reads beyond the shared options.
#[derive(Debug)]
pub struct Tracking<'a> {
    pub settings: &'a Settings,
    /// Values that, normalized, are placeholders rather than evidence.
    pub pending_values: &'a [String],
    /// Patterns that, matching the normalized value, make it a placeholder.
    pub pending_patterns: Vec<Regex>,
    /// The allowed production blocker phases; empty allows any.
    pub blocker_phases: &'a [String],
    /// The statuses a paired record may have (`ledger/record-status`); `None` allows any.
    pub allowed: Option<&'a [String]>,
    /// An unfinished paired record keeps `Commit` `pending` in plan and implementation.
    pub unfinished_commit_pending: bool,
}

/// The patterns of the analysis.
struct Patterns {
    plan: Option<Regex>,
    paired_implementation: Option<Regex>,
    paired_verification: Option<Regex>,
    implementation: Regex,
    verification: Regex,
    follow_up: Regex,
    follow_up_status: Regex,
    blocker: Regex,
    plan_candidate: Option<Regex>,
    paired_implementation_candidate: Option<Regex>,
    paired_verification_candidate: Option<Regex>,
    implementation_candidate: Regex,
    blocker_candidate: Regex,
    heading_boundary: Regex,
    item_boundary: Regex,
    item_field: Regex,
    plan_field: Regex,
    backticked: Regex,
    leading_backticked: Regex,
    full_oid: Regex,
    none: Regex,
    no_dependency: Regex,
    dependency_list: Regex,
    dependency_id: Regex,
    verification_item: Option<Regex>,
    blocker_prerequisites: Regex,
    blocker_prerequisite: Regex,
    link: Regex,
    markup: Regex,
    trailing_punctuation: Regex,
    whitespace: Regex,
}

impl Patterns {
    fn new(parity: Option<&Parity>) -> gate_sdk::Result<Self> {
        let paired = |template: &dyn Fn(&str) -> String| -> gate_sdk::Result<Option<Regex>> {
            parity
                .map(|parity| compile(&template(&crate::text::literal(&parity.prefix))))
                .transpose()
        };

        Ok(Self {
            plan: paired(&|prefix| format!(r"(?mR)^### (I-{prefix}-{ID}): (.*)$"))?,
            paired_implementation: paired(&|prefix| {
                format!(r"(?mR)^- `(I-{prefix}-{ID})`: (.*)$")
            })?,
            paired_verification: paired(&|prefix| format!(r"(?mR)^- `(V-{prefix}-{ID})`: (.*)$"))?,
            implementation: compile(&format!(r"(?mR)^- `(I-{ID})`: (.*)$"))?,
            verification: compile(&format!(r"(?mR)^- `(V-{ID})`: (.*)$"))?,
            follow_up: compile(&format!(r"(?mR)^- `(F-{ID})`: "))?,
            follow_up_status: compile(&format!(r"(?mR)^- `(F-{ID})`: Status `([^`]+)`"))?,
            blocker: compile(&format!(r"(?mR)^- `(PB-{ID})`: (.*)$"))?,
            plan_candidate: paired(&|prefix| format!(r"(?imR)^###[\t ]+`?I-{prefix}[^\n]*$"))?,
            paired_implementation_candidate: paired(&|prefix| {
                format!(r"(?imR)^-[\t ]+`?I-{prefix}[^\n]*$")
            })?,
            paired_verification_candidate: paired(&|prefix| {
                format!(r"(?imR)^-[\t ]+`?V-{prefix}[^\n]*$")
            })?,
            implementation_candidate: compile(r"(?imR)^-[\t ]+`?I-[^\n]*$")?,
            blocker_candidate: compile(r"(?imR)^-[\t ]+`?PB-[^\n]*$")?,
            heading_boundary: compile(r"(?mR)^### ")?,
            item_boundary: compile(r"(?mR)^- ")?,
            item_field: compile(r"(?mR)^  - ([^:\n]+): ?(.*)$")?,
            plan_field: compile(r"(?mR)^- \*\*([^*:\n]+):\*\* ?(.*)$")?,
            backticked: compile(r"^`([^`]+)`\.?$")?,
            leading_backticked: compile(r"^`([^`]+)`")?,
            full_oid: compile(r"(?i)^`[0-9a-f]{40}`\.?$")?,
            none: compile(r"^none\.?$")?,
            no_dependency: compile(r"(?i)^(?:none|no separate dependency)\b")?,
            dependency_list: compile(&format!(r"^`(?:I|F)-{ID}`(?:, `(?:I|F)-{ID}`)*\.?$"))?,
            dependency_id: compile(&format!(r"\b((?:I|F)-{ID})\b"))?,
            verification_item: paired(&|prefix| format!(r"^`(V-{prefix}-{ID})`\.?$"))?,
            blocker_prerequisites: compile(&format!(
                r"^`(?:I|F|V|PB)-{ID}`(?:, `(?:I|F|V|PB)-{ID}`)*\.?$"
            ))?,
            blocker_prerequisite: compile(&format!(r"`((?:I|F|V|PB)-{ID})`"))?,
            link: compile(r"\[([^\]]+)\]\([^)]*\)")?,
            markup: compile(r"[`*_~]")?,
            trailing_punctuation: compile(r"[.;:]+$")?,
            whitespace: compile(r"\s+")?,
        })
    }
}

/// What the record passes learned, for the production blocker pass.
struct Known<'k> {
    verification: &'k HashSet<String>,
    implementation_statuses: &'k HashMap<String, String>,
    follow_up_statuses: &'k HashMap<String, String>,
    verification_statuses: &'k HashMap<String, String>,
    verification_notes: &'k HashMap<String, String>,
}

/// The parsed production blockers.
#[derive(Clone, Copy)]
struct Blockers<'b> {
    statuses: &'b [(String, Option<String>)],
    evidence: &'b HashMap<String, Option<String>>,
    prerequisites: &'b HashMap<String, Vec<String>>,
    locations: &'b HashMap<String, At>,
}

/// One record definition: its identifier, a context for messages and the location of its first
/// definition.
struct Entry {
    id: String,
    context: String,
    location: At,
}

fn entry(path: &str, id: &str, locations: &HashMap<String, At>) -> Entry {
    Entry {
        id: id.to_owned(),
        context: format!("{path} {id}"),
        location: located(locations, id, path),
    }
}

fn located(locations: &HashMap<String, At>, id: &str, path: &str) -> At {
    locations.get(id).cloned().unwrap_or_else(|| At::file(path))
}

/// Analyses the ledger's records; the findings carry the aspects of `ledger/records` and
/// `ledger/record-status`.
///
/// # Errors
/// A pattern of the plugin that does not compile.
pub fn analyse(catalog: &Catalog<'_>, tracking: &Tracking<'_>) -> gate_sdk::Result<Findings> {
    let patterns = Patterns::new(tracking.settings.parity.as_ref())?;
    let mut analysis = Analysis {
        findings: Findings::default(),
        patterns: &patterns,
        tracking,
        known_implementation: HashSet::new(),
        known_follow_ups: HashSet::new(),
    };
    analysis.check_all(catalog);
    Ok(analysis.findings)
}

/// The paired parts of the analysis, present with parity.
struct Paired<'p> {
    parity: &'p Parity,
    plan: &'p Regex,
    implementation: &'p Regex,
    verification: &'p Regex,
    plan_candidate: &'p Regex,
    implementation_candidate: &'p Regex,
    verification_candidate: &'p Regex,
    verification_item: &'p Regex,
}

impl<'p> Paired<'p> {
    fn new(settings: &'p Settings, p: &'p Patterns) -> Option<Self> {
        Some(Self {
            parity: settings.parity.as_ref()?,
            plan: p.plan.as_ref()?,
            implementation: p.paired_implementation.as_ref()?,
            verification: p.paired_verification.as_ref()?,
            plan_candidate: p.plan_candidate.as_ref()?,
            implementation_candidate: p.paired_implementation_candidate.as_ref()?,
            verification_candidate: p.paired_verification_candidate.as_ref()?,
            verification_item: p.verification_item.as_ref()?,
        })
    }
}

struct Analysis<'a, 't> {
    findings: Findings,
    patterns: &'a Patterns,
    tracking: &'a Tracking<'t>,
    known_implementation: HashSet<String>,
    known_follow_ups: HashSet<String>,
}

impl Analysis<'_, '_> {
    fn emit(&mut self, aspect: &str, at: &At, message: String) {
        self.findings.emit(aspect, at, message);
    }

    fn emit_keyed(&mut self, aspect: &str, at: &At, key: String, message: String) {
        self.findings.emit_keyed(aspect, at, key, message);
    }

    fn settings(&self) -> &Settings {
        self.tracking.settings
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one pass over the records keeps each check in the order it reports"
    )]
    fn check_all(&mut self, catalog: &Catalog<'_>) {
        let p = self.patterns;
        let settings = self.tracking.settings;
        let implementation_path = settings.implementation.as_str();
        let verification_path = settings.verification.as_str();
        let paired = Paired::new(settings, p);
        let plan_path = paired
            .as_ref()
            .map_or("", |paired| paired.parity.plan.as_str());
        let label = paired
            .as_ref()
            .map_or("", |paired| paired.parity.label.as_str());
        let (plan_matches, implementation_matches, verification_matches) = match &paired {
            Some(paired) => (
                catalog.matches(plan_path, paired.plan),
                catalog.matches(implementation_path, paired.implementation),
                catalog.matches(verification_path, paired.verification),
            ),
            None => (Vec::new(), Vec::new(), Vec::new()),
        };
        let all_implementation = catalog.matches(implementation_path, &p.implementation);
        let all_verification = catalog.matches(verification_path, &p.verification);
        self.known_follow_ups = catalog
            .matches(&settings.follow_ups, &p.follow_up)
            .iter()
            .map(|found| found.group(1).to_owned())
            .collect();

        let (plan_ids, implementation_ids, verification_ids) = match &paired {
            Some(paired) => (
                self.records(
                    &plan_matches,
                    &catalog.matches(plan_path, paired.plan_candidate),
                    plan_path,
                    &format!("{label} plan heading"),
                ),
                self.records(
                    &implementation_matches,
                    &catalog.matches(implementation_path, paired.implementation_candidate),
                    implementation_path,
                    &format!("{label} implementation record"),
                ),
                self.records(
                    &verification_matches,
                    &catalog.matches(verification_path, paired.verification_candidate),
                    verification_path,
                    &format!("{label} verification record"),
                ),
            ),
            None => (Vec::new(), Vec::new(), Vec::new()),
        };
        let known_implementation = self.records(
            &all_implementation,
            &catalog.matches(implementation_path, &p.implementation_candidate),
            implementation_path,
            "implementation record",
        );
        self.known_implementation = known_implementation.iter().cloned().collect();

        let plan_sections = sections(&plan_matches, &p.heading_boundary);
        let implementation_sections = sections(&implementation_matches, &p.item_boundary);
        let all_implementation_sections = sections(&all_implementation, &p.item_boundary);
        let verification_sections = sections(&verification_matches, &p.item_boundary);
        let all_verification_sections = sections(&all_verification, &p.item_boundary);
        let known_verification: HashSet<String> = all_verification
            .iter()
            .map(|found| found.group(1).to_owned())
            .collect();
        let plan_locations = first_locations(&plan_matches);
        let implementation_locations = first_locations(&all_implementation);
        let verification_locations = first_locations(&all_verification);
        let plan_set: HashSet<&String> = plan_ids.iter().collect();
        let implementation_set: HashSet<&String> = implementation_ids.iter().collect();
        let verification_set: HashSet<&String> = verification_ids.iter().collect();

        for id in &plan_ids {
            if !implementation_set.contains(id) {
                self.emit_keyed(
                    "record-counterpart",
                    &At::file(implementation_path),
                    format!("{id}#missing"),
                    format!("{implementation_path} is missing {id}"),
                );
            }

            let verification_id = counterpart(id, "I-", "V-");

            if !verification_set.contains(&verification_id) {
                self.emit_keyed(
                    "record-counterpart",
                    &At::file(verification_path),
                    format!("{verification_id}#missing"),
                    format!("{verification_path} is missing {verification_id} for {id}"),
                );
            }
        }

        for id in &implementation_ids {
            if !plan_set.contains(id) {
                self.emit_keyed(
                    "record-counterpart",
                    &located(&implementation_locations, id, implementation_path),
                    format!("{id}#unplanned"),
                    format!("{implementation_path} contains {id} without a matching plan heading"),
                );
            }
        }

        for id in &verification_ids {
            let implementation_id = counterpart(id, "V-", "I-");

            if !plan_set.contains(&implementation_id) {
                self.emit_keyed(
                    "record-counterpart",
                    &located(&verification_locations, id, verification_path),
                    format!("{id}#no-implementation"),
                    format!(
                        "{verification_path} contains {id} without matching {implementation_id}"
                    ),
                );
            }
        }

        let mut graph: HashMap<String, Vec<String>> = HashMap::new();
        let mut known_statuses: HashMap<String, String> = HashMap::new();

        for found in &all_implementation {
            let id = found.group(1);
            let entry = entry(implementation_path, id, &implementation_locations);
            let fields = fields(
                all_implementation_sections
                    .get(id)
                    .map_or("", String::as_str),
                &p.item_field,
            );
            let status = self
                .optional(&fields, &["Status"], &entry, "Status")
                .and_then(|value| self.leading_backticked(&value));
            let blocked_by = self.optional(&fields, &["Blocked by"], &entry, "Blocked by");

            if let Some(status) = status {
                known_statuses.insert(id.to_owned(), status);
            }

            let dependencies = self.implementation_dependencies(id, blocked_by.as_deref(), &entry);
            graph.insert(id.to_owned(), dependencies);
        }

        let mut follow_up_statuses: HashMap<String, String> = HashMap::new();

        for found in catalog.matches(&settings.follow_ups, &p.follow_up_status) {
            follow_up_statuses.insert(found.group(1).to_owned(), trim(found.group(2)).to_owned());
        }

        let mut known_verification_statuses: HashMap<String, String> = HashMap::new();
        let mut known_verification_notes: HashMap<String, String> = HashMap::new();

        for found in &all_verification {
            let id = found.group(1);
            let entry = entry(verification_path, id, &verification_locations);
            let fields = fields(
                all_verification_sections.get(id).map_or("", String::as_str),
                &p.item_field,
            );
            let status = self
                .optional(&fields, &["Status"], &entry, "Status")
                .and_then(|value| self.leading_backticked(&value));
            let notes = self.optional(
                &fields,
                &["Verification notes"],
                &entry,
                "Verification notes",
            );

            if let Some(status) = status {
                known_verification_statuses.insert(id.to_owned(), status);
            }

            if let Some(notes) = notes {
                known_verification_notes.insert(id.to_owned(), notes);
            }
        }

        let mut plan_dependencies: HashMap<String, Option<Vec<String>>> = HashMap::new();
        let mut plan_statuses: HashMap<String, Option<String>> = HashMap::new();
        let mut plan_commits: HashMap<String, Option<String>> = HashMap::new();

        for id in &plan_ids {
            let entry = entry(plan_path, id, &plan_locations);
            let fields = fields(
                plan_sections.get(id).map_or("", String::as_str),
                &p.plan_field,
            );
            let status_value = self.require(&fields, &["Status"], &entry, "Status", "record-field");
            let blocked_by = self.require(
                &fields,
                &["Blocked by"],
                &entry,
                "Blocked by",
                "record-field",
            );
            let status = self.backticked_field(status_value.as_deref(), &entry, "Status");
            self.require(
                &fields,
                &["Owning repository", "Owning repositories"],
                &entry,
                "owning repository",
                "record-field",
            );

            if let Some(paired) = &paired
                && status.as_deref() == Some(paired.parity.active_status.as_str())
            {
                let reference = paired.parity.reference.as_str();

                for name in std::iter::once(reference)
                    .chain(paired.parity.fields.iter().map(String::as_str))
                {
                    let value = self.require(&fields, &[name], &entry, name, "parity-fields");

                    if name == reference
                        && value
                            .as_deref()
                            .is_some_and(|value| !p.full_oid.is_match(value))
                    {
                        self.emit_keyed(
                            "parity-fields",
                            &entry.location,
                            format!("{}/{reference}#full-oid", entry.id),
                            format!("{} must use a full {reference} OID", entry.context),
                        );
                    }
                }
            }

            let commit = self.require(&fields, &["Commit"], &entry, "Commit", "record-field");
            plan_statuses.insert(id.clone(), status);
            plan_commits.insert(id.clone(), commit);
            let dependencies = match blocked_by {
                Some(blocked_by) => self.dependencies(&blocked_by, &entry),
                None => None,
            };
            plan_dependencies.insert(id.clone(), dependencies);
        }

        let mut implementation_statuses: Vec<(String, Option<String>)> = Vec::new();
        let mut implementation_commits: HashMap<String, Option<String>> = HashMap::new();

        for id in &implementation_ids {
            let entry = entry(implementation_path, id, &implementation_locations);
            let fields = fields(
                implementation_sections.get(id).map_or("", String::as_str),
                &p.item_field,
            );
            let status_value = self.require(&fields, &["Status"], &entry, "Status", "record-field");
            self.require(
                &fields,
                &["Blocked by"],
                &entry,
                "Blocked by",
                "record-field",
            );
            let verification_item = self.require(
                &fields,
                &["Verification items", "Verification"],
                &entry,
                "Verification items",
                "record-field",
            );
            let commit = self.require(&fields, &["Commit"], &entry, "Commit", "record-field");
            let status = self.backticked_field(status_value.as_deref(), &entry, "Status");
            let expected = counterpart(id, "I-", "V-");
            let mapped = verification_item.as_deref().and_then(|item| {
                paired
                    .as_ref()
                    .and_then(|paired| paired.verification_item.captures(item))
                    .map(|captures| captures[1].to_owned())
            });
            self.require(
                &fields,
                &["Owning repository", "Owning repositories"],
                &entry,
                "owning repository",
                "record-field",
            );

            if mapped.as_deref() != Some(expected.as_str()) {
                self.emit_keyed(
                    "record-counterpart",
                    &entry.location,
                    format!("{}#verification-map", entry.id),
                    format!("{} must map exactly to {expected}", entry.context),
                );
            }

            set(&mut implementation_statuses, id, status);
            implementation_commits.insert(id.clone(), commit);
        }

        let implementation_status_map: HashMap<&String, &Option<String>> = implementation_statuses
            .iter()
            .map(|(id, status)| (id, status))
            .collect();
        let mut verification_notes: HashMap<String, Option<String>> = HashMap::new();
        let mut verification_statuses: HashMap<String, Option<String>> = HashMap::new();

        for id in &verification_ids {
            let entry = entry(verification_path, id, &verification_locations);
            let fields = fields(
                verification_sections.get(id).map_or("", String::as_str),
                &p.item_field,
            );
            let status_value = self.require(&fields, &["Status"], &entry, "Status", "record-field");
            let notes = self.require(
                &fields,
                &["Verification notes"],
                &entry,
                "Verification notes",
                "record-field",
            );
            let status = self.backticked_field(status_value.as_deref(), &entry, "Status");
            verification_statuses.insert(id.clone(), status);
            verification_notes.insert(id.clone(), notes);
        }

        let flatten = |value: Option<&Option<String>>| value.cloned().flatten();

        for id in &plan_ids {
            if !implementation_set.contains(id) {
                continue;
            }

            if let (Some(Some(plan)), Some(implementation)) =
                (plan_dependencies.get(id), graph.get(id))
                && !same_members(plan, implementation)
            {
                self.emit(
                    "record-dependency",
                    &located(&plan_locations, id, plan_path),
                    format!("{id} has different plan and implementation Blocked by sets"),
                );
            }

            let verification_id = counterpart(id, "I-", "V-");
            let states = [
                flatten(plan_statuses.get(id)),
                implementation_status_map
                    .get(id)
                    .and_then(|status| (*status).clone()),
                flatten(verification_statuses.get(&verification_id)),
            ];

            if states
                .iter()
                .all(|state| state.as_deref().is_some_and(|state| !state.is_empty()))
                && states.iter().collect::<HashSet<_>>().len() != 1
            {
                self.emit(
                    "record-status",
                    &located(&plan_locations, id, plan_path),
                    format!("{id} and {verification_id} must have exactly matching Status values"),
                );
            }
        }

        for id in &plan_ids {
            let verification_id = counterpart(id, "I-", "V-");
            let states = [
                (plan_path, flatten(plan_statuses.get(id))),
                (
                    implementation_path,
                    implementation_status_map
                        .get(id)
                        .and_then(|status| (*status).clone()),
                ),
                (
                    verification_path,
                    flatten(verification_statuses.get(&verification_id)),
                ),
            ];
            let location = located(&plan_locations, id, plan_path);

            if let Some(allowed) = self.tracking.allowed {
                for ((path, status), role) in
                    states
                        .iter()
                        .zip(["plan", "implementation", "verification"])
                {
                    if let Some(status) = status.as_deref().filter(|status| !status.is_empty())
                        && !allowed.iter().any(|candidate| candidate == status)
                    {
                        self.emit_keyed(
                            "record-status",
                            &location,
                            format!("{id}#unsupported-{role}"),
                            format!("{id} has unsupported Status {status} in {path}"),
                        );
                    }
                }
            }

            let plan_commit = self.commit_value(flatten(plan_commits.get(id)).as_deref());
            let implementation_commit =
                self.commit_value(flatten(implementation_commits.get(id)).as_deref());

            if let (Some(plan), Some(implementation)) = (&plan_commit, &implementation_commit)
                && !plan.is_empty()
                && !implementation.is_empty()
                && plan != implementation
            {
                self.emit_keyed(
                    "record-commit",
                    &location,
                    format!("{id}#differ"),
                    format!("{id} commits differ: plan={plan}, implementation={implementation}"),
                );
            }

            // The closed statuses compare as written here, not in lowercase.
            if self.tracking.unfinished_commit_pending
                && let Some(plan_status) =
                    states[0].1.as_deref().filter(|status| !status.is_empty())
                && !settings
                    .closed_statuses
                    .iter()
                    .any(|closed| closed == plan_status)
                && (plan_commit.as_deref() != Some("pending")
                    || implementation_commit.as_deref() != Some("pending"))
            {
                self.emit(
                    "record-commit",
                    &location,
                    format!("{id} is unfinished and must keep its commit pending"),
                );
            }
        }

        for (id, status) in &implementation_statuses {
            if !status
                .as_deref()
                .is_some_and(|status| settings.is_closed(status))
            {
                continue;
            }

            if !flatten(plan_commits.get(id)).is_some_and(|commit| p.full_oid.is_match(&commit)) {
                self.emit_keyed(
                    "record-commit",
                    &located(&plan_locations, id, plan_path),
                    format!("{id}#plan-oid"),
                    format!("{plan_path} {id} is closed without a backticked 40-hex Commit OID"),
                );
            }

            if !flatten(implementation_commits.get(id))
                .is_some_and(|commit| p.full_oid.is_match(&commit))
            {
                self.emit_keyed(
                    "record-commit",
                    &located(&implementation_locations, id, implementation_path),
                    format!("{id}#implementation-oid"),
                    format!(
                        "{implementation_path} {id} is closed without a backticked 40-hex Commit OID"
                    ),
                );
            }

            let verification_id = counterpart(id, "I-", "V-");

            if self.is_pending(flatten(verification_notes.get(&verification_id)).as_deref()) {
                self.emit_keyed(
                    "record-closure",
                    &located(&verification_locations, &verification_id, verification_path),
                    format!("{verification_id}#notes"),
                    format!(
                        "{verification_path} {verification_id} is closed without concrete verification notes"
                    ),
                );
            }

            for dependency in graph.get(id).cloned().unwrap_or_default() {
                let status = if dependency.starts_with("I-") {
                    known_statuses.get(&dependency)
                } else {
                    follow_up_statuses.get(&dependency)
                };

                if !status.is_some_and(|status| settings.is_closed(status)) {
                    self.emit_keyed(
                        "record-closure",
                        &located(&implementation_locations, id, implementation_path),
                        format!("{id}/{dependency}#unresolved"),
                        format!(
                            "{implementation_path} {id} is closed with unresolved dependency {dependency}"
                        ),
                    );
                }
            }
        }

        let mut states: HashMap<String, bool> = HashMap::new();
        let mut path: Vec<String> = Vec::new();
        let walk = Walk {
            graph: &graph,
            statuses: &known_statuses,
            locations: &implementation_locations,
            path: implementation_path,
            label,
        };

        for id in &implementation_ids {
            self.visit(id, &walk, &mut states, &mut path);
        }

        self.blockers(
            catalog,
            &Known {
                verification: &known_verification,
                implementation_statuses: &known_statuses,
                follow_up_statuses: &follow_up_statuses,
                verification_statuses: &known_verification_statuses,
                verification_notes: &known_verification_notes,
            },
        );
    }

    /// Reports malformed, untitled and duplicate records; returns the identifiers in first-seen
    /// order.
    fn records(
        &mut self,
        matches: &[Match],
        candidates: &[Match],
        path: &str,
        name: &str,
    ) -> Vec<String> {
        if matches.is_empty() {
            self.emit_keyed(
                "record-shape",
                &At::file(path),
                format!("{name}#none"),
                format!("{path} must contain at least one {name}"),
            );
        }

        if candidates.len() != matches.len() {
            let keys: HashSet<(String, usize)> = matches.iter().map(Match::key).collect();
            let location = candidates
                .iter()
                .find(|candidate| !keys.contains(&candidate.key()))
                .map_or_else(|| At::file(path), Match::locate);
            self.emit_keyed(
                "record-shape",
                &location,
                format!("{name}#malformed"),
                format!("{path} contains malformed {name} definitions"),
            );
        }

        for found in matches {
            if trim(found.group(2)).is_empty() {
                self.emit_keyed(
                    "record-shape",
                    &found.locate(),
                    format!("{}#title", found.group(1)),
                    format!("{path} {} must have a non-empty title", found.group(1)),
                );
            }
        }

        let mut ids: Vec<String> = Vec::new();
        let mut duplicate: Option<&Match> = None;

        for found in matches {
            let id = found.group(1).to_owned();

            if ids.contains(&id) {
                duplicate = duplicate.or(Some(found));
            } else {
                ids.push(id);
            }
        }

        if let Some(found) = duplicate {
            self.emit_keyed(
                "record-unique",
                &found.locate(),
                format!("{name}#duplicate"),
                format!("{path} contains duplicate {name} IDs"),
            );
        }

        ids
    }

    fn require(
        &mut self,
        fields: &Fields,
        names: &[&str],
        entry: &Entry,
        label: &str,
        aspect: &str,
    ) -> Option<String> {
        let values = values(fields, names);

        if let [value] = values.as_slice()
            && !value.is_empty()
        {
            return Some(value.clone());
        }

        self.emit_keyed(
            aspect,
            &entry.location,
            format!("{}/{label}#required", entry.id),
            format!(
                "{} must contain exactly one non-empty {label} field",
                entry.context
            ),
        );
        None
    }

    fn optional(
        &mut self,
        fields: &Fields,
        names: &[&str],
        entry: &Entry,
        label: &str,
    ) -> Option<String> {
        let values = values(fields, names);

        match values.as_slice() {
            [] => None,
            [value] if !value.is_empty() => Some(value.clone()),
            _ => {
                self.emit_keyed(
                    "record-field",
                    &entry.location,
                    format!("{}/{label}#duplicate", entry.id),
                    format!(
                        "{} must not contain duplicate or empty {label} fields",
                        entry.context
                    ),
                );
                None
            }
        }
    }

    fn backticked(&self, value: &str) -> Option<String> {
        self.patterns
            .backticked
            .captures(value)
            .map(|captures| trim(&captures[1]).to_owned())
            .filter(|value| !value.is_empty())
    }

    fn leading_backticked(&self, value: &str) -> Option<String> {
        self.patterns
            .leading_backticked
            .captures(value)
            .map(|captures| trim(&captures[1]).to_owned())
            .filter(|value| !value.is_empty())
    }

    fn backticked_field(
        &mut self,
        value: Option<&str>,
        entry: &Entry,
        label: &str,
    ) -> Option<String> {
        let value = value.filter(|value| !value.is_empty())?;
        let parsed = self.backticked(value);

        if parsed.is_none() {
            self.emit_keyed(
                "record-field",
                &entry.location,
                format!("{}/{label}#backticked", entry.id),
                format!(
                    "{} {label} must be one non-empty backticked value",
                    entry.context
                ),
            );
        }

        parsed
    }

    /// A commit field as the comparison reads it: the backticked value, or the text without a
    /// trailing period.
    fn commit_value(&self, value: Option<&str>) -> Option<String> {
        let value = value?;
        Some(
            self.backticked(value)
                .unwrap_or_else(|| value.strip_suffix('.').unwrap_or(value).to_owned()),
        )
    }

    fn is_pending(&self, value: Option<&str>) -> bool {
        let p = self.patterns;
        let value = value.unwrap_or("");
        let unlinked = p.link.replace_all(value, "$1");
        let unmarked = p.markup.replace_all(&unlinked, "");
        let trimmed = trim(&unmarked);
        let unpunctuated = p.trailing_punctuation.replace(trimmed, "");
        let collapsed = p.whitespace.replace_all(trim(&unpunctuated), " ");
        let normalized = collapsed.to_lowercase();

        self.tracking.pending_values.contains(&normalized)
            || self
                .tracking
                .pending_patterns
                .iter()
                .any(|pattern| pattern.is_match(&normalized))
    }

    fn validate_dependencies(&mut self, dependencies: Vec<String>, entry: &Entry) -> Vec<String> {
        let mut seen: HashSet<String> = HashSet::new();

        for dependency in &dependencies {
            if !seen.insert(dependency.clone()) {
                self.emit_keyed(
                    "record-dependency",
                    &entry.location,
                    format!("{}/{dependency}#duplicate", entry.id),
                    format!(
                        "{} contains duplicate dependency {dependency}",
                        entry.context
                    ),
                );
            }

            let known = if dependency.starts_with("I-") {
                self.known_implementation.contains(dependency)
            } else {
                self.known_follow_ups.contains(dependency)
            };

            if !known {
                self.emit_keyed(
                    "record-dependency",
                    &entry.location,
                    format!("{}/{dependency}#unknown", entry.id),
                    format!(
                        "{} references unknown dependency {dependency}",
                        entry.context
                    ),
                );
            }
        }

        dependencies
    }

    fn dependency_ids(&self, value: &str) -> Vec<String> {
        self.patterns
            .dependency_id
            .captures_iter(value)
            .map(|captures| captures[1].to_owned())
            .collect()
    }

    fn dependencies(&mut self, value: &str, entry: &Entry) -> Option<Vec<String>> {
        if self.patterns.none.is_match(value) {
            return Some(Vec::new());
        }

        if !self.patterns.dependency_list.is_match(value) {
            self.emit_keyed(
                "record-dependency",
                &entry.location,
                format!("{}/Blocked by#format", entry.id),
                format!(
                    "{} Blocked by must be none or a comma-separated list of backticked I-* or F-* IDs",
                    entry.context
                ),
            );
            return None;
        }

        let ids = self.dependency_ids(value);
        Some(self.validate_dependencies(ids, entry))
    }

    fn implementation_dependencies(
        &mut self,
        id: &str,
        value: Option<&str>,
        entry: &Entry,
    ) -> Vec<String> {
        let Some(value) = value.filter(|value| !value.is_empty()) else {
            return Vec::new();
        };

        if let Some(parity) = &self.settings().parity
            && id.starts_with(&format!("I-{}-", parity.prefix))
        {
            return self.dependencies(value, entry).unwrap_or_default();
        }

        if self.patterns.no_dependency.is_match(value) {
            return Vec::new();
        }

        let ids = self.dependency_ids(value);
        self.validate_dependencies(ids, entry)
    }

    fn visit(
        &mut self,
        id: &str,
        walk: &Walk<'_>,
        states: &mut HashMap<String, bool>,
        path: &mut Vec<String>,
    ) {
        let Walk {
            graph,
            statuses,
            locations,
            path: file,
            label,
        } = *walk;

        match states.get(id) {
            Some(true) => return,
            Some(false) => {
                let start = path.iter().position(|item| item == id).unwrap_or(0);
                let mut cycle: Vec<&str> = path[start..].iter().map(String::as_str).collect();
                cycle.push(id);
                self.emit_keyed(
                    "dependency-cycle",
                    &located(locations, id, file),
                    cycle.join(" -> "),
                    format!(
                        "{file} contains a cycle reachable from a {label} slice: {}",
                        cycle.join(" -> ")
                    ),
                );
                return;
            }
            None => {}
        }

        if !statuses.contains_key(id) {
            self.emit_keyed(
                "record-dependency",
                &located(locations, id, file),
                format!("{id}#unreadable-status"),
                format!("{file} {id} is reachable from {label} without a readable Status"),
            );
        }

        states.insert(id.to_owned(), false);
        path.push(id.to_owned());

        for dependency in graph.get(id).cloned().unwrap_or_default() {
            if dependency.starts_with("I-") && self.known_implementation.contains(&dependency) {
                self.visit(&dependency, walk, states, path);
            }
        }

        path.pop();
        states.insert(id.to_owned(), true);
    }

    fn blockers(&mut self, catalog: &Catalog<'_>, known: &Known<'_>) {
        let p = self.patterns;
        let blockers_path = self.tracking.settings.blockers.clone();
        let matches = catalog.matches(&blockers_path, &p.blocker);
        let ids = self.records(
            &matches,
            &catalog.matches(&blockers_path, &p.blocker_candidate),
            &blockers_path,
            "production blocker record",
        );
        let id_set: HashSet<String> = ids.iter().cloned().collect();
        let blocker_sections = sections(&matches, &p.item_boundary);
        let locations = first_locations(&matches);
        let mut statuses: Vec<(String, Option<String>)> = Vec::new();
        let mut evidence: HashMap<String, Option<String>> = HashMap::new();
        let mut prerequisites: HashMap<String, Vec<String>> = HashMap::new();

        for id in &ids {
            let entry = entry(&blockers_path, id, &locations);
            let fields = fields(
                blocker_sections.get(id).map_or("", String::as_str),
                &p.item_field,
            );
            let phase_value = self.require(&fields, &["Phase"], &entry, "Phase", "record-field");
            let phase = self.backticked_field(phase_value.as_deref(), &entry, "Phase");
            let status_value = self.require(&fields, &["Status"], &entry, "Status", "record-field");
            let status = self.backticked_field(status_value.as_deref(), &entry, "Status");
            let exit_evidence = self.require(
                &fields,
                &["Exit evidence"],
                &entry,
                "Exit evidence",
                "record-field",
            );
            let links = self.require(
                &fields,
                &["Prerequisite links"],
                &entry,
                "Prerequisite links",
                "record-field",
            );
            self.require(&fields, &["Owner"], &entry, "Owner", "record-field");
            self.require(
                &fields,
                &["Ledger links"],
                &entry,
                "Ledger links",
                "record-field",
            );
            self.require(
                &fields,
                &["Exit criteria"],
                &entry,
                "Exit criteria",
                "record-field",
            );

            let phases = self.tracking.blocker_phases;

            if let Some(phase) = &phase
                && !phases.is_empty()
                && !phases.iter().any(|allowed| allowed == phase)
            {
                self.emit_keyed(
                    "record-field",
                    &entry.location,
                    format!("{}/Phase#unsupported", entry.id),
                    format!("{} uses unsupported Phase {phase}", entry.context),
                );
            }

            set(&mut statuses, id, status);
            evidence.insert(id.clone(), exit_evidence);
            let parsed = self.prerequisites(links.as_deref(), &entry, known.verification, &id_set);
            prerequisites.insert(id.clone(), parsed);
        }

        self.closed_blockers(
            &blockers_path,
            &Blockers {
                statuses: &statuses,
                evidence: &evidence,
                prerequisites: &prerequisites,
                locations: &locations,
            },
            known,
        );
    }

    /// Closed production blockers carry evidence and only closed prerequisites.
    fn closed_blockers(&mut self, path: &str, blockers: &Blockers<'_>, known: &Known<'_>) {
        let Blockers {
            statuses,
            evidence,
            prerequisites,
            locations,
        } = *blockers;
        let settings = self.tracking.settings;
        let closed: HashSet<&String> = statuses
            .iter()
            .filter(|(_, status)| status.as_deref() == Some("closed"))
            .map(|(id, _)| id)
            .collect();

        for (id, _) in statuses.iter().filter(|(id, _)| closed.contains(id)) {
            let entry = entry(path, id, locations);

            if self.is_pending(evidence.get(id).cloned().flatten().as_deref()) {
                self.emit_keyed(
                    "record-closure",
                    &entry.location,
                    format!("{}/Exit evidence#placeholder", entry.id),
                    format!(
                        "{} is closed without non-placeholder Exit evidence",
                        entry.context
                    ),
                );
            }

            for prerequisite in prerequisites.get(id).cloned().unwrap_or_default() {
                let open = if prerequisite.starts_with("V-") {
                    let status = known.verification_statuses.get(&prerequisite);
                    let notes = known.verification_notes.get(&prerequisite);

                    if !status.is_some_and(|status| settings.is_closed(status))
                        || self.is_pending(notes.map(String::as_str))
                    {
                        self.emit_keyed(
                            "record-closure",
                            &entry.location,
                            format!("{}/{prerequisite}#open-verification", entry.id),
                            format!(
                                "{} is closed while verification prerequisite {prerequisite} is open",
                                entry.context
                            ),
                        );
                    }

                    false
                } else if prerequisite.starts_with("PB-") {
                    !closed.contains(&prerequisite)
                } else {
                    let status = if prerequisite.starts_with("I-") {
                        known.implementation_statuses.get(&prerequisite)
                    } else {
                        known.follow_up_statuses.get(&prerequisite)
                    };
                    !status.is_some_and(|status| settings.is_closed(status))
                };

                if open {
                    self.emit_keyed(
                        "record-closure",
                        &entry.location,
                        format!("{}/{prerequisite}#unresolved", entry.id),
                        format!(
                            "{} is closed with unresolved prerequisite {prerequisite}",
                            entry.context
                        ),
                    );
                }
            }
        }
    }

    fn prerequisites(
        &mut self,
        value: Option<&str>,
        entry: &Entry,
        known_verification: &HashSet<String>,
        blockers: &HashSet<String>,
    ) -> Vec<String> {
        let p = self.patterns;
        let Some(value) = value.filter(|value| !value.is_empty() && !p.none.is_match(value)) else {
            return Vec::new();
        };

        if !p.blocker_prerequisites.is_match(value) {
            self.emit_keyed(
                "record-dependency",
                &entry.location,
                format!("{}/Prerequisite links#format", entry.id),
                format!(
                    "{} Prerequisite links must be none or a comma-separated list of backticked I/F/V/PB IDs",
                    entry.context
                ),
            );
            return Vec::new();
        }

        let found: Vec<String> = p
            .blocker_prerequisite
            .captures_iter(value)
            .map(|captures| captures[1].to_owned())
            .collect();
        let mut seen: HashSet<&String> = HashSet::new();

        for prerequisite in &found {
            if !seen.insert(prerequisite) {
                self.emit_keyed(
                    "record-dependency",
                    &entry.location,
                    format!("{}/{prerequisite}#duplicate", entry.id),
                    format!(
                        "{} contains duplicate prerequisite {prerequisite}",
                        entry.context
                    ),
                );
            }

            let known = (prerequisite.starts_with("I-")
                && self.known_implementation.contains(prerequisite))
                || (prerequisite.starts_with("F-") && self.known_follow_ups.contains(prerequisite))
                || (prerequisite.starts_with("V-") && known_verification.contains(prerequisite))
                || (prerequisite.starts_with("PB-") && blockers.contains(prerequisite));

            if !known {
                self.emit_keyed(
                    "record-dependency",
                    &entry.location,
                    format!("{}/{prerequisite}#unknown", entry.id),
                    format!(
                        "{} references unknown prerequisite {prerequisite}",
                        entry.context
                    ),
                );
            }
        }

        found
    }
}

/// The dependency graph a cycle walk reads.
#[derive(Clone, Copy)]
struct Walk<'w> {
    graph: &'w HashMap<String, Vec<String>>,
    statuses: &'w HashMap<String, String>,
    locations: &'w HashMap<String, At>,
    path: &'w str,
    label: &'w str,
}

fn values(fields: &Fields, names: &[&str]) -> Vec<String> {
    names
        .iter()
        .flat_map(|name| fields.get(*name).cloned().unwrap_or_default())
        .collect()
}

/// Sets a value in an insertion-ordered list, keeping the first position of its key.
fn set(list: &mut Vec<(String, Option<String>)>, id: &str, value: Option<String>) {
    match list.iter_mut().find(|(key, _)| key == id) {
        Some(item) => item.1 = value,
        None => list.push((id.to_owned(), value)),
    }
}

fn counterpart(id: &str, from: &str, to: &str) -> String {
    id.strip_prefix(from)
        .map_or_else(|| id.to_owned(), |rest| format!("{to}{rest}"))
}

fn same_members(left: &[String], right: &[String]) -> bool {
    let left: HashSet<&String> = left.iter().collect();
    let right: HashSet<&String> = right.iter().collect();
    left == right
}

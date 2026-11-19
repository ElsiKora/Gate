//! `nestjs/field-inventory`: every persisted string or JSON column and every API-plane DTO property
//! that can hold free text declares exactly one class in `fields`, with its owning decision and the
//! planes it is exposed on.
//!
//! A field can hold free text unless its TypeScript type is a repository enum or string literal,
//! its Automator type is no string or object, its Automator format admits no free text, or it is a
//! DTO object with a nested DTO type. An Automator code format makes it a code of that kind without
//! an entry. Each entry names a class, a code `kind` for `CODE`, a `reason` for `MACHINE`, an
//! accepted decision and the exposure, which must equal the planes derived from the source; a
//! private staff note or third-party value outside the internal plane needs an accepted
//! `exposing-decision`. Source the rule cannot read where it decides exposure or scope is
//! `unreadable`.
//!
//! A `CODE` entry of kind `CONFIG_KEY` lists under `reference` the configuration and secret
//! contracts that locate its record; a `STABLE_KEY` code never reaches `PUBLIC`; an `IDENTITY` code
//! keeps its Automator pattern, cannot change and is unique within its scope.

mod code;
mod exposure;
mod inventory;
mod model;
mod object;
mod reference;

use crate::shared::paths::{self, Form};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Deref;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::TypeScript;
use gate_typescript::owners::Sources;

use self::inventory::Inventory;
use self::model::{Class, CodeKind, Entry, Field, Key, Plane, Reference, Role, Scope};
use crate::layout::{Layout, PluginOptions, folder_prefix};
use crate::shared::json_lines::element_lines;
use crate::shared::report::location;
use crate::shared::{decisions, project};

/// The options of `nestjs/field-inventory`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The classified fields; a field that can hold free text and has no entry is unclassified.
    fields: Vec<Entry>,
    /// The roots of the environment configuration and secret contracts a `CONFIG_KEY` references;
    /// by default `config/runtime-config/` below the source root, `configuration/` below the
    /// infrastructure folder and `lib/config/` below the infrastructure-as-code project.
    contract_roots: Option<Vec<String>>,
    /// The persisted column types that hold text.
    text_column_types: Vec<String>,
    /// The Automator string formats that make a field a code, with its kind.
    code_formats: BTreeMap<String, CodeKind>,
    /// The Automator string formats that admit no free text.
    machine_formats: Vec<String>,
    /// How many base classes of an entity are followed for its columns.
    base_depth: u8,
    /// How deep a DTO class reference is followed through constants and objects.
    reference_depth: u8,
    /// The decision index. Entries need one: without it, or when it is not tracked, the rule
    /// reports one `#classification` finding at the options and checks no decision.
    decisions: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            fields: Vec::new(),
            contract_roots: None,
            text_column_types: strings(&[
                "bpchar",
                "char",
                "character",
                "character varying",
                "citext",
                "hstore",
                "json",
                "jsonb",
                "longtext",
                "mediumtext",
                "nchar",
                "nvarchar",
                "simple-array",
                "simple-json",
                "string",
                "text",
                "tinytext",
                "varchar",
                "xml",
            ]),
            code_formats: BTreeMap::from([
                ("COUNTRY_CODE_ALPHA2".to_owned(), CodeKind::Iso3166Alpha2),
                ("CURRENCY_CODE".to_owned(), CodeKind::Iso4217),
                ("LANGUAGE_CODE_ISO639".to_owned(), CodeKind::Iso6391),
                ("LOCALE".to_owned(), CodeKind::Iso6391),
            ]),
            machine_formats: strings(&MACHINE_FORMATS),
            base_depth: 8,
            reference_depth: 12,
            decisions: None,
        }
    }
}

/// The default Automator formats that admit no free text.
const MACHINE_FORMATS: [&str; 38] = [
    "API_KEY",
    "BASE64",
    "BCRYPT",
    "BIGINT_STRING",
    "BITCOIN_ADDRESS",
    "COORDINATES",
    "CREDIT_CARD",
    "CRON",
    "DATE",
    "EMAIL",
    "ETHEREUM_ADDRESS",
    "GIT_COMMIT_SHA",
    "HASH_MD5",
    "HASH_SHA1",
    "HASH_SHA256",
    "HASH_SHA512",
    "HEX_COLOR",
    "HSLA_COLOR",
    "HSL_COLOR",
    "IBAN",
    "IPV4",
    "IPV6",
    "ISBN",
    "JWT",
    "MAC_ADDRESS",
    "MIME_TYPE",
    "MONGODB_OBJECT_ID",
    "NANOID",
    "PASSWORD",
    "PHONE_NUMBER",
    "POSTAL_CODE",
    "RGBA_COLOR",
    "RGB_COLOR",
    "SEMVER",
    "TIMEZONE",
    "ULID",
    "USERNAME",
    "UUID",
];

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

/// The vocabulary the source is read with: package names, planes, column types, formats and
/// depths.
#[derive(Debug)]
pub(crate) struct Names {
    /// The CRUD automator package.
    pub automator: String,
    /// The `TypeORM` package.
    pub typeorm: String,
    /// The modules root with a trailing slash.
    modules_root: String,
    /// The plane folders below the modules root, each with its plane.
    planes: Vec<(String, Plane)>,
    pub text_types: Vec<String>,
    pub code_formats: BTreeMap<String, CodeKind>,
    pub machine_formats: Vec<String>,
    pub base_depth: u8,
    pub reference_depth: u8,
    /// The roots of the configuration and secret contracts, set or derived from the layout.
    pub contract_roots: Vec<String>,
}

impl Names {
    fn new(layout: &Layout, options: &Options) -> Self {
        let planes = layout
            .planes_without_domain()
            .filter_map(|folder| {
                Plane::ALL
                    .into_iter()
                    .find(|plane| plane.label().eq_ignore_ascii_case(folder))
                    .map(|plane| (folder.to_owned(), plane))
            })
            .collect();

        Self {
            automator: layout.automator_package.clone(),
            typeorm: layout.typeorm_package.clone(),
            modules_root: folder_prefix(&layout.modules_root),
            planes,
            text_types: options.text_column_types.clone(),
            code_formats: options.code_formats.clone(),
            machine_formats: options.machine_formats.clone(),
            base_depth: options.base_depth,
            reference_depth: options.reference_depth,
            contract_roots: options.contract_roots.clone().unwrap_or_else(|| {
                vec![
                    format!("{}config/runtime-config/", layout.source_prefix()),
                    format!(
                        "{}configuration/",
                        folder_prefix(&layout.infrastructure_folder)
                    ),
                    format!("{}lib/config/", folder_prefix(&layout.cdk_root)),
                ]
            }),
        }
    }

    /// The plane a source path belongs to: `<modules-root>/<plane>/…`.
    pub(crate) fn plane(&self, path: &str) -> Option<Plane> {
        let folder = path.strip_prefix(&self.modules_root)?.split('/').next()?;

        self.planes
            .iter()
            .find(|(known, _)| known == folder)
            .map(|(_, plane)| *plane)
    }
}

/// The sources with the vocabulary they are read with; it reads as the sources.
#[derive(Clone, Copy)]
pub(crate) struct Read<'s, 'a> {
    pub sources: &'s Sources<'a>,
    pub names: &'s Names,
}

impl<'a> Deref for Read<'_, 'a> {
    type Target = Sources<'a>;

    fn deref(&self) -> &Self::Target {
        self.sources
    }
}

/// The rule.
#[derive(Debug, Default)]
pub struct FieldInventory;

impl Options {
    /// Checks that every path option is a normalized path relative to the project root.
    fn check_paths(&self) -> gate_sdk::Result<()> {
        paths::check_all(
            "contract-roots",
            self.contract_roots.iter().flatten(),
            Form::Prefix,
        )?;
        if let Some(decisions) = &self.decisions {
            paths::check("decisions", decisions, Form::File)?;
        }
        for (index, entry) in self.fields.iter().enumerate() {
            paths::check(&format!("fields[{index}].path"), &entry.path, Form::File)?;
        }
        Ok(())
    }
}

impl Rule<crate::Nest> for FieldInventory {
    type Options = Options;

    const NAME: &'static str = "field-inventory";
    const TITLE: &'static str =
        "Every field that can hold free text declares one class, its decision and its exposure";
    const ASPECTS: &'static [&'static str] = &[
        "classification",
        "code-format",
        "code-immutable",
        "code-unique",
        "config-key-field",
        "config-key-source",
        "config-key-unreferenced",
        "exposure",
        "private-exposure",
        "stale-entry",
        "unclassified",
        "unreadable",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            run.options().check_paths()?;
            let options = run.options();
            let index = options
                .decisions
                .as_ref()
                .map(|path| (path, shared.text(path)));
            let decisions = index
                .as_ref()
                .and_then(|(_, text)| text.as_deref().map(decisions::accepted));
            let located = match run.options_file() {
                Some(file) => Located {
                    lines: element_lines(&shared.text(file).unwrap_or_default(), "fields"),
                    path: file.to_owned(),
                },
                None => Located {
                    path: "gate.toml".to_owned(),
                    lines: Vec::new(),
                },
            };
            let names = Names::new(&layout, options);
            let typescript = shared.typescript()?;
            let sources = Sources::new(typescript, &layout.source_root)?;
            let read = Read {
                sources: &sources,
                names: &names,
            };
            let accepted = |decision: &str| {
                decisions
                    .as_ref()
                    .is_none_or(|index| index.contains(decision))
            };
            let mut findings = evaluate(read, typescript, options, &located, &accepted)?;

            // Without its decision index the rule cannot tell an accepted decision; one finding
            // stands for every decision the entries name, so that the run never passes on them.
            let unread = match &index {
            Some((path, None)) => Some(format!(
                "the decision index {path} is not tracked; correct `decisions` or add the index"
            )),
            None if !options.fields.is_empty() => Some(
                "the entries name decisions, but `decisions` names no decision index; set it to \
                 the index that lists the accepted decisions"
                    .to_owned(),
            ),
            _ => None,
        };

            if let Some(message) = unread {
                findings.push(Finding {
                    aspect: "classification",
                    path: located.path.clone(),
                    line: None,
                    message,
                    target: None,
                    key: Some("decisions".to_owned()),
                });
                findings.sort_by(|left, right| {
                    (&left.path, left.line, &left.message).cmp(&(
                        &right.path,
                        right.line,
                        &right.message,
                    ))
                });
            }
            let mut reported: Vec<(&'static str, String, Option<u32>, String)> = Vec::new();

            for finding in findings {
                let row = (
                    finding.aspect,
                    finding.path.clone(),
                    finding.line,
                    finding.message.clone(),
                );

                if reported.contains(&row) {
                    continue;
                }

                reported.push(row);
                let line = finding.line.and_then(|line| usize::try_from(line).ok());
                let report = run
                    .report()
                    .finding(
                        finding.aspect,
                        location(&finding.path, line),
                        finding.message,
                    )
                    .help(help(finding.aspect));

                if let Some(target) = finding.target {
                    report.target(target);
                }

                if let Some(key) = finding.key {
                    report.key(key);
                }
            }

            Ok(())
        })
    }
}

fn help(aspect: &str) -> &'static str {
    match aspect {
        "unclassified" => {
            "add an entry for the field to `fields` with its class, decision and exposure"
        }
        "exposure" => "set the entry's exposure to the planes the source exposes the field on",
        "private-exposure" => {
            "keep the field off the other planes, or name an accepted exposingDecision"
        }
        "stale-entry" => "remove the entry, or correct it to an existing field",
        "unreadable" => "write the configuration so that it can be read statically",
        "code-format" => "declare the Automator pattern of the code's kind",
        "code-immutable" => "keep an IDENTITY code out of every update body",
        "code-unique" => "add a unique index or constraint over the code and its scope",
        "config-key-unreferenced" => "list the contracts that reference the key under `reference`",
        "config-key-source" => "reference a string property of a configuration or secret contract",
        "config-key-field" => "remove the references, or make the entry a CONFIG_KEY code",
        _ => "correct the entry",
    }
}

/// One finding with the field it is about.
#[derive(Clone, Debug)]
pub(crate) struct Finding {
    pub aspect: &'static str,
    pub path: String,
    pub line: Option<u32>,
    pub message: String,
    /// `<path>:<Class>.<member>`, with ` [<PLANE>,…]` for `unclassified`; `None` for a finding
    /// about no field.
    pub target: Option<String>,
    /// The finding's stable key: `<Class>.<member>`, with `#<check>` where one field or entry can
    /// fail several checks of one aspect, or the name of what the finding is about; it stays when
    /// the message's wording or the options file's path changes.
    pub key: Option<String>,
}

impl Finding {
    /// A finding about the field `key`.
    pub(crate) fn about(
        aspect: &'static str,
        path: impl Into<String>,
        line: Option<u32>,
        message: String,
        key: &Key,
    ) -> Self {
        Self {
            aspect,
            path: path.into(),
            line,
            message,
            target: Some(format!("{}:{key}", key.path)),
            key: Some(key.to_string()),
        }
    }

    /// The finding with `#check` after its key, for one of several checks of one aspect.
    pub(crate) fn check(mut self, check: &str) -> Self {
        if let Some(key) = &mut self.key {
            key.push('#');
            key.push_str(check);
        }
        self
    }
}

/// The options file location of the entries, for findings about them.
pub(crate) struct Located {
    pub path: String,
    lines: Vec<u32>,
}

impl Located {
    pub(crate) fn line(&self, index: usize) -> Option<u32> {
        self.lines.get(index).copied()
    }
}

/// Reads the source and compares it with the entries.
fn evaluate(
    read: Read<'_, '_>,
    typescript: &TypeScript<'_>,
    options: &Options,
    located: &Located,
    accepted: &dyn Fn(&str) -> bool,
) -> gate_sdk::Result<Vec<Finding>> {
    let entries = &options.fields;
    let inventory = Inventory::read(read);
    let exposure = exposure::derive(&inventory);
    let (classified, mut findings) = check_entries(entries, located, accepted);

    for unread in &inventory.unreadable {
        findings.push(Finding {
            aspect: "unreadable",
            path: unread.path.clone(),
            line: Some(unread.line),
            message: unread.message.clone(),
            target: None,
            key: None,
        });
    }

    for field in inventory.fields.values() {
        let index = classified.get(&field.key).copied();
        let entry = index.map(|index| &entries[index]);
        let derived = exposure.get(&field.key).cloned().unwrap_or_default();

        findings.extend(compare(
            field,
            entry,
            &derived,
            index.and_then(|index| located.line(index)),
            &located.path,
        ));
    }

    for (key, index) in &classified {
        if !inventory.fields.contains_key(key) {
            findings.push(Finding::about(
                "stale-entry",
                located.path.clone(),
                located.line(*index),
                format!("entry {key} matches no entity column or DTO property"),
                key,
            ));
        }
    }

    for (key, index) in &classified {
        findings.extend(references(
            typescript,
            &read.names.contract_roots,
            accepted,
            &entries[*index],
            inventory.fields.get(key),
            located.line(*index),
            &located.path,
        )?);
    }

    let codes = code::Codes::new(&inventory);

    for (key, index) in &classified {
        findings.extend(codes.findings(
            &entries[*index],
            inventory.fields.get(key),
            located.line(*index),
            &located.path,
        ));
    }

    findings.sort_by(|left, right| {
        (&left.path, left.line, &left.message).cmp(&(&right.path, right.line, &right.message))
    });

    Ok(findings)
}

/// The first entry of each field by its index, and the findings about the entries themselves.
fn check_entries(
    entries: &[Entry],
    lines: &Located,
    accepted: &dyn Fn(&str) -> bool,
) -> (BTreeMap<Key, usize>, Vec<Finding>) {
    let mut classified: BTreeMap<Key, usize> = BTreeMap::new();
    let mut findings = Vec::new();

    for (index, entry) in entries.iter().enumerate() {
        let key = entry.key();
        let mut problem = |check: &str, message: String| {
            findings.push(
                Finding::about(
                    "classification",
                    lines.path.clone(),
                    lines.line(index),
                    message,
                    &key,
                )
                .check(check),
            );
        };

        if classified.contains_key(&key) {
            problem(
                "repeated",
                format!("entry {key} repeats an earlier entry for the same field"),
            );
            continue;
        }

        classified.insert(key.clone(), index);
        check_entry(entry, &key, accepted, &mut problem);
    }

    (classified, findings)
}

/// The findings about one entry's own fields.
fn check_entry(
    entry: &Entry,
    key: &Key,
    accepted: &dyn Fn(&str) -> bool,
    problem: &mut dyn FnMut(&str, String),
) {
    match (entry.class, entry.kind) {
        (Class::Code, None) => problem("kind", format!("entry {key} is a CODE and needs a kind")),
        (class, Some(kind)) if class != Class::Code => problem(
            "kind",
            format!(
                "entry {key} names the code kind {}, which only a CODE entry carries",
                kind.label()
            ),
        ),
        _ => {}
    }

    if entry.class == Class::Machine && entry.reason.as_deref().is_none_or(str::is_empty) {
        problem(
            "reason",
            format!("entry {key} is a MACHINE value and needs a reason"),
        );
    }

    if !accepted(&entry.decision) {
        problem(
            "decision",
            format!(
                "entry {key} names {}, which is not an accepted decision",
                entry.decision
            ),
        );
    }

    if let Some(decision) = &entry.exposing_decision {
        if !entry.class.is_private() {
            problem(
                "exposing-decision",
                format!(
                    "entry {key} names an exposingDecision, which only a STAFF_NOTE or \
                     THIRD_PARTY entry carries"
                ),
            );
        } else if !accepted(decision) {
            problem(
                "exposing-decision",
                format!(
                    "entry {key} names the exposingDecision {decision}, which is not an accepted \
                     decision"
                ),
            );
        }
    }

    if entry.alternative == Some(entry.class) {
        problem(
            "alternative",
            format!(
                "entry {key} names its own class {} as its alternative",
                entry.class.label()
            ),
        );
    }

    if entry.exposure.iter().collect::<BTreeSet<_>>().len() != entry.exposure.len() {
        problem(
            "exposure",
            format!("entry {key} lists an exposure plane twice"),
        );
    }

    if entry.class != Class::Code && (entry.role.is_some() || entry.scope.is_some()) {
        problem(
            "role-scope",
            format!("entry {key} names a role or a scope, which only a CODE entry carries"),
        );
    } else if entry.role == Some(Role::Identity) && entry.scope.is_none() {
        problem(
            "role-scope",
            format!("entry {key} is an IDENTITY code and needs its scope"),
        );
    } else if entry.role != Some(Role::Identity) && entry.scope.is_some() {
        problem(
            "role-scope",
            format!("entry {key} names a scope, which only an IDENTITY code carries"),
        );
    }
}

/// The findings of one source field against its entry, whose options line is `entry_line`.
fn compare(
    field: &Field,
    entry: Option<&Entry>,
    derived: &BTreeSet<Plane>,
    entry_line: Option<u32>,
    options_path: &str,
) -> Vec<Finding> {
    let at_field = |aspect: &'static str, message: String| {
        Finding::about(
            aspect,
            field.key.path.clone(),
            Some(field.line),
            message,
            &field.key,
        )
    };
    let stale = |reason: String| {
        Finding::about(
            "stale-entry",
            options_path,
            entry_line,
            format!("entry {} classifies a field that {reason}", field.key),
            &field.key,
        )
    };

    match (&field.scope, entry) {
        (Scope::Text, None) => {
            let mut finding = at_field(
                "unclassified",
                format!(
                    "{} can hold free text and has no entry in {options_path}",
                    field.key
                ),
            );
            finding.target = Some(pinned(&field.key, derived));
            vec![finding]
        }
        (Scope::Text, Some(entry)) => {
            let declared: BTreeSet<Plane> = entry.exposure.iter().copied().collect();
            let mut findings = Vec::new();

            if *derived != declared {
                findings.push(at_field(
                    "exposure",
                    format!(
                        "{} is exposed on {} but its entry declares {}",
                        field.key,
                        planes(derived),
                        planes(&declared)
                    ),
                ));
            }

            if entry.class.is_private()
                && entry.exposing_decision.is_some()
                && derived.iter().all(|plane| *plane == Plane::Internal)
            {
                let reach = if derived.is_empty() {
                    "is exposed on no plane"
                } else {
                    "is exposed on INTERNAL only"
                };
                findings.push(stale(format!(
                    "{reach}, so its exposingDecision exposes nothing"
                )));
            }

            if entry.is_stable_key() && derived.contains(&Plane::Public) {
                findings.push(
                    at_field(
                        "private-exposure",
                        format!(
                            "{} is a STABLE_KEY code exposed on {}, but an internal stable key \
                             is never displayed on PUBLIC",
                            field.key,
                            planes(derived)
                        ),
                    )
                    .check("stable-key"),
                );
            }

            if entry.class.is_private()
                && entry.exposing_decision.is_none()
                && derived.iter().any(|plane| *plane != Plane::Internal)
            {
                findings.push(
                    at_field(
                        "private-exposure",
                        format!(
                            "{} is a {} exposed on {} without an exposingDecision",
                            field.key,
                            entry.class.label(),
                            planes(derived)
                        ),
                    )
                    .check("exposing-decision"),
                );
            }

            findings
        }
        (Scope::Code(kind, format), Some(_)) => vec![stale(format!(
            "its Automator format {format} already makes a {} code",
            kind.label()
        ))],
        (Scope::Excluded(reason), Some(_)) => {
            vec![stale(format!("cannot hold free text: {reason}"))]
        }
        (Scope::Code(..) | Scope::Excluded(_), None) => Vec::new(),
    }
}

/// The configuration-key findings of one entry, whose options line is `entry_line`: a reference
/// on an entry that is no `CONFIG_KEY` code, a reference that repeats a source, names an
/// unaccepted decision, has no reason or names no string property of a configuration or secret
/// contract, and a configuration-key field of the source without one.
fn references(
    typescript: &TypeScript<'_>,
    roots: &[String],
    accepted: &dyn Fn(&str) -> bool,
    entry: &Entry,
    field: Option<&Field>,
    entry_line: Option<u32>,
    options_path: &str,
) -> gate_sdk::Result<Vec<Finding>> {
    let key = entry.key();
    let at_entry = |aspect: &'static str, message: String| {
        Finding::about(aspect, options_path, entry_line, message, &key)
    };
    let mut findings = Vec::new();

    if !entry.is_config_key() {
        if !entry.reference.is_empty() {
            findings.push(at_entry(
                "config-key-field",
                format!(
                    "entry {key} lists references, which only a CODE entry of kind CONFIG_KEY \
                     carries"
                ),
            ));
        }

        return Ok(findings);
    }

    let mut sources = BTreeSet::new();

    for reference in &entry.reference {
        let repeated = !sources.insert(reference.source.as_str());
        reference_findings(
            typescript,
            roots,
            accepted,
            reference,
            repeated,
            &mut |check: &str, message: String| {
                findings.push(
                    at_entry("config-key-source", format!("entry {key} {message}"))
                        .check(&format!("{}#{check}", reference.source)),
                );
            },
        )?;
    }

    if let Some(field) = field
        && field.scope == Scope::Text
        && entry.reference.is_empty()
    {
        findings.push(Finding::about(
            "config-key-unreferenced",
            key.path.clone(),
            Some(field.line),
            format!(
                "{key} is a configuration key that no configuration or secret contract references"
            ),
            &key,
        ));
    }

    Ok(findings)
}

/// The problems of one reference of a `CONFIG_KEY` entry, each said after the entry: a repeated
/// source, an unaccepted decision, a missing reason, a source that names no string property of a
/// configuration or secret contract.
fn reference_findings(
    typescript: &TypeScript<'_>,
    roots: &[String],
    accepted: &dyn Fn(&str) -> bool,
    reference: &Reference,
    repeated: bool,
    problem: &mut dyn FnMut(&str, String),
) -> gate_sdk::Result<()> {
    let source = &reference.source;

    if repeated {
        problem("repeated", format!("lists the reference {source:?} twice"));
    }

    if !accepted(&reference.decision) {
        problem(
            "decision",
            format!(
                "references {source:?} under {}, which is not an accepted decision",
                reference.decision
            ),
        );
    }

    if reference.reason.is_empty() {
        problem("reason", format!("references {source:?} without a reason"));
    }

    if let Some(found) = reference::problem(typescript, roots, source)? {
        problem("source", format!("references {source:?}, which {found}"));
    }

    Ok(())
}

/// The target of an `unclassified` finding: the field and the planes it reaches, sorted, so an
/// exception for it stops matching once the field reaches another plane.
fn pinned(key: &Key, derived: &BTreeSet<Plane>) -> String {
    let planes: Vec<&str> = derived.iter().map(|plane| plane.label()).collect();

    format!("{}:{key} [{}]", key.path, planes.join(","))
}

/// A plane set as a message prints it.
pub(crate) fn planes(set: &BTreeSet<Plane>) -> String {
    if set.is_empty() {
        return "no plane".to_owned();
    }

    set.iter()
        .map(|plane| plane.label())
        .collect::<Vec<_>>()
        .join(", ")
}

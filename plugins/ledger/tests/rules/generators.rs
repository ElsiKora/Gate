//! The generators `decision-counts` and `review-progress`: the blocks byte for byte, the problems
//! of a roadmap with their lines, and option errors.

use std::fmt::Write as _;

use gate_sdk::{GenerateOutput, Location};

use crate::{Files, IMPLEMENTATION};

const INDEX: &str = "docs/ledger/decision-index.md";
const ROADMAP: &str = "docs/ledger/plans/roadmap.md";
const KINDS: &str = r#""cross-cutting-kinds": ["RET", "REPORT"], "zero-answer-kinds": ["PROCESS"]"#;

/// An index with four rows: a repeated identifier whose first row is accepted, and one decision
/// superseded.
const INDEX_TEXT: &str = "\
# Decisions

<!-- gate:begin decision-counts -->
- Decision IDs: `0`
<!-- gate:end decision-counts -->

| ID | Status | Title | File |
| --- | --- | --- | --- |
| `D-ONE-001` | `accepted` | One | `decisions/one.md` |
| `D-TWO-001` | `superseded by D-THREE-001` | Two | `decisions/two.md` |
| `D-THREE-001` | `accepted` | Three | `decisions/three.md` |
| `D-THREE-001` | `superseded` | Three again | `decisions/three.md` |
";

const IMPLEMENTATION_TEXT: &str = "\
# Implementation

- `I-ONE-001`: One
  - Status: `completed`
- `I-TWO-001`: Two
  - Status: `in progress`
- `I-THREE-001`: Three
  - Status: `planned`
";

/// A roadmap every count reads cleanly from.
const ROADMAP_TEXT: &str = "\
# Roadmap

## Review Progress

<!-- gate:begin review-progress -->
stale
<!-- gate:end review-progress -->

## Feature Inventory

#### F001: Alpha

- Review: `implement`

#### F002: Beta

- Review: `awaiting answer`

#### F003: Gamma

- Review: `unreviewed`

```md
#### F004: Inside a fence
```

## Decision Journal

### First choice

Counts: 2 answers · F001

### Process note

Counts: 0 answers · PROCESS

### Report shape

Counts: 1 answer · REPORT

## Accepted Execution Queue

| Order | Slice | Record |
| --- | --- | --- |
| 1 | first | `I-ONE-001` |
| 2 | second | `I-TWO-001` |
| 3 | third | `I-THREE-001` |
";

fn ledger() -> Files {
    Files::new(&[
        (INDEX, INDEX_TEXT),
        (IMPLEMENTATION, IMPLEMENTATION_TEXT),
        (ROADMAP, ROADMAP_TEXT),
    ])
}

fn generate(name: &str, options: &str, files: &Files) -> gate_sdk::Result<GenerateOutput> {
    gate_sdk::generate(&ledger::PLUGIN, files, name, "", options, "")
}

/// The written files as text.
fn written(output: &GenerateOutput) -> Vec<(String, String)> {
    output
        .files
        .iter()
        .map(|file| {
            (
                file.path.clone(),
                String::from_utf8(file.contents.clone()).expect("UTF-8"),
            )
        })
        .collect()
}

/// The findings as `(path, line, message)`.
fn problems(output: &GenerateOutput) -> Vec<(String, Option<u32>, String)> {
    output
        .findings
        .iter()
        .map(|finding| {
            let Some(Location::File(location)) = &finding.location else {
                panic!("a generator finding names a file: {:?}", finding.location);
            };
            (
                location.path.clone(),
                location.span.map(|span| span.line),
                finding.message.clone(),
            )
        })
        .collect()
}

/// The one-based line of the only line of `text` that is `wanted`.
fn line(text: &str, wanted: &str) -> Option<u32> {
    let found: Vec<usize> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| *line == wanted)
        .map(|(index, _)| index + 1)
        .collect();
    assert_eq!(found.len(), 1, "{wanted:?} is one line of the text");
    found
        .first()
        .map(|line| u32::try_from(*line).expect("small"))
}

#[test]
fn decision_counts_count_every_row_and_the_superseded_decisions_at_their_first_row() {
    let output = generate("decision-counts", "{}", &ledger()).expect("runs");
    let expected = INDEX_TEXT.replace(
        "<!-- gate:begin decision-counts -->\n- Decision IDs: `0`\n",
        "<!-- gate:begin decision-counts -->\n\n- Decision IDs: `4`\n- Superseded decisions: `1`\n\n",
    );
    assert_eq!(written(&output), [(INDEX.to_owned(), expected)]);
    assert_eq!(problems(&output), []);
}

#[test]
fn decision_counts_write_nothing_for_an_untracked_index_and_report_a_missing_block() {
    let untracked = generate("decision-counts", "{}", &ledger().without(INDEX)).expect("runs");
    assert_eq!(written(&untracked), []);
    assert_eq!(problems(&untracked), []);

    let other = r#"{"decisions": "docs/index.md"}"#;
    let files = ledger().with("docs/index.md", "| `D-A-001` | `accepted` | A | `a.md` |\n");
    let missing = generate("decision-counts", other, &files).expect("runs");
    assert_eq!(written(&missing), []);
    assert_eq!(
        problems(&missing),
        [(
            "docs/index.md".to_owned(),
            None,
            "the managed block `<!-- gate:begin decision-counts -->` … `<!-- gate:end decision-counts -->` is missing"
                .to_owned()
        )]
    );

    let repeated = ledger().edit(
        INDEX,
        "<!-- gate:end decision-counts -->\n",
        "<!-- gate:end decision-counts -->\n<!-- gate:end decision-counts -->\n",
    );
    let output = generate("decision-counts", "{}", &repeated).expect("runs");
    assert_eq!(written(&output), []);
    assert_eq!(
        problems(&output),
        [(
            INDEX.to_owned(),
            Some(3),
            "the managed block decision-counts needs exactly one `<!-- gate:begin decision-counts -->` line followed by one `<!-- gate:end decision-counts -->` line"
                .to_owned()
        )]
    );

    assert_eq!(
        generate(
            "decision-counts",
            r#"{"decisions": "./index.md"}"#,
            &ledger()
        )
        .expect_err("not normal")
        .message(),
        "generator decision-counts: [generate] ledger/decision-counts decisions \"./index.md\" is not in normal form; write \"index.md\""
    );
}

/// One row of the aligned progress table.
fn row(label: &str, count: usize) -> String {
    format!("| {label:<58} | {count:<5} |\n")
}

#[test]
fn review_progress_writes_the_table_from_the_journal_inventory_queue_and_index() {
    let options = format!(r#"{{"path": "{ROADMAP}", {KINDS}}}"#);
    let output = generate("review-progress", &options, &ledger()).expect("runs");

    let mut table = row("Metric", 0).replace("| 0     |", "| Count |");
    let _ = writeln!(table, "| {} | {} |", "-".repeat(58), "-".repeat(5));
    for (label, count) in [
        ("Answered product choices", 3),
        ("Feature answers", 2),
        ("RET answers", 0),
        ("REPORT answers", 1),
        ("Features with completed product review", 1),
        ("Features in progress", 1),
        ("Unreviewed features", 1),
        ("Total inventory features", 3),
        (
            "Features remaining to finish, including the active feature",
            2,
        ),
        ("Execution I/V pairs", 3),
        ("Completed I/V pairs", 1),
        ("In-progress I/V pairs", 1),
        ("Planned I/V pairs", 1),
        ("Decision IDs", 4),
        ("Superseded decisions", 1),
    ] {
        table.push_str(&row(label, count));
    }
    let expected = ROADMAP_TEXT.replace(
        "<!-- gate:begin review-progress -->\nstale\n",
        &format!("<!-- gate:begin review-progress -->\n\n{table}\n"),
    );
    assert_eq!(written(&output), [(ROADMAP.to_owned(), expected)]);
    assert_eq!(problems(&output), []);
}

/// A roadmap with one of each problem the generator reports.
const PROBLEMS_TEXT: &str = "\
# Roadmap

<!-- gate:begin review-progress -->
<!-- gate:end review-progress -->

## Feature Inventory

Counts: 1 answer · F001

#### F001: Alpha

- Review: `implement`
- Review: `deferred`

#### F002: Beta

#### F003: Gamma

- Review: `maybe`

#### F001: Alpha again

- Review: `rejected`

## Decision Journal

### Plural

Counts: 2 answer · F001

### Zero

Counts: 0 answers · RET

### Unknown kind

Counts: 3 answers · OTHER

### Prose first

Some prose.
Counts: 1 answer · F001

### Twice

Counts: 1 answer · F001
Counts: 1 answer · F003

### Missing feature

Counts: 1 answer · F009

## Accepted Execution Queue

| 1 | gone | `I-GONE-001` |
| 2 | bare | `I-BARE-001` |
";

#[test]
fn review_progress_reports_what_it_cannot_read_with_its_line_and_still_writes_the_table() {
    let text = PROBLEMS_TEXT;
    let implementation = format!("{IMPLEMENTATION_TEXT}- `I-BARE-001`: Bare\n");
    let files = ledger()
        .with(ROADMAP, text)
        .with(IMPLEMENTATION, &implementation);
    let options = format!(r#"{{"path": "{ROADMAP}", {KINDS}}}"#);
    let output = generate("review-progress", &options, &files).expect("runs");
    let at = |wanted: &str| line(text, wanted);
    let problem = |line: Option<u32>, message: &str| (ROADMAP.to_owned(), line, message.to_owned());

    assert_eq!(
        problems(&output),
        [
            problem(
                Some(8),
                "a `Counts:` line belongs only to a Decision Journal entry"
            ),
            problem(
                at("- Review: `deferred`"),
                "F001 has a second `- Review:` line"
            ),
            problem(
                at("Counts: 2 answer · F001"),
                "use `1 answer`, `<n> answers`, and `0 answers` only with PROCESS"
            ),
            problem(
                at("Counts: 0 answers · RET"),
                "use `1 answer`, `<n> answers`, and `0 answers` only with PROCESS"
            ),
            problem(
                at("Counts: 3 answers · OTHER"),
                "the line is not `Counts: <n> answer(s) · <F###|RET|REPORT|PROCESS>`"
            ),
            problem(
                Some(42),
                "the `Counts:` line must directly follow the journal heading"
            ),
            problem(
                at("Counts: 1 answer · F003"),
                "the journal entry `Twice` has a second `Counts:` line"
            ),
            problem(
                at("#### F002: Beta"),
                "F002 has no `- Review: `<status>`` line"
            ),
            problem(
                at("#### F003: Gamma"),
                "F003 has unknown review status `maybe`"
            ),
            problem(
                at("#### F001: Alpha again"),
                "F001 appears twice in the inventory"
            ),
            problem(
                at("### Plural"),
                "the journal entry `Plural` has no `Counts:` line"
            ),
            problem(
                at("### Zero"),
                "the journal entry `Zero` has no `Counts:` line"
            ),
            problem(
                at("### Unknown kind"),
                "the journal entry `Unknown kind` has no `Counts:` line"
            ),
            problem(
                at("### Missing feature"),
                "`Counts:` names F009 which is not in the inventory"
            ),
            problem(
                at("| 1 | gone | `I-GONE-001` |"),
                "the execution queue names I-GONE-001, which has no implementation record"
            ),
            problem(
                at("| 2 | bare | `I-BARE-001` |"),
                "the execution queue names I-BARE-001, whose implementation record has no status"
            ),
        ]
    );
    assert_eq!(output.files.len(), 1, "the table is still written");
}

#[test]
fn review_progress_reports_an_untracked_roadmap_and_refuses_bad_options() {
    let options = format!(r#"{{"path": "docs/none.md", {KINDS}}}"#);
    let output = generate("review-progress", &options, &ledger()).expect("runs");
    assert_eq!(written(&output), []);
    assert_eq!(
        problems(&output),
        [(
            "docs/none.md".to_owned(),
            None,
            "the configured roadmap is not tracked".to_owned()
        )]
    );

    let error = |options: &str| {
        generate("review-progress", options, &ledger())
            .expect_err("refused")
            .message()
            .to_owned()
    };
    assert!(
        error(r#"{"cross-cutting-kinds": [], "zero-answer-kinds": []}"#).contains("path"),
        "path is required"
    );
    assert_eq!(
        error(&format!(
            r#"{{"path": "{ROADMAP}", "cross-cutting-kinds": ["re t"], "zero-answer-kinds": []}}"#
        )),
        "generator review-progress: [generate] ledger/review-progress kind \"re t\" must be uppercase letters and digits, such as \"RET\""
    );
    let kinds = |cross: &str, zero: &str| {
        error(&format!(
            r#"{{"path": "{ROADMAP}", "cross-cutting-kinds": [{cross}], "zero-answer-kinds": [{zero}]}}"#
        ))
    };
    assert_eq!(
        kinds(r#""F001""#, ""),
        "generator review-progress: [generate] ledger/review-progress kind \"F001\" has the form of a feature (F and three digits); name the kind otherwise"
    );
    let twice = "generator review-progress: [generate] ledger/review-progress kind \"RET\" is listed twice; list each kind once, in cross-cutting-kinds or in zero-answer-kinds";
    assert_eq!(kinds(r#""RET", "RET""#, ""), twice);
    assert_eq!(kinds(r#""RET""#, r#""RET""#), twice);
}

#[test]
fn a_configured_kind_that_starts_with_f_is_not_a_feature() {
    let files = ledger().edit(
        ROADMAP,
        "Counts: 1 answer · REPORT",
        "Counts: 1 answer · FIX",
    );
    let options = format!(
        r#"{{"path": "{ROADMAP}", "cross-cutting-kinds": ["FIX"], "zero-answer-kinds": ["PROCESS"]}}"#
    );
    let output = generate("review-progress", &options, &files).expect("runs");
    assert_eq!(
        problems(&output),
        [],
        "FIX is a kind, not a feature missing from the inventory"
    );
    let table = &written(&output)[0].1;
    assert!(table.contains(&row("Feature answers", 2)), "{table}");
    assert!(table.contains(&row("FIX answers", 1)), "{table}");
}

#[test]
fn an_untracked_decision_index_is_a_finding_and_counts_zero() {
    let options = format!(r#"{{"path": "{ROADMAP}", {KINDS}}}"#);
    let output = generate("review-progress", &options, &ledger().without(INDEX)).expect("runs");
    assert_eq!(
        problems(&output),
        [(
            INDEX.to_owned(),
            None,
            "the configured decision index is not tracked".to_owned()
        )]
    );
    let table = &written(&output)[0].1;
    assert!(table.contains(&row("Decision IDs", 0)), "{table}");
}

#[test]
fn the_generators_own_no_glob() {
    let info = gate_sdk::describe(&ledger::PLUGIN);
    let generators: Vec<(&str, usize)> = info
        .generators
        .iter()
        .map(|generator| (generator.name.as_str(), generator.owns.len()))
        .collect();
    assert_eq!(generators, [("decision-counts", 0), ("review-progress", 0)]);
}

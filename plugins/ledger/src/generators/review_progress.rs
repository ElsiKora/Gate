//! `ledger/review-progress`: the review progress of a roadmap, in its managed block
//! `review-progress`. It counts the answers of the decision journal's `Counts:` lines, the review
//! statuses of the feature inventory, the records of the execution queue with their implementation
//! statuses, and the decision index; what it cannot read is a finding with its line.

use std::collections::{BTreeMap, BTreeSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Generate, Generator, Result};
use regex::Regex;

use super::decision_counts::{counts as decision_counts, default_decisions};
use super::{at, write_block};
use crate::block;
use crate::catalog::Catalog;
use crate::records::{Ledger, compile};
use crate::settings::{Settings, check_path};

/// The name of the managed block.
pub const BLOCK: &str = "review-progress";

/// The generator.
#[derive(Debug, Default)]
pub struct ReviewProgress;

/// The options of `ledger/review-progress`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The roadmap that holds the block, relative to the project.
    pub path: String,
    /// The decision index whose rows and superseded decisions are counted.
    #[serde(default = "default_decisions")]
    pub decisions: String,
    /// The entry file of the implementation records; the Markdown files of its same-stem directory
    /// belong to it.
    #[serde(default = "default_implementation")]
    pub implementation: String,
    /// The answer kinds besides features (`F` and three digits), each with its own row
    /// `<KIND> answers`, in this order.
    pub cross_cutting_kinds: Vec<String>,
    /// The answer kinds that record no answer: only they may say `0 answers`, and they always do.
    pub zero_answer_kinds: Vec<String>,
    /// The heading of the decision journal, whose `###` entries carry the `Counts:` lines.
    #[serde(default = "default_journal")]
    pub journal_heading: String,
    /// The heading of the execution queue, whose table rows name implementation records.
    #[serde(default = "default_queue")]
    pub queue_heading: String,
    /// The review statuses of a feature whose product review is complete.
    #[serde(default = "default_completed")]
    pub completed_reviews: Vec<String>,
    /// The review statuses of a feature in review.
    #[serde(default = "default_in_progress")]
    pub in_progress_reviews: Vec<String>,
    /// The review status of a feature not yet reviewed.
    #[serde(default = "default_unreviewed")]
    pub unreviewed_review: String,
}

fn default_implementation() -> String {
    Settings::default().implementation
}

fn default_journal() -> String {
    "## Decision Journal".to_owned()
}

fn default_queue() -> String {
    "## Accepted Execution Queue".to_owned()
}

fn default_completed() -> Vec<String> {
    ["retain existing", "implement", "deferred", "rejected"]
        .map(str::to_owned)
        .to_vec()
}

fn default_in_progress() -> Vec<String> {
    ["awaiting answer", "clarification needed"]
        .map(str::to_owned)
        .to_vec()
}

fn default_unreviewed() -> String {
    "unreviewed".to_owned()
}

impl Generator for ReviewProgress {
    type Options = Options;

    const NAME: &'static str = "review-progress";
    const TITLE: &'static str = "The review progress table in the managed block of a roadmap";
    /// It writes only the roadmap.
    const OWNS: &'static [&'static str] = &[];

    fn generate(&self, run: &mut Generate<'_, Options>) -> Result<()> {
        let options = run.options();
        for (name, path) in [
            ("path", &options.path),
            ("decisions", &options.decisions),
            ("implementation", &options.implementation),
        ] {
            check_path(&format!("[generate] ledger/review-progress {name}"), path)?;
        }
        let grammar = Grammar::new(options)?;
        let path = options.path.clone();
        let tree = run.tree();

        let Some(text) = tree.text(&path) else {
            run.report()
                .finding("", at(&path, None), "the configured roadmap is not tracked")
                .target(path.clone())
                .help("track the roadmap, or correct `path` in [generate] ledger/review-progress");
            return Ok(());
        };
        let model = parse(&text, &grammar, options);
        let mut problems = model.problems.clone();

        let settings = Settings {
            implementation: options.implementation.clone(),
            ..Settings::default()
        };
        let ledger = Ledger::read(&Catalog::new(tree), &settings)?;
        let pairs = queue_pairs(&model, &ledger, &mut problems);
        let index = tree.text(&options.decisions);
        let decisions = index.as_deref().map_or((0, 0), decision_counts);
        let untracked_index = index.is_none().then(|| options.decisions.clone());
        let inner = render(&rows(&model, options, pairs, decisions));

        if let Some(index) = untracked_index {
            run.report()
                .finding(
                    "",
                    at(&index, None),
                    "the configured decision index is not tracked",
                )
                .target(index)
                .help(
                    "track the decision index, or correct `decisions` in [generate] \
                     ledger/review-progress; its counts are written as 0",
                );
        }
        for problem in problems {
            run.report()
                .finding("", at(&path, problem.line), problem.message)
                .target(path.clone())
                .help("correct the roadmap; the table is written from what could be read");
        }
        write_block(run, &path, &text, BLOCK, &inner);
        Ok(())
    }
}

/// The queue's records by implementation status (completed or done, any other, planned); a row
/// whose record is missing or has no status is a problem at the row's line.
fn queue_pairs(model: &Model, ledger: &Ledger, problems: &mut Vec<Problem>) -> [usize; 3] {
    let mut pairs = [0_usize; 3];
    for (id, line) in &model.queue {
        let message = match ledger.status(id) {
            Some("completed" | "done") => {
                pairs[0] += 1;
                continue;
            }
            Some("planned") => {
                pairs[2] += 1;
                continue;
            }
            Some(_) => {
                pairs[1] += 1;
                continue;
            }
            None if ledger.implementation.iter().any(|record| record.id == *id) => {
                format!("the execution queue names {id}, whose implementation record has no status")
            }
            None => format!("the execution queue names {id}, which has no implementation record"),
        };
        problems.push(Problem {
            line: Some(*line),
            message,
        });
    }
    pairs
}

/// Whether an answer kind is a feature: `F` and three digits.
fn is_feature(kind: &str) -> bool {
    kind.strip_prefix('F')
        .is_some_and(|digits| digits.len() == 3 && digits.bytes().all(|byte| byte.is_ascii_digit()))
}

/// The patterns of a roadmap, with the configured kinds.
struct Grammar {
    counts: Regex,
    feature: Regex,
    review: Regex,
    queue_row: Regex,
    /// The `Counts:` line as a message shows it.
    counts_form: String,
    /// The zero-answer kinds as a message names them.
    zero_form: String,
}

impl Grammar {
    fn new(options: &Options) -> Result<Self> {
        let kinds: Vec<&String> = options
            .cross_cutting_kinds
            .iter()
            .chain(&options.zero_answer_kinds)
            .collect();
        if let Some(kind) = kinds.iter().find(|kind| {
            kind.is_empty()
                || !kind
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        }) {
            return Err(Error::config(format!(
                "[generate] ledger/review-progress kind {kind:?} must be uppercase letters and digits, such as \"RET\""
            )));
        }
        if let Some(kind) = kinds.iter().find(|kind| is_feature(kind)) {
            return Err(Error::config(format!(
                "[generate] ledger/review-progress kind {kind:?} has the form of a feature (F and three digits); name the kind otherwise"
            )));
        }
        let mut seen = BTreeSet::new();
        if let Some(kind) = kinds.iter().find(|kind| !seen.insert(kind.as_str())) {
            return Err(Error::config(format!(
                "[generate] ledger/review-progress kind {kind:?} is listed twice; list each kind once, in cross-cutting-kinds or in zero-answer-kinds"
            )));
        }
        let alternatives: Vec<String> = std::iter::once("F[0-9]{3}".to_owned())
            .chain(kinds.iter().map(|kind| regex::escape(kind)))
            .collect();
        let shown: Vec<&str> = std::iter::once("F###")
            .chain(kinds.iter().map(|kind| kind.as_str()))
            .collect();
        Ok(Self {
            counts: compile(&format!(
                r"^Counts: (0|[1-9][0-9]*) (answers?) · ({})$",
                alternatives.join("|")
            ))?,
            feature: compile(r"^#### (F[0-9]{3}): (.+)$")?,
            review: compile(r"^- Review: `([^`]+)`")?,
            queue_row: compile(r"^\| *[0-9]+ *\|[^|]*\| *`(I-[A-Z0-9-]+)`")?,
            counts_form: format!("Counts: <n> answer(s) · <{}>", shown.join("|")),
            zero_form: options.zero_answer_kinds.join(" or "),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Counts {
    answers: u32,
    kind: String,
}

#[derive(Clone, Debug)]
struct Entry {
    heading: String,
    line: usize,
    counts: Option<Counts>,
}

#[derive(Clone, Debug)]
struct Feature {
    id: String,
    line: usize,
    review: Option<String>,
}

/// Something the roadmap does not say the way the generator reads it.
#[derive(Clone, Debug)]
struct Problem {
    line: Option<usize>,
    message: String,
}

/// What a roadmap says.
#[derive(Debug, Default)]
struct Model {
    entries: Vec<Entry>,
    features: Vec<Feature>,
    /// The record of each queue row, with the row's line.
    queue: Vec<(String, usize)>,
    problems: Vec<Problem>,
}

impl Model {
    /// The answers per kind.
    fn answers(&self) -> BTreeMap<&str, u32> {
        let mut answers = BTreeMap::new();
        for counts in self
            .entries
            .iter()
            .filter_map(|entry| entry.counts.as_ref())
        {
            *answers.entry(counts.kind.as_str()).or_insert(0) += counts.answers;
        }
        answers
    }
}

/// Reads the journal, the inventory and the execution queue of a roadmap, outside fenced code.
fn parse(text: &str, grammar: &Grammar, options: &Options) -> Model {
    let mut model = Model::default();
    let fenced = fenced_lines(text);
    let mut section = "";
    let mut entry: Option<usize> = None;
    let mut feature: Option<usize> = None;
    let mut entry_has_content = false;

    for (index, line) in text.lines().enumerate() {
        if fenced.get(index).copied().unwrap_or(false) {
            continue;
        }
        let number = index + 1;

        if line.starts_with("## ") {
            section = line;
            entry = None;
            feature = None;
            continue;
        }
        if line.starts_with("### ") || line.starts_with("#### ") {
            entry = None;
            feature = None;
        }
        if section == options.journal_heading
            && let Some(heading) = line.strip_prefix("### ")
        {
            model.entries.push(Entry {
                heading: heading.to_owned(),
                line: number,
                counts: None,
            });
            entry = Some(model.entries.len() - 1);
            entry_has_content = false;
            continue;
        }
        if let Some(captures) = grammar.feature.captures(line) {
            model.features.push(Feature {
                id: captures[1].to_owned(),
                line: number,
                review: None,
            });
            feature = Some(model.features.len() - 1);
            continue;
        }
        if line.starts_with("Counts:") {
            if entry.is_some() && entry_has_content {
                model.problems.push(Problem {
                    line: Some(number),
                    message: "the `Counts:` line must directly follow the journal heading"
                        .to_owned(),
                });
            }
            parse_counts(&mut model, grammar, options, entry, line, number);
            continue;
        }
        if entry.is_some() && !line.trim().is_empty() {
            entry_has_content = true;
        }
        if let Some(position) = feature
            && let Some(captures) = grammar.review.captures(line)
        {
            let current = &mut model.features[position];
            if current.review.is_some() {
                model.problems.push(Problem {
                    line: Some(number),
                    message: format!("{} has a second `- Review:` line", current.id),
                });
            } else {
                current.review = Some(captures[1].to_owned());
            }
        }
        if section == options.queue_heading
            && let Some(captures) = grammar.queue_row.captures(line)
        {
            model.queue.push((captures[1].to_owned(), number));
        }
    }

    validate(&mut model, options);
    model
}

fn parse_counts(
    model: &mut Model,
    grammar: &Grammar,
    options: &Options,
    entry: Option<usize>,
    line: &str,
    number: usize,
) {
    let Some(position) = entry else {
        model.problems.push(Problem {
            line: Some(number),
            message: "a `Counts:` line belongs only to a Decision Journal entry".to_owned(),
        });
        return;
    };
    let Some(captures) = grammar.counts.captures(line) else {
        model.problems.push(Problem {
            line: Some(number),
            message: format!("the line is not `{}`", grammar.counts_form),
        });
        return;
    };
    let answers: u32 = captures[1].parse().unwrap_or(0);
    let kind = captures[3].to_owned();
    let plural_ok = (answers == 1) == (&captures[2] == "answer");
    let zero_ok = options.zero_answer_kinds.contains(&kind) == (answers == 0);
    if !plural_ok || !zero_ok {
        model.problems.push(Problem {
            line: Some(number),
            message: format!(
                "use `1 answer`, `<n> answers`, and `0 answers` only with {}",
                grammar.zero_form
            ),
        });
        return;
    }

    let current = &mut model.entries[position];
    if current.counts.is_some() {
        model.problems.push(Problem {
            line: Some(number),
            message: format!(
                "the journal entry `{}` has a second `Counts:` line",
                current.heading
            ),
        });
    } else {
        current.counts = Some(Counts { answers, kind });
    }
}

fn validate(model: &mut Model, options: &Options) {
    let mut seen = BTreeSet::new();
    let known: BTreeSet<String> = model
        .features
        .iter()
        .map(|feature| feature.id.clone())
        .collect();

    for feature in &model.features {
        if !seen.insert(feature.id.clone()) {
            model.problems.push(Problem {
                line: Some(feature.line),
                message: format!("{} appears twice in the inventory", feature.id),
            });
        }
        match feature.review.as_deref() {
            None => model.problems.push(Problem {
                line: Some(feature.line),
                message: format!("{} has no `- Review: `<status>`` line", feature.id),
            }),
            Some(status) if !is_review(options, status) => model.problems.push(Problem {
                line: Some(feature.line),
                message: format!("{} has unknown review status `{status}`", feature.id),
            }),
            Some(_) => {}
        }
    }

    for entry in &model.entries {
        match &entry.counts {
            None => model.problems.push(Problem {
                line: Some(entry.line),
                message: format!(
                    "the journal entry `{}` has no `Counts:` line",
                    entry.heading
                ),
            }),
            Some(counts) if is_feature(&counts.kind) && !known.contains(&counts.kind) => {
                model.problems.push(Problem {
                    line: Some(entry.line),
                    message: format!(
                        "`Counts:` names {} which is not in the inventory",
                        counts.kind
                    ),
                });
            }
            Some(_) => {}
        }
    }
}

fn is_review(options: &Options, status: &str) -> bool {
    options
        .completed_reviews
        .iter()
        .any(|known| known == status)
        || options
            .in_progress_reviews
            .iter()
            .any(|known| known == status)
        || options.unreviewed_review == status
}

/// The rows of the table: label and count.
fn rows(
    model: &Model,
    options: &Options,
    pairs: [usize; 3],
    decisions: (usize, usize),
) -> Vec<(String, usize)> {
    let answers = model.answers();
    let count = |value: u32| usize::try_from(value).unwrap_or(usize::MAX);
    let total: u32 = answers.values().sum();
    let features: u32 = answers
        .iter()
        .filter(|(kind, _)| is_feature(kind))
        .map(|(_, answers)| answers)
        .sum();
    let reviews = |keep: &dyn Fn(&str) -> bool| {
        model
            .features
            .iter()
            .filter(|feature| feature.review.as_deref().is_some_and(keep))
            .count()
    };
    let completed = reviews(&|review| {
        options
            .completed_reviews
            .iter()
            .any(|known| known == review)
    });
    let in_progress = reviews(&|review| {
        options
            .in_progress_reviews
            .iter()
            .any(|known| known == review)
    });
    let unreviewed = reviews(&|review| review == options.unreviewed_review);

    let mut rows = vec![
        ("Answered product choices".to_owned(), count(total)),
        ("Feature answers".to_owned(), count(features)),
    ];
    rows.extend(options.cross_cutting_kinds.iter().map(|kind| {
        (
            format!("{kind} answers"),
            count(answers.get(kind.as_str()).copied().unwrap_or(0)),
        )
    }));
    rows.extend([
        (
            "Features with completed product review".to_owned(),
            completed,
        ),
        ("Features in progress".to_owned(), in_progress),
        ("Unreviewed features".to_owned(), unreviewed),
        ("Total inventory features".to_owned(), model.features.len()),
        (
            "Features remaining to finish, including the active feature".to_owned(),
            in_progress + unreviewed,
        ),
        ("Execution I/V pairs".to_owned(), model.queue.len()),
        ("Completed I/V pairs".to_owned(), pairs[0]),
        ("In-progress I/V pairs".to_owned(), pairs[1]),
        ("Planned I/V pairs".to_owned(), pairs[2]),
        ("Decision IDs".to_owned(), decisions.0),
        ("Superseded decisions".to_owned(), decisions.1),
    ]);
    rows
}

/// The text of the block.
fn render(rows: &[(String, usize)]) -> String {
    let body: Vec<Vec<String>> = rows
        .iter()
        .map(|(label, count)| vec![label.clone(), count.to_string()])
        .collect();
    block::table(&["Metric", "Count"], &body)
}

/// Whether each line is inside a fenced code block (its fences included): a fence opens with three
/// or more backticks or tildes and closes with at least as many of the same, alone on the line.
fn fenced_lines(text: &str) -> Vec<bool> {
    let mut inside = Vec::new();
    let mut fence: Option<(char, usize)> = None;

    for line in text.lines() {
        let trimmed = line.trim_start();
        let marker = trimmed
            .chars()
            .next()
            .filter(|first| *first == '`' || *first == '~');
        let run = marker.map_or(0, |first| {
            trimmed.chars().take_while(|found| *found == first).count()
        });
        match (fence, marker) {
            (None, Some(first)) if run >= 3 => {
                fence = Some((first, run));
                inside.push(true);
            }
            (Some((first, length)), Some(current))
                if current == first && run >= length && trimmed[run..].trim().is_empty() =>
            {
                fence = None;
                inside.push(true);
            }
            (Some(_), _) => inside.push(true),
            (None, _) => inside.push(false),
        }
    }
    inside
}

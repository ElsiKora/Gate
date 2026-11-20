//! `ledger/closing-notes`: a temporary difference whose exact closer is closed carries one dated
//! closing note, and one whose closer is open carries none.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Rule, Run};
use regex::Regex;

use crate::catalog::{Catalog, Findings};
use crate::input::{self, INPUT};
use crate::records::{Ledger, Record, compile};
use crate::settings::Settings;

/// The rule.
#[derive(Debug, Default)]
pub struct ClosingNotes;

/// The options of `ledger/closing-notes`. `field` is required: the field is the project's own, and
/// without it the rule would check nothing.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The record field that lists the temporary differences, such as
    /// ``"`TEMPORARY_DELTA` records"``.
    #[schemars(length(min = 1))]
    pub field: String,
    /// Letters an entry label may carry before its number: with `["A"]`, both `(2)` and `(A2)`
    /// start an entry.
    #[serde(default)]
    pub entry_label_letters: Vec<String>,
}

impl Rule for ClosingNotes {
    type Options = Options;

    const NAME: &'static str = "closing-notes";
    const TITLE: &'static str =
        "A temporary difference whose exact closer is closed carries a dated closing note";
    const ASPECTS: &'static [&'static str] = &[INPUT];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let settings = Settings::read(run)?;
        let tree = run.tree();
        let products = settings
            .parity
            .as_ref()
            .map(|parity| parity.products.as_slice())
            .unwrap_or_default();

        if run.options().field.trim().is_empty() {
            return Err(Error::config(
                "ledger/closing-notes option field is empty; name the record field that lists the temporary differences",
            ));
        }

        let patterns = Patterns::new(run.options(), products)?;

        if !input::tracked(run.report(), &tree, Self::NAME, &input::records(&settings)) {
            return Ok(());
        }

        let name = entry_name(&run.options().field);
        let ledger = Ledger::read(&Catalog::new(tree), &settings)?;
        let mut findings = Findings::default();

        for record in ledger.implementation.iter().chain(&ledger.plan) {
            for entry in entries(record, &patterns) {
                let Some(closer) = patterns
                    .closer
                    .captures(&entry.text)
                    .and_then(|captures| captures.get(1))
                    .map(|closer| closer.as_str())
                else {
                    continue;
                };
                let Some(status) = ledger.status(closer) else {
                    continue;
                };
                let notes = patterns.note.find_iter(&entry.text).count();
                let message = match (settings.is_settled(status), notes) {
                    (true, 0) => format!(
                        "has a {name} entry whose exact closer {closer} is `{status}`; add the note `Closed (YYYY-MM-DD) by the merge of `I-...` as `<full OID>`` naming the merge that closed it"
                    ),
                    (true, 1) | (false, 0) => continue,
                    (true, _) => format!(
                        "has a {name} entry with {notes} closing notes; keep the one note naming the merge that closed it, or split the entry at labels"
                    ),
                    (false, _) => format!(
                        "has a {name} entry with a closing note while its exact closer {closer} is still `{status}`; remove the note until the closer closes, or name the record that closed the difference as its closer"
                    ),
                };
                let label = patterns
                    .label
                    .find(&entry.text)
                    .map_or("", |label| label.as_str());
                findings.emit_keyed(
                    "",
                    &record.location.below(entry.line),
                    format!("{}/{closer}{label}", record.id),
                    format!("{} {} {message}", record.location.path, record.id),
                );
            }
        }

        findings.write_all(run.report());
        Ok(())
    }
}

/// The name of an entry in messages: the field's first backticked word, or the field.
fn entry_name(field: &str) -> &str {
    field
        .strip_prefix('`')
        .and_then(|rest| rest.split_once('`'))
        .map_or(field, |(name, _)| name)
}

struct Patterns {
    field: Regex,
    item: Regex,
    label: Regex,
    closer: Regex,
    note: Regex,
}

impl Patterns {
    fn new(options: &Options, products: &[String]) -> gate_sdk::Result<Self> {
        let id = "I-[A-Z0-9]+(?:-[A-Z0-9]+)*";
        let letters = options
            .entry_label_letters
            .iter()
            .map(|letter| {
                if letter.len() == 1 && letter.bytes().all(|byte| byte.is_ascii_uppercase()) {
                    Ok(letter.as_str())
                } else {
                    Err(Error::config(format!(
                        "ledger/closing-notes option entry-label-letters has {letter:?}; each entry is one uppercase letter"
                    )))
                }
            })
            .collect::<gate_sdk::Result<String>>()?;
        let letters = if letters.is_empty() {
            String::new()
        } else {
            format!("[{letters}]?")
        };
        let field = crate::text::literal(&options.field);
        let boundary = if options
            .field
            .chars()
            .last()
            .is_some_and(|last| last.is_ascii_alphanumeric() || last == '_')
        {
            r"\b"
        } else {
            ""
        };
        let products: Vec<String> = products
            .iter()
            .map(|product| crate::text::literal(product))
            .collect();
        let (closed_for, before_id, note_product) = if products.is_empty() {
            (String::new(), String::new(), String::new())
        } else {
            let any = products.join("|");
            (
                format!("(?: for (?:{any}))?"),
                format!(r"(?:(?:{any})\s+)?"),
                format!(
                    "(?:{})?",
                    products
                        .iter()
                        .map(|product| format!("{product} "))
                        .collect::<Vec<_>>()
                        .join("|")
                ),
            )
        };

        Ok(Self {
            field: compile(&format!(r"^( *)- (?:\*\*)?({field}{boundary}.*)$"))?,
            item: compile(r"^( *)- (.*)$")?,
            label: compile(&format!(r"\({letters}[1-9][0-9]?\)"))?,
            closer: compile(&format!(
                r"(?i:\b(?:exact closer|closer|closing slice|closing(?: exactly)? in|closes(?: only)? in|close in|closed{closed_for} by|closing)\b):?\s+{before_id}`({id})`"
            ))?,
            note: compile(&format!(
                r"Closed \([0-9]{{4}}-(?:0[1-9]|1[0-2])-(?:0[1-9]|[12][0-9]|3[01])\) by the merge of {note_product}`{id}` as `[0-9a-f]{{40}}`"
            ))?,
        })
    }
}

/// One entry of a field: its text and the line, counted from the record line, on which it starts.
struct Entry {
    text: String,
    line: u32,
}

/// One piece of the field: the joined text of its lines and, for each line, the offset in the text
/// at which it starts and its line counted from the record line.
struct Piece {
    text: String,
    lines: Vec<(usize, u32)>,
}

impl Piece {
    fn new(text: &str, line: u32) -> Self {
        Self {
            text: text.trim().to_owned(),
            lines: vec![(0, line)],
        }
    }

    fn push(&mut self, text: &str, line: u32) {
        self.text.push(' ');
        self.lines.push((self.text.len(), line));
        self.text.push_str(text.trim());
    }

    fn line(&self, offset: usize) -> u32 {
        self.lines
            .iter()
            .take_while(|(start, _)| *start <= offset)
            .last()
            .map_or(0, |(_, line)| *line)
    }
}

/// The entries of every listed field of a record: the field line, from the field name on, with
/// the deeper lines below it up to a blank line or a line no deeper than the field line; a list
/// item one level deeper starts a new piece, and any other deeper line continues the piece above.
fn entries(record: &Record, patterns: &Patterns) -> Vec<Entry> {
    let lines: Vec<&str> = record.section.split('\n').collect();
    let mut entries = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let Some(captures) = patterns.field.captures(lines[index]) else {
            index += 1;
            continue;
        };
        let indent = captures[1].len();
        let field = &captures[2];
        let none_led = field.split_once(':').is_some_and(|(_, value)| {
            value
                .trim_start_matches('*')
                .trim_start()
                .starts_with("none")
        });
        let mut pieces = vec![Piece::new(field, line_number(index))];
        index += 1;

        while let Some(line) = lines.get(index) {
            let depth = line.len() - line.trim_start().len();

            if line.trim().is_empty() || depth <= indent {
                break;
            }

            match patterns.item.captures(line) {
                Some(item) if item[1].len() == indent + 2 => {
                    pieces.push(Piece::new(&item[2], line_number(index)));
                }
                _ => {
                    if let Some(piece) = pieces.last_mut() {
                        piece.push(line, line_number(index));
                    }
                }
            }

            index += 1;
        }

        for (number, piece) in pieces.iter().enumerate() {
            entries.extend(split(piece, number == 0 && none_led, &patterns.label));
        }
    }

    entries
}

/// A section line index as a line counted from the record line.
fn line_number(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

/// Splits a piece at its entry labels, which start the piece or follow `. `, `; ` or `: `, dropping
/// the text before the first label when it only says that no difference remains.
fn split(piece: &Piece, skip_leading: bool, label: &Regex) -> Vec<Entry> {
    let bytes = piece.text.as_bytes();
    let mut starts: Vec<usize> = label
        .find_iter(&piece.text)
        .map(|found| found.start())
        .filter(|&start| {
            start == 0
                || (start >= 2
                    && bytes[start - 1] == b' '
                    && matches!(bytes[start - 2], b'.' | b';' | b':'))
        })
        .collect();
    let leading = starts.first() != Some(&0);

    if leading {
        starts.insert(0, 0);
    }

    let mut ends = starts.iter().skip(1).copied().collect::<Vec<_>>();
    ends.push(piece.text.len());

    starts
        .iter()
        .zip(ends)
        .enumerate()
        .filter(|(index, _)| !(leading && skip_leading && *index == 0))
        .map(|(_, (&start, end))| Entry {
            text: piece.text[start..end].to_owned(),
            line: piece.line(start),
        })
        .collect()
}

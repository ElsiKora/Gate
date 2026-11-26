//! `ledger/exception-owners`: the owner an exception names is an open implementation record. The
//! rule runs in the repository call and judges every exception of every `gate.exceptions.toml`
//! of the workspace once, against the records the repository call's `[plugins.ledger] options`
//! name. The decisions exceptions name are `ledger/config-decisions`'s.

use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, NoOptions, Rule, Run, Scope, Tree};

use crate::catalog::{At, Findings};
use crate::rules::config_decisions::is_exceptions_file;
use crate::settings::Settings;
use crate::text::LineIndex;

/// One `[[exception]]` as this rule reads it; the host checks the rest of its shape.
#[derive(Debug, Deserialize)]
#[serde(crate = "gate_sdk::serde")]
struct Exception {
    rule: String,
    owner: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(crate = "gate_sdk::serde")]
struct File {
    #[serde(default)]
    exception: Vec<toml::Spanned<Exception>>,
}

/// An exception with the line it starts on.
struct Located {
    at: At,
    exception: Exception,
}

/// The exceptions of the file at `path`, each at its line; empty when the file is not tracked.
fn exceptions(tree: &Tree<'_>, path: &str) -> gate_sdk::Result<Vec<Located>> {
    let Some(text) = tree.text(path) else {
        return Ok(Vec::new());
    };
    let file: File = toml::from_str(&text).map_err(|error| {
        Error::config(format!(
            "{path} is not a valid exceptions file: {}; fix it so the host can load it",
            error.message()
        ))
    })?;
    let lines = LineIndex::new(&text);

    Ok(file
        .exception
        .into_iter()
        .map(|spanned| {
            let line = lines.line(spanned.span().start);
            Located {
                at: At::line(path, line),
                exception: spanned.into_inner(),
            }
        })
        .collect())
}

/// The rule `ledger/exception-owners`. It learns the exceptions files from the host
/// ([`Run::rule_decisions`]): the repository call lists every exception of every
/// `gate.exceptions.toml` the core read, whether or not it names a decision, and a project's call
/// lists none, so each exception is judged once, at its file and line.
#[derive(Debug, Default)]
pub struct ExceptionOwners;

impl Rule<crate::LedgerCall> for ExceptionOwners {
    type Options = NoOptions;

    const NAME: &'static str = "exception-owners";
    const TITLE: &'static str = "The owner an exception names is an open implementation record";
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, NoOptions, crate::LedgerCall>) -> gate_sdk::Result<()> {
        let mut files: Vec<String> = Vec::new();
        for written in run.rule_decisions() {
            let path = &written.at.path;
            if is_exceptions_file(path) && !files.contains(path) {
                files.push(path.clone());
            }
        }
        if files.is_empty() {
            return Ok(());
        }
        judge_owners(run, &files)
    }
}

/// Reports each exception of `files` whose owner is not an open implementation record.
fn judge_owners(
    run: &mut Run<'_, NoOptions, crate::LedgerCall>,
    files: &[String],
) -> gate_sdk::Result<()> {
    run.with_call(|run, shared| {
        let tree = run.tree();
        let mut exceptions = Vec::new();
        for path in files {
            exceptions.extend(self::exceptions(&tree, path)?);
        }

        if exceptions
            .iter()
            .all(|located| located.exception.owner.is_none())
        {
            return Ok(());
        }

        let settings = Settings::read(run)?;
        let ledger = shared.ledger(&settings)?;
        let implementation = settings.implementation.as_str();
        let mut findings = Findings::default();

        for Located { at, exception } in &exceptions {
            let Some(owner) = exception.owner.as_deref().map(str::trim) else {
                continue;
            };
            let rule = &exception.rule;
            let record = ledger
                .implementation
                .iter()
                .find(|record| record.id == owner);

            match record {
                None => findings.emit_keyed(
                    "",
                    at,
                    format!("{rule}/{owner}#unlisted"),
                    format!(
                        "the exception for {rule} names the owner {owner}, which {implementation} does not list; name an open implementation record, or remove the owner"
                    ),
                ),
                Some(record) => match record.status.as_deref() {
                    Some(status) if settings.is_open(status) => {}
                    status => {
                        let state = status.map_or_else(
                            || "which has no status".to_owned(),
                            |status| format!("which is `{status}`"),
                        );
                        findings.emit_keyed(
                            "",
                            at,
                            format!("{rule}/{owner}#closed"),
                            format!(
                                "the exception for {rule} names the owner {owner}, {state}; only an open record, planned or in progress, owns an exception, so fix the finding, or move the exception to the open record that owns the work"
                            ),
                        );
                    }
                },
            }
        }

        findings.write_all(run.report());
        Ok(())
    })
}

//! `ledger/exception-owners`: the owner an exception names is an open implementation record.
//! `ledger/root-exception-owners` judges the root's exceptions the same way, in the repository
//! call, when the root is not a project. The decisions exceptions name are
//! `ledger/config-decisions`'s.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Rule, Run, Scope, Tree};

use crate::catalog::{At, Findings};
use crate::settings::{Settings, check_path};
use crate::text::LineIndex;

/// The options of both owner rules.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The exceptions file: the project's, relative to its root; for the root rules, relative to
    /// the directory of the scope's own `gate.toml`.
    pub exceptions: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            exceptions: "gate.exceptions.toml".to_owned(),
        }
    }
}

/// One `[[exception]]` as these rules read it; the host checks the rest of its shape.
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

/// The exceptions of the file, each at its line; empty when the file is not tracked.
fn exceptions(tree: &Tree<'_>, path: &str, rule: &str) -> gate_sdk::Result<Vec<Located>> {
    check_path(&format!("ledger/{rule} option exceptions"), path)?;
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

/// The rule `ledger/exception-owners`.
#[derive(Debug, Default)]
pub struct ExceptionOwners;

impl Rule<crate::LedgerCall> for ExceptionOwners {
    type Options = Options;

    const NAME: &'static str = "exception-owners";
    const TITLE: &'static str = "The owner an exception names is an open implementation record";

    fn check(&self, run: &mut Run<'_, Options, crate::LedgerCall>) -> gate_sdk::Result<()> {
        let path = run.options().exceptions.clone();
        judge_owners(run, &path, Self::NAME)
    }
}

/// The rule `ledger/root-exception-owners`: `ledger/exception-owners` for the root's exceptions.
#[derive(Debug, Default)]
pub struct RootExceptionOwners;

impl Rule<crate::LedgerCall> for RootExceptionOwners {
    type Options = Options;

    const NAME: &'static str = "root-exception-owners";
    const TITLE: &'static str = "The owner a root exception names is an open implementation record";
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options, crate::LedgerCall>) -> gate_sdk::Result<()> {
        match root_exceptions(run) {
            Some(path) => judge_owners(run, &path, Self::NAME),
            None => Ok(()),
        }
    }
}

/// The exceptions file beside the scope's own `gate.toml`, when the call has one: beside the root
/// file in the repository call of a root that is not a project; `None` under a root project, whose
/// root file the project rules read.
fn root_exceptions(run: &Run<'_, Options, crate::LedgerCall>) -> Option<String> {
    let exceptions = &run.options().exceptions;
    let config = run.config_file()?;
    Some(match config.rsplit_once('/') {
        Some((directory, _)) => format!("{directory}/{exceptions}"),
        None => exceptions.clone(),
    })
}

/// Reports each exception of `path` whose owner is not an open implementation record: the one
/// judgement of `ledger/exception-owners` and `ledger/root-exception-owners`.
fn judge_owners(
    run: &mut Run<'_, Options, crate::LedgerCall>,
    path: &str,
    name: &str,
) -> gate_sdk::Result<()> {
    run.with_call(|run, shared| {
        let settings = Settings::read(run)?;
        let tree = run.tree();
        let exceptions = exceptions(&tree, path, name)?;

        if exceptions
            .iter()
            .all(|located| located.exception.owner.is_none())
        {
            return Ok(());
        }

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

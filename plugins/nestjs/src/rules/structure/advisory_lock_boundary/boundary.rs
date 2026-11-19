//! The boundaries of executable advisory SQL: the executor, whose every call is one of the advisory
//! functions, qualified with `pg_catalog`, with `$1::integer, $2::integer`; each baseline migration, with exactly one
//! `pg_catalog.pg_advisory_xact_lock` of the ledger static key; and the negative test, with exactly
//! one `pg_catalog.pg_advisory_xact_lock($1::bigint)`. A statement with advisory calls must not
//! derive a key another way and stands nowhere else.

use super::source::{Fail, FailKind};
use super::sql::{self, Text, units};

/// The boundaries of executable advisory SQL: the executor and the functions it may call, and
/// optionally baseline migrations with the ledger key they lock and a negative test.
#[derive(Clone, Debug)]
pub struct Boundaries {
    pub executor: String,
    pub functions: Vec<String>,
    pub migrations: Vec<String>,
    pub negative: Option<String>,
    /// The `<FAMILY>.<RESOURCE>` static key the baseline migrations lock.
    pub ledger: Option<String>,
}

/// The aspect and message of a rejected file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rejection {
    pub aspect: &'static str,
    pub message: Text,
}

/// The aspect of a file the extraction rejects.
#[must_use]
pub fn extraction_rejection(fail: Fail) -> Rejection {
    Rejection {
        aspect: match fail.kind {
            FailKind::Unresolved => "unresolved",
            FailKind::Sql => "sql",
        },
        message: fail.message,
    }
}

/// Checks one executable statement of `path` against the boundaries. Without a ledger key, a
/// baseline migration is not compared.
///
/// # Errors
/// Returns the aspect and the message of the first rule the statement breaks.
pub fn validate(
    path: &str,
    statement: &[u16],
    boundaries: &Boundaries,
    ledger: Option<(i64, i64)>,
) -> Result<(), Rejection> {
    let rejection = |aspect, message: String| Rejection {
        aspect,
        message: units(&message),
    };
    let analysis = sql::analysis(statement).map_err(|error| Rejection {
        aspect: "sql",
        message: error.0,
    })?;

    if analysis.calls.is_empty() {
        return Ok(());
    }

    if analysis.alternate_derivation {
        return Err(rejection(
            "derivation",
            format!("{path} contains an alternate advisory-key derivation."),
        ));
    }

    let normalized = |argument: &[u16]| {
        sql::normalize_argument(argument).map_err(|error| Rejection {
            aspect: "sql",
            message: error.0,
        })
    };

    if path == boundaries.executor {
        for call in &analysis.calls {
            let valid = boundaries
                .functions
                .iter()
                .any(|name| call.name == units(name))
                && call.qualified
                && call.arguments.len() == 2
                && normalized(&call.arguments[0])? == units("$1::integer")
                && normalized(&call.arguments[1])? == units("$2::integer");

            if !valid {
                return Err(rejection(
                    "executor",
                    format!(
                        "{path} contains a malformed executor operation {}.",
                        String::from_utf16_lossy(&call.name)
                    ),
                ));
            }
        }

        return Ok(());
    }

    if boundaries
        .migrations
        .iter()
        .any(|migration| migration == path)
    {
        let Some(ledger) = ledger else {
            return Ok(());
        };
        let valid = match analysis.calls.as_slice() {
            [call] => {
                call.name == units("pg_advisory_xact_lock")
                    && call.qualified
                    && call.arguments.len() == 2
                    && normalized(&call.arguments[0])? == units(&ledger.0.to_string())
                    && normalized(&call.arguments[1])? == units(&ledger.1.to_string())
            }
            _ => false,
        };

        return if valid {
            Ok(())
        } else {
            Err(rejection(
                "migration",
                format!("{path} must contain only the frozen baseline procedure pair."),
            ))
        };
    }

    if boundaries.negative.as_deref() == Some(path) {
        let valid = match analysis.calls.as_slice() {
            [call] => {
                call.name == units("pg_advisory_xact_lock")
                    && call.qualified
                    && call.arguments.len() == 1
                    && normalized(&call.arguments[0])? == units("$1::bigint")
            }
            _ => false,
        };

        return if valid {
            Ok(())
        } else {
            Err(rejection(
                "negative",
                format!("{path} must contain exactly one schema-qualified $1::bigint negative."),
            ))
        };
    }

    Err(rejection(
        "outside",
        format!("{path} contains executable advisory SQL outside the exact allowlist."),
    ))
}

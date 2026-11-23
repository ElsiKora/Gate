//! The executable SQL of a JavaScript or TypeScript source: a static reconstruction of the text a
//! SQL sink receives.
//!
//! A SQL sink is a call of one of the sink methods (by default `.query`, `.select` or
//! `.addSelect`) or a template tagged with one of the sink tags (by default `sql`), also
//! through `bind`, `call`, `apply`, aliases, destructuring and computed member names. Its argument
//! is rebuilt from literals, templates, `+`, `concat`, `join`, `replace` and `replaceAll` with a
//! string search, arrays with spread and holes, object properties and every assignment of a
//! variable, within depth, work, candidate and text budgets. Text is UTF-16 code
//! units, as JavaScript strings are.
//!
//! Where the reconstruction cannot follow the source, it fails closed. An operation on a rebuilt
//! text that it cannot evaluate, a `replace` whose search or replacement is not rebuilt (a regular
//! expression or a callback, for one) or a method it does not model, makes an argument it keeps
//! from a text unresolved advisory SQL. The search for advisory evidence behind an unresolved argument walks its expressions and
//! follows the assignments of every identifier inside it, each once per argument within a work
//! budget; a function or class body or other node it does not model is read as its text, evidence
//! when that holds an advisory fragment, and so is a function or class declaration the argument
//! names, where the fragment must be more than a bare `pg_`. An assignment to a parenthesised name
//! (`(sql) = …`) is followed.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_typescript::SourceFile;
use rustc_hash::FxHashSet as HashSet;

use self::evidence::File;
use self::interpreter::Interpreter;
pub use self::shell::Patterns as ShellPatterns;
use super::sql::{self, SqlError, Text, units};

mod evidence;
mod interpreter;
mod node;
mod numeric;
mod recovery;
mod shell;
mod sinks;
mod text;

/// What kind of rejection a [`Fail`] is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailKind {
    /// A sink argument with advisory evidence that cannot be rebuilt, or a reconstruction past its
    /// budget.
    Unresolved,
    /// SQL the advisory-lock lexer or analysis rejects.
    Sql,
}

/// Why the contract rejects a source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fail {
    pub kind: FailKind,
    pub message: Text,
}

impl Fail {
    fn new(message: &str) -> Self {
        Self {
            kind: FailKind::Unresolved,
            message: units(message),
        }
    }

    fn resource() -> Self {
        Self::new("Static expression resource budget was exceeded.")
    }
}

impl From<SqlError> for Fail {
    fn from(error: SqlError) -> Self {
        Self {
            kind: FailKind::Sql,
            message: error.0,
        }
    }
}

impl std::fmt::Display for Fail {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&String::from_utf16_lossy(&self.message))
    }
}

impl std::error::Error for Fail {}

type Outcome<T> = Result<T, Fail>;

/// The variables a reconstruction has already followed on its path.
type Seen<'a> = HashSet<&'a str>;

/// The names that make a call or a tagged template a SQL sink.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct SqlSinks {
    /// The methods whose first argument is SQL.
    pub methods: Vec<String>,
    /// The tags of SQL templates.
    pub tags: Vec<String>,
}

impl Default for SqlSinks {
    fn default() -> Self {
        Self {
            methods: ["addSelect", "query", "select"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            tags: vec!["sql".to_owned()],
        }
    }
}

/// The executable SQL of one file and the boundary paths it is read against.
#[derive(Debug)]
pub struct Extraction<'s> {
    /// The path as messages name it.
    pub path: &'s str,
    /// Whether the file is the canonical executor, whose every candidate must resolve.
    pub canonical: bool,
    /// The SQL sinks.
    pub sinks: &'s SqlSinks,
}

/// The executable advisory SQL statements of a parsed JavaScript or TypeScript source, in source
/// order.
///
/// # Errors
/// Fails where a sink argument with advisory evidence cannot be
/// rebuilt, its SQL cannot be read, or a reconstruction exceeds its budget.
pub fn statements<'a>(
    file: &SourceFile<'a>,
    reconstruction: Reconstruction<'a>,
    extraction: &Extraction<'_>,
) -> Result<Vec<Text>, Fail> {
    let reading = File::new(file, reconstruction.0, extraction.sinks.clone())?;
    let unresolved = || {
        Fail::new(&format!(
            "{} contains unresolved executable advisory SQL.",
            extraction.path
        ))
    };
    let mut statements = Vec::new();

    for node in file.semantic().nodes().iter() {
        let kind = node.kind();

        for argument in reading.sql_arguments(&kind)? {
            let interpreter = &reading.interpreter;
            interpreter.unrebuilt.set(false);
            let statement = interpreter.fresh_text(argument)?;
            let analysis = statement.as_deref().map(sql::analysis).transpose()?;
            let mutated = interpreter
                .names(argument)
                .iter()
                .any(|name| reading.mutated.contains(name));

            if extraction.canonical {
                let candidates =
                    reading.complete_candidates(argument, &Seen::default(), &mut 0, 0)?;
                let Some(candidates) = candidates.filter(|_| !mutated) else {
                    return Err(unresolved());
                };

                for candidate in candidates {
                    let analysis = sql::analysis(&candidate)?;

                    if analysis.interpolation {
                        return Err(unresolved());
                    }

                    if !analysis.calls.is_empty() {
                        statements.push(candidate);
                    }
                }
            } else if let (Some(statement), Some(analysis)) = (&statement, &analysis)
                && !analysis.interpolation
                && !mutated
            {
                if !analysis.calls.is_empty() {
                    statements.push(statement.clone());
                }
            } else {
                let mut advisory = false;

                for candidate in interpreter.fresh_candidates(argument)? {
                    if !sql::analysis(&candidate)?.calls.is_empty() {
                        advisory = true;
                        break;
                    }
                }

                // An operation the reconstruction cannot evaluate may hide a call, so it fails
                // closed (see the module documentation).
                if advisory
                    || mutated
                    || interpreter.unrebuilt.get()
                    || reading.contains_advisory(argument)?
                {
                    return Err(unresolved());
                }
            }
        }
    }

    Ok(statements)
}

/// The registered advisory-lock identifiers a text holds, in the order of `registered`: its raw numeric literals and, where `file` holds its syntax
/// tree, every value a node evaluates to.
///
/// # Errors
/// Fails where an evaluation exceeds its budget.
pub fn registered_ids<'a>(
    text: &str,
    file: Option<(&SourceFile<'a>, &Reconstruction<'a>)>,
    registered: &[i64],
) -> Result<Vec<i64>, Fail> {
    numeric::registered_ids(
        text,
        file.map(|(file, reconstruction)| (file, &reconstruction.0)),
        registered,
    )
}

/// The variables and identifiers of one parsed source, read once for both [`registered_ids`] and
/// [`statements`].
pub struct Reconstruction<'a>(Interpreter<'a>);

impl<'a> Reconstruction<'a> {
    #[must_use]
    pub fn new(file: &SourceFile<'a>) -> Self {
        Self(Interpreter::new(file))
    }
}

/// Checks the shell commands of a shell script, JSON file or `package.json` for advisory SQL.
///
/// # Errors
/// Fails where a command runs advisory SQL, a `package.json` is not
/// JSON, or an expansion exceeds its budget.
pub fn shell_commands(
    patterns: &ShellPatterns,
    path: &str,
    text: &str,
    executables: &[String],
) -> Result<(), Fail> {
    shell::check(patterns, path, text, executables)
}

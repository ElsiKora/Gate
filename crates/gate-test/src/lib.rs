//! Fixture tests for Gate plugins.
//!
//! A rule's fixtures live in one directory, one case per subdirectory:
//!
//! ```text
//! tests/fixtures/
//!   default-marker/
//!     tree/            the files the rule sees, paths relative to the project root
//!     options.json     the rule's options (optional; `{}` when absent)
//!     expected.txt     the findings, in the format of [`render`]
//! ```
//!
//! [`check_fixtures`] runs the rule natively, through the same SDK code the component runs, on
//! each case's tree and compares what it reports with `expected.txt`. With `GATE_BLESS=1` in the
//! environment it writes `expected.txt` from what the rule reports instead, for the author to
//! review in the diff.
//!
//! ```no_run
//! # mod my_plugin { pub static PLUGIN: gate_sdk::PluginSpec = gate_sdk::PluginSpec {
//! #     name: "x", version: "0",
//! #     options_schema: gate_sdk::bindings::schema::<gate_sdk::NoOptions>,
//! #     rules: &[], generators: &[], presets: &[] }; }
//! #[test]
//! fn marker() {
//!     gate_test::check_fixtures(
//!         &my_plugin::PLUGIN,
//!         "marker",
//!         concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"),
//!     )
//!     .unwrap_or_else(|failure| panic!("{failure}"));
//! }
//! ```

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use gate_sdk::{CheckRequest, Finding, GitHost, Host, Location, PluginSpec, RuleRun, TreeHost};

/// The environment variable that turns comparing into writing `expected.txt`.
pub const BLESS_VARIABLE: &str = "GATE_BLESS";

/// What to do about findings that differ.
const BLESS_HINT: &str =
    "fix the rule, or accept what it reports with GATE_BLESS=1 and review expected.txt in the diff";

/// What a fixture run found wrong: every case that failed, with what to do.
#[derive(Debug)]
pub struct Failure(String);

impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Failure {}

/// Whether `expected.txt` is written rather than compared.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Compare,
    Bless,
}

impl Mode {
    /// [`Mode::Bless`] when `GATE_BLESS` is set to `1`, else [`Mode::Compare`].
    #[must_use]
    pub fn from_env() -> Self {
        if std::env::var_os(BLESS_VARIABLE).is_some_and(|value| value == "1") {
            Self::Bless
        } else {
            Self::Compare
        }
    }
}

/// Runs rule `rule` of `spec` on every case under `fixtures`, in the mode `GATE_BLESS` sets.
///
/// # Errors
/// A [`Failure`] naming each case whose findings differ from its `expected.txt`, each case that
/// cannot be read, and the rule's own errors; or a `fixtures` directory with no case.
pub fn check_fixtures(
    spec: &PluginSpec,
    rule: &str,
    fixtures: impl AsRef<Path>,
) -> Result<(), Failure> {
    check_fixtures_in(spec, rule, fixtures.as_ref(), Mode::from_env())
}

/// [`check_fixtures`] in an explicit `mode`.
///
/// # Errors
/// As [`check_fixtures`].
pub fn check_fixtures_in(
    spec: &PluginSpec,
    rule: &str,
    fixtures: &Path,
    mode: Mode,
) -> Result<(), Failure> {
    let cases = cases(fixtures)?;
    let mut failures = String::new();
    for case in &cases {
        if let Err(message) = check_case(spec, rule, case, mode) {
            let _ = writeln!(failures, "{message}");
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Failure(failures))
    }
}

/// The case directories under `fixtures`, sorted.
fn cases(fixtures: &Path) -> Result<Vec<PathBuf>, Failure> {
    let entries = std::fs::read_dir(fixtures).map_err(|error| {
        Failure(format!(
            "cannot read the fixtures directory {}: {error}; create it with one directory per case",
            fixtures.display()
        ))
    })?;
    let mut cases = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| Failure(format!("cannot read {}: {error}", fixtures.display())))?
            .path();
        if path.is_dir() {
            cases.push(path);
        }
    }
    cases.sort();
    if cases.is_empty() {
        return Err(Failure(format!(
            "{} has no case: add a directory with a tree/ and an expected.txt",
            fixtures.display()
        )));
    }
    Ok(cases)
}

fn check_case(spec: &PluginSpec, rule: &str, case: &Path, mode: Mode) -> Result<(), String> {
    let name = case
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let tree =
        Fixture::read(&case.join("tree")).map_err(|message| format!("case {name}: {message}"))?;
    let options = match std::fs::read_to_string(case.join("options.json")) {
        Ok(options) => options,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("case {name}: cannot read options.json: {error}")),
    };
    let request = CheckRequest {
        rules: vec![RuleRun {
            name: rule.to_owned(),
            options,
            ..RuleRun::default()
        }],
        ..CheckRequest::default()
    };
    let findings =
        gate_sdk::check(spec, &tree, &request).map_err(|error| format!("case {name}: {error}"))?;
    let actual = render(&findings);
    let expected_path = case.join("expected.txt");

    if mode == Mode::Bless {
        return std::fs::write(&expected_path, &actual)
            .map_err(|error| format!("case {name}: cannot write expected.txt: {error}"));
    }
    let expected = match std::fs::read_to_string(&expected_path) {
        Ok(expected) => expected,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!(
                "case {name}: no expected.txt; the rule reports:\n{actual}{BLESS_HINT}"
            ));
        }
        Err(error) => return Err(format!("case {name}: cannot read expected.txt: {error}")),
    };
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "case {name}: the findings differ from expected.txt\n--- expected\n{expected}--- reported\n{actual}{BLESS_HINT}"
        ))
    }
}

/// The findings as `expected.txt` holds them, in the order reported: one line
/// `<location> <rule>[#<aspect>]: <message>`, where the location is `path`, `path:line`,
/// `path:line:column`, `commit <id>` or `-` for none; then `  target: ..` and `  help: ..` lines
/// when the finding has them.
#[must_use]
pub fn render(findings: &[Finding]) -> String {
    let mut out = String::new();
    for finding in findings {
        let location = match &finding.location {
            None => "-".to_owned(),
            Some(Location::Commit(id)) => format!("commit {id}"),
            Some(Location::File(file)) => match file.span {
                None => file.path.clone(),
                Some(span) if span.column == 0 => format!("{}:{}", file.path, span.line),
                Some(span) => format!("{}:{}:{}", file.path, span.line, span.column),
            },
        };
        let rule = if finding.aspect.is_empty() {
            finding.rule.clone()
        } else {
            format!("{}#{}", finding.rule, finding.aspect)
        };
        let _ = writeln!(out, "{location} {rule}: {}", finding.message);
        if let Some(target) = &finding.target {
            let _ = writeln!(out, "  target: {target}");
        }
        if let Some(help) = &finding.help {
            let _ = writeln!(out, "  help: {help}");
        }
    }
    out
}

/// A fixture tree read from disk: every file under the directory, by its `/`-separated path.
/// It lists and reads as the runtime does for a project call: a listing is every path that starts
/// with the prefix, sorted.
struct Fixture {
    files: BTreeMap<String, Vec<u8>>,
}

impl Fixture {
    fn read(root: &Path) -> Result<Self, String> {
        let mut files = BTreeMap::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(directory) = pending.pop() {
            let entries = std::fs::read_dir(&directory).map_err(|error| {
                format!(
                    "cannot read {}: {error}; a case keeps the rule's files in tree/",
                    directory.display()
                )
            })?;
            for entry in entries {
                let path = entry
                    .map_err(|error| format!("cannot read {}: {error}", directory.display()))?
                    .path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                let relative = path
                    .strip_prefix(root)
                    .map_err(|error| format!("{}: {error}", path.display()))?
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                let bytes = std::fs::read(&path)
                    .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
                files.insert(relative, bytes);
            }
        }
        Ok(Self { files })
    }
}

impl TreeHost for Fixture {
    fn list(&self, prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|path| path.starts_with(prefix))
            .cloned()
            .collect()
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(path).cloned()
    }

    fn changed(&self) -> Option<Vec<String>> {
        None
    }
}

impl Host for Fixture {
    fn git(&self) -> Option<&dyn GitHost> {
        None
    }

    fn log(&self, _: &str) {}
}

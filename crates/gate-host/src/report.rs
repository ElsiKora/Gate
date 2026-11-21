//! The report of a run, and its four output formats.

use std::fmt::Write as _;

use serde::Serialize;
use serde_json::json;

use crate::baseline::BASELINE_FILE;
use crate::changes::{Reason, Selection};
use crate::error::Result;
use crate::findings::{Finding, Level, Location, SHORT_ID};

/// The output format of a report.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Format {
    /// Lines for people, ending with a one-line summary.
    #[default]
    Text,
    /// One JSON document.
    Json,
    /// SARIF 2.1.0.
    Sarif,
    /// GitHub Actions workflow commands (annotations), then the summary.
    Github,
}

/// The exit code of a run with no error finding.
pub const EXIT_CLEAN: i32 = 0;
/// The exit code of a run with at least one error finding.
pub const EXIT_ERRORS: i32 = 1;
/// The exit code of a configuration or environment failure.
pub const EXIT_FAILURE: i32 = 2;

/// The outcome of a `gate check`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Report {
    /// The run's mode: `tree`, `staged`, `rev <oid>`, `changed since <base>`, `push <range>`.
    pub mode: String,
    /// How many rules ran.
    pub rules: usize,
    /// The findings, in the deterministic order.
    pub findings: Vec<Finding>,
    /// How many findings an entry of `gate/baseline.json` accepted, so they are not reported.
    pub baselined: usize,
    /// The projects that ran and that were skipped.
    pub selection: Selection,
    /// The plugins replaced for this run by `--plugin`, each as `<name>=<path> sha256 <hex>`.
    pub overrides: Vec<String>,
}

impl Report {
    /// The number of findings at `level`.
    #[must_use]
    pub fn count(&self, level: Level) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.level == level)
            .count()
    }

    /// 1 when there is an error finding, else 0.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        if self.count(Level::Error) > 0 {
            EXIT_ERRORS
        } else {
            EXIT_CLEAN
        }
    }

    /// `gate check (<mode>): N rules, E errors, W warnings; projects: ran a, b; skipped c`, with
    /// `, B findings held back by gate/baseline.json` after the warnings when the baseline held
    /// any back, then `; override <name>=<path> sha256 <hex>` for each `--plugin`.
    #[must_use]
    pub fn summary(&self) -> String {
        let names = |decisions: &[crate::changes::Decision]| {
            if decisions.is_empty() {
                "none".to_owned()
            } else {
                decisions
                    .iter()
                    .map(|decision| decision.project.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        };
        let overrides = self.overrides.iter().fold(String::new(), |mut text, line| {
            let _ = write!(text, "; override {line}");
            text
        });
        let baselined = if self.baselined == 0 {
            String::new()
        } else {
            format!(
                ", {} {} held back by {BASELINE_FILE}",
                self.baselined,
                plural(self.baselined, "finding", "findings")
            )
        };
        format!(
            "gate check ({}): {} {}, {} {}, {} {}{baselined}; projects: ran {}; skipped {}{overrides}",
            self.mode,
            self.rules,
            plural(self.rules, "rule", "rules"),
            self.count(Level::Error),
            plural(self.count(Level::Error), "error", "errors"),
            self.count(Level::Warning),
            plural(self.count(Level::Warning), "warning", "warnings"),
            names(&self.selection.ran),
            names(&self.selection.skipped),
        )
    }

    /// The report in `format`.
    ///
    /// # Errors
    /// Fails when the report cannot be serialized.
    pub fn render(&self, format: Format) -> Result<String> {
        match format {
            Format::Text => Ok(self.text()),
            Format::Json => self.json(),
            Format::Sarif => self.sarif(),
            Format::Github => Ok(self.github()),
        }
    }

    fn text(&self) -> String {
        let mut out = String::new();

        for finding in &self.findings {
            out.push_str(&render_finding(finding));
        }

        let selective = self
            .selection
            .ran
            .iter()
            .chain(&self.selection.skipped)
            .any(|decision| decision.reason != Reason::Full);

        if selective {
            for decision in &self.selection.ran {
                let _ = writeln!(out, "ran {}: {}", decision.project, decision.reason);
            }
            for decision in &self.selection.skipped {
                let _ = writeln!(out, "skipped {}: {}", decision.project, decision.reason);
            }
        }

        out.push_str(&self.summary());
        out.push('\n');
        out
    }

    fn json(&self) -> Result<String> {
        let findings = self
            .findings
            .iter()
            .map(|finding| {
                let mut value = serde_json::to_value(finding)?;
                if let Some(object) = value.as_object_mut() {
                    object.insert("fingerprint".to_owned(), json!(finding.fingerprint()));
                }
                Ok(value)
            })
            .collect::<Result<Vec<serde_json::Value>>>()?;
        let value = json!({
            "mode": self.mode,
            "rules": self.rules,
            "errors": self.count(Level::Error),
            "warnings": self.count(Level::Warning),
            "notes": self.count(Level::Note),
            "baselined": self.baselined,
            "projects": self.selection,
            "findings": findings,
            "summary": self.summary(),
        });
        let mut out = serde_json::to_string_pretty(&value)?;
        out.push('\n');
        Ok(out)
    }

    fn sarif(&self) -> Result<String> {
        let mut rules: Vec<&str> = self
            .findings
            .iter()
            .map(|finding| finding.rule.as_str())
            .collect();
        rules.sort_unstable();
        rules.dedup();

        let results: Vec<serde_json::Value> = self
            .findings
            .iter()
            .map(|finding| {
                let mut result = json!({
                    "ruleId": finding.rule,
                    "level": finding.level.to_string(),
                    "message": { "text": finding.message },
                    "partialFingerprints": { "gate/v1": finding.fingerprint() },
                });
                let mut properties = serde_json::Map::new();
                if let Some(Location::Commit { commit }) = &finding.location {
                    properties.insert("commit".to_owned(), json!(commit));
                }
                if let Some(Location::File { path, span }) = &finding.location {
                    let mut physical = json!({
                        "artifactLocation": { "uri": path },
                    });
                    if let Some(span) = span {
                        physical["region"] = json!({
                            "startLine": span.line,
                            "startColumn": span.column,
                            "endLine": span.end_line,
                            "endColumn": span.end_column,
                        });
                    }
                    result["locations"] = json!([{ "physicalLocation": physical }]);
                }
                if !finding.aspect.is_empty() {
                    properties.insert("aspect".to_owned(), json!(finding.aspect));
                }
                if let Some(norm) = &finding.norm {
                    properties.insert("norm".to_owned(), json!(norm));
                }
                if let Some(project) = &finding.project {
                    properties.insert("project".to_owned(), json!(project));
                }
                if !properties.is_empty() {
                    result["properties"] = serde_json::Value::Object(properties);
                }
                result
            })
            .collect();

        let value = json!({
            "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "gate",
                        "version": env!("CARGO_PKG_VERSION"),
                        "informationUri": "https://github.com/ElsiKora/Gate",
                        "rules": rules.iter().map(|rule| json!({ "id": rule })).collect::<Vec<_>>(),
                    }
                },
                "results": results,
                "properties": { "baselined": self.baselined },
            }],
        });
        let mut out = serde_json::to_string_pretty(&value)?;
        out.push('\n');
        Ok(out)
    }

    fn github(&self) -> String {
        let mut out = String::new();

        for finding in &self.findings {
            let command = match finding.level {
                Level::Error => "error",
                Level::Warning => "warning",
                Level::Note => "notice",
            };
            let mut properties = Vec::new();
            let mut message = finding.message.clone();
            if let Some(Location::Commit { commit }) = &finding.location {
                let _ = write!(message, "\ncommit: {commit}");
            }
            if let Some(Location::File { path, span }) = &finding.location {
                properties.push(format!("file={}", escape_property(path)));
                if let Some(span) = span {
                    properties.push(format!("line={}", span.line));
                    properties.push(format!("col={}", span.column));
                    properties.push(format!("endLine={}", span.end_line));
                    properties.push(format!("endColumn={}", span.end_column));
                }
            }
            properties.push(format!(
                "title={}",
                escape_property(&finding.rule_with_aspect())
            ));
            if let Some(help) = &finding.help {
                let _ = write!(message, "\nhelp: {help}");
            }
            if let Some(norm) = &finding.norm {
                let _ = write!(message, "\nnorm: {norm}");
            }
            let _ = writeln!(
                out,
                "::{command} {}::{}",
                properties.join(","),
                escape_data(&message)
            );
        }

        out.push_str(&self.summary());
        out.push('\n');
        out
    }
}

/// One finding as the text format prints it: `level[rule#aspect] path:line:column: message`, then
/// its help and norm lines.
#[must_use]
pub fn render_finding(finding: &Finding) -> String {
    let location = finding
        .location
        .as_ref()
        .map(|location| match location {
            Location::File {
                path,
                span: Some(span),
            } => format!(" {path}:{}:{}", span.line, span.column),
            Location::File { path, span: None } => format!(" {path}"),
            Location::Commit { commit } => format!(
                " commit {}",
                commit.get(..SHORT_ID).unwrap_or(commit.as_str())
            ),
        })
        .unwrap_or_default();
    let mut out = format!(
        "{}[{}]{location}: {}\n",
        finding.level,
        finding.rule_with_aspect(),
        finding.message
    );
    if let Some(help) = &finding.help {
        let _ = writeln!(out, "  help: {help}");
    }
    if let Some(norm) = &finding.norm {
        let _ = writeln!(out, "  norm: {norm}");
    }
    out
}

/// The output format of `gate check --list-changed-projects`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SelectionFormat {
    /// `ran <project>: <reason>` and `skipped <project>: <reason>` lines, then the repository scope.
    #[default]
    Text,
    /// One JSON document: `projects` (the names that run), `repository` and every decision.
    Json,
    /// `projects=<JSON list>` and `repository=<true|false>`, for `$GITHUB_OUTPUT`.
    GithubOutput,
}

/// The projects a change selects, in `format`.
///
/// # Errors
/// Fails when the selection cannot be written as JSON.
pub fn render_selection(selection: &Selection, format: SelectionFormat) -> Result<String> {
    let json = |value: &serde_json::Value, pretty: bool| {
        if pretty {
            serde_json::to_string_pretty(value)
        } else {
            serde_json::to_string(value)
        }
        .map_err(|error| {
            crate::Error::environment(format!("cannot write the selection as JSON: {error}"))
        })
    };
    let names: Vec<&str> = selection
        .ran
        .iter()
        .map(|decision| decision.project.as_str())
        .collect();
    match format {
        SelectionFormat::Text => {
            let mut out = String::new();
            for decision in &selection.ran {
                let _ = writeln!(out, "ran {}: {}", decision.project, decision.reason);
            }
            for decision in &selection.skipped {
                let _ = writeln!(out, "skipped {}: {}", decision.project, decision.reason);
            }
            let _ = writeln!(
                out,
                "repository: {}",
                if selection.repository {
                    "runs"
                } else {
                    "skipped"
                }
            );
            Ok(out)
        }
        SelectionFormat::Json => {
            let mut out = json(
                &json!({
                    "projects": names,
                    "repository": selection.repository,
                    "selection": selection,
                }),
                true,
            )?;
            out.push('\n');
            Ok(out)
        }
        SelectionFormat::GithubOutput => Ok(format!(
            "projects={}\nrepository={}\n",
            json(&json!(names), false)?,
            selection.repository
        )),
    }
}

fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 { one } else { many }
}

fn escape_data(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn escape_property(value: &str) -> String {
    escape_data(value).replace(':', "%3A").replace(',', "%2C")
}

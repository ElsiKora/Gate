//! `gate list` and `gate explain`: what the configuration holds, without running anything.

use std::fmt::Write as _;

use gate_host::config::{Options, RuleRef, SettingOrigin, Settings};
use gate_host::projects::display_root;
use gate_host::{ScopedException, Workspace};
use serde_json::json;

/// What `gate explain` asks about: one rule, one aspect, or every rule of a plugin.
pub enum Subject {
    /// `<plugin>/*`.
    Family(String),
    /// `<plugin>/<rule>` or `<plugin>/<rule>#<aspect>`.
    Rule(RuleRef),
}

impl Subject {
    /// Parses `<plugin>/*`, `<plugin>/<rule>` or `<plugin>/<rule>#<aspect>`.
    pub fn parse(value: &str) -> Result<Self, String> {
        if let Some(plugin) = value.strip_suffix("/*") {
            return if gate_host::config::is_kebab(plugin) {
                Ok(Self::Family(plugin.to_owned()))
            } else {
                Err(format!(
                    "{value:?} is not a family wildcard; write <plugin>/*, for example \"layers/*\""
                ))
            };
        }
        RuleRef::parse(value).map(Self::Rule)
    }

    fn plugin(&self) -> &str {
        match self {
            Self::Family(plugin) => plugin,
            Self::Rule(rule) => rule.rule.plugin(),
        }
    }

    /// Whether a row about `key` (a rule id, a family, or a rule with `#aspect`) concerns the
    /// subject.
    fn concerns(&self, key: &str) -> bool {
        let (rule, aspect) = match key.split_once('#') {
            Some((rule, aspect)) => (rule, Some(aspect)),
            None => (key, None),
        };
        let plugin = rule.split_once('/').map_or(rule, |(plugin, _)| plugin);

        match self {
            Self::Family(family) => plugin == family,
            Self::Rule(wanted) => {
                if rule.ends_with("/*") {
                    return plugin == wanted.rule.plugin();
                }
                rule == wanted.rule.to_string()
                    && match (&wanted.aspect, aspect) {
                        (Some(wanted), Some(aspect)) => wanted == aspect,
                        _ => true,
                    }
            }
        }
    }

    fn concerns_exception(&self, exception: &ScopedException) -> bool {
        match self {
            Self::Family(family) => exception.rule.rule.plugin() == family,
            Self::Rule(wanted) => {
                exception.rule.rule == wanted.rule
                    && match (&wanted.aspect, &exception.rule.aspect) {
                        (Some(wanted), Some(aspect)) => wanted == aspect,
                        _ => true,
                    }
            }
        }
    }
}

/// One setting as `gate list` prints it.
struct Row<'a> {
    kind: &'static str,
    key: String,
    value: String,
    origin: &'a SettingOrigin,
}

/// Every setting of `settings`, one row each, in a stable order.
fn rows(settings: &Settings) -> Vec<Row<'_>> {
    let mut rows = Vec::new();

    for (name, plugin) in &settings.plugins {
        let reads = if plugin.value.reads.is_empty() {
            String::new()
        } else {
            format!(
                " reads {}",
                plugin
                    .value
                    .reads
                    .iter()
                    .map(gate_host::config::RepositoryPath::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let options = plugin
            .value
            .options
            .as_ref()
            .map(|options| format!(" ({})", describe_options(options)))
            .unwrap_or_default();
        rows.push(Row {
            kind: "plugin",
            key: name.to_string(),
            value: format!("{}{options}{reads}", plugin.value.source),
            origin: &plugin.origin,
        });
    }

    for (id, language) in &settings.languages {
        rows.push(Row {
            kind: "language",
            key: id.clone(),
            value: language.value.to_string(),
            origin: &language.origin,
        });
    }

    for preset in &settings.extends {
        rows.push(Row {
            kind: "extends",
            key: preset.value.to_string(),
            value: String::new(),
            origin: &preset.origin,
        });
    }

    for (plugin, setting) in &settings.families {
        rows.push(Row {
            kind: "rule",
            key: format!("{plugin}/*"),
            value: format!("level {}", setting.value),
            origin: &setting.origin,
        });
    }

    for (rule, entry) in &settings.rules {
        if let Some(level) = &entry.level {
            rows.push(Row {
                kind: "rule",
                key: rule.to_string(),
                value: format!("level {}", level.value),
                origin: &level.origin,
            });
        }
        if let Some(norm) = &entry.norm {
            rows.push(Row {
                kind: "rule",
                key: rule.to_string(),
                value: format!("norm {}", norm.value),
                origin: &norm.origin,
            });
        }
        if let Some(options) = &entry.options {
            rows.push(Row {
                kind: "rule",
                key: rule.to_string(),
                value: describe_options(&options.value),
                origin: &options.origin,
            });
        }
        for (aspect, level) in &entry.levels {
            rows.push(Row {
                kind: "rule",
                key: format!("{rule}#{aspect}"),
                value: format!("level {}", level.value),
                origin: &level.origin,
            });
        }
        for (aspect, norm) in &entry.norms {
            rows.push(Row {
                kind: "rule",
                key: format!("{rule}#{aspect}"),
                value: format!("norm {}", norm.value),
                origin: &norm.origin,
            });
        }
    }

    for (generator, entry) in &settings.generate {
        rows.push(Row {
            kind: "generate",
            key: generator.to_string(),
            value: entry
                .value
                .as_ref()
                .map(describe_options)
                .unwrap_or_default(),
            origin: &entry.origin,
        });
    }

    for (name, step) in &settings.steps {
        let mut value = step.value.run.clone();
        if step.value.heavy {
            value.push_str(" (heavy)");
        }
        if let Some(files) = step.value.files {
            let _ = write!(value, " (+ {files} files");
            if let Some(glob) = &step.value.glob {
                let _ = write!(value, " matching {glob}");
            }
            value.push(')');
        }
        rows.push(Row {
            kind: "step",
            key: name.to_string(),
            value,
            origin: &step.origin,
        });
    }

    for (name, suite) in &settings.suites {
        let mut value = suite
            .value
            .steps
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        if suite.value.fail_fast {
            value.push_str(" (fail-fast)");
        }
        rows.push(Row {
            kind: "suite",
            key: name.to_string(),
            value,
            origin: &suite.origin,
        });
    }

    rows
}

fn describe_options(options: &Options) -> String {
    match &options.file {
        Some(file) => format!("options-file {file}"),
        None => "options".to_owned(),
    }
}

fn write_rows(out: &mut String, indent: &str, rows: &[Row<'_>], origin: bool) {
    let table: Vec<[String; 4]> = rows
        .iter()
        .map(|row| {
            [
                row.kind.to_owned(),
                row.key.clone(),
                row.value.clone(),
                if origin {
                    row.origin.to_string()
                } else {
                    String::new()
                },
            ]
        })
        .collect();
    write_table(out, indent, &table);
}

/// The scopes `gate list` prints: the repository's own settings when the top is not a project,
/// then each project's merged settings.
fn scopes(workspace: &Workspace) -> Vec<(String, &Settings)> {
    let mut scopes = Vec::new();

    if !workspace.root_is_project() {
        scopes.push((
            format!("repository ({})", workspace.root_config().path()),
            workspace.repository(),
        ));
    }

    for project in workspace.projects().iter() {
        if let Some(settings) = workspace.settings(&project.name) {
            scopes.push((
                format!("project {} ({})", project.name, display_root(&project.root)),
                settings,
            ));
        }
    }

    scopes
}

/// The projects, then the settings of the repository and of each project; with `origin`, the
/// file, line and section each setting came from.
pub fn text(workspace: &Workspace, origin: bool) -> String {
    let mut out = String::new();
    let projects = workspace.projects();
    let _ = writeln!(out, "projects ({}):", projects.len());

    let table: Vec<[String; 4]> = projects
        .iter()
        .map(|project| {
            let origins = project
                .origins
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let dependencies = if project.dependencies.is_empty() {
                String::new()
            } else {
                format!(
                    "depends on {}",
                    project
                        .dependencies
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            [
                project.name.clone(),
                display_root(&project.root).to_owned(),
                format!("({origins})"),
                dependencies,
            ]
        })
        .collect();
    write_table(&mut out, "  ", &table);

    for (title, settings) in scopes(workspace) {
        let _ = writeln!(out, "{title}:");
        let rows = rows(settings);
        if rows.is_empty() {
            let _ = writeln!(out, "  nothing configured");
        }
        write_rows(&mut out, "  ", &rows, origin);
    }

    out
}

/// Writes rows with each column padded to its widest cell; trailing empty cells are dropped.
fn write_table<const N: usize>(out: &mut String, indent: &str, rows: &[[String; N]]) {
    let mut widths = [0; N];
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }

    for row in rows {
        let last = row.iter().rposition(|cell| !cell.is_empty()).unwrap_or(0);
        let mut line = String::from(indent);
        for (index, (cell, width)) in row.iter().zip(widths).enumerate().take(last + 1) {
            if index == last {
                line.push_str(cell);
            } else {
                let _ = write!(line, "{cell:<width$}  ");
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
}

fn settings_json(settings: &Settings) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = rows(settings)
        .iter()
        .map(|row| {
            json!({
                "kind": row.kind,
                "key": row.key,
                "value": row.value,
                "origin": row.origin,
            })
        })
        .collect();
    let options: serde_json::Map<String, serde_json::Value> = settings
        .rules
        .iter()
        .filter_map(|(rule, entry)| {
            entry
                .options
                .as_ref()
                .map(|options| (rule.to_string(), options.value.value.clone()))
        })
        .collect();
    json!({ "settings": rows, "options": options })
}

/// The same as [`text`], as JSON, origins included.
pub fn json(workspace: &Workspace) -> Result<String, gate_host::Error> {
    let settings: serde_json::Map<String, serde_json::Value> = workspace
        .projects()
        .iter()
        .filter_map(|project| {
            workspace
                .settings(&project.name)
                .map(|settings| (project.name.clone(), settings_json(settings)))
        })
        .collect();
    let value = json!({
        "projects": workspace.projects(),
        "repository": settings_json(workspace.repository()),
        "project-settings": settings,
    });
    let mut out = serde_json::to_string_pretty(&value)?;
    out.push('\n');
    Ok(out)
}

/// What the configuration says about `subject`, in every scope, with origins.
pub fn explain(workspace: &Workspace, subject: &Subject, name: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{name}");
    let mut found = false;

    for (title, settings) in scopes(workspace) {
        let related: Vec<Row<'_>> = rows(settings)
            .into_iter()
            .filter(|row| {
                (row.kind == "rule" && subject.concerns(&row.key))
                    || (row.kind == "plugin" && row.key == subject.plugin())
            })
            .collect();

        if related.iter().all(|row| row.kind == "plugin") {
            continue;
        }

        found = true;
        let _ = writeln!(out, "  {title}:");
        write_rows(&mut out, "    ", &related, true);
    }

    let exceptions: Vec<&ScopedException> = workspace
        .exceptions()
        .iter()
        .filter(|exception| subject.concerns_exception(exception))
        .collect();

    if !exceptions.is_empty() {
        found = true;
        let _ = writeln!(out, "  exceptions:");
        for exception in exceptions {
            let target = &exception.target;
            let decision = exception
                .decision
                .as_ref()
                .map(|decision| format!(" (decision {decision})"))
                .unwrap_or_default();
            let owner = exception
                .owner
                .as_ref()
                .map(|owner| format!(" (owner {owner})"))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "    {} at {target}: {}{decision}{owner}  {}:{}",
                exception.rule, exception.reason, exception.file, exception.line
            );
        }
    }

    if !found {
        let _ = writeln!(out, "  no gate.toml configures it");
    }

    out
}

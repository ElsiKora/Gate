//! `gate list` and `gate explain`: what the configuration holds, without running anything.

use std::fmt::Write as _;

use gate_host::config::{Options, RuleRef, SettingOrigin, Settings};
use gate_host::projects::display_root;
use gate_host::{PluginOverride, PresetSetting, PresetSettings, ScopedException, Workspace};
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

/// What the presets of each scope set, or why it could not be read: `gate list` and `gate
/// explain` still print what the `gate.toml` files write, and say why the presets' rules are
/// missing.
#[derive(Debug)]
pub struct Presets {
    settings: Result<PresetSettings, gate_host::Error>,
}

impl Presets {
    /// Reads what the presets set from what each declared plugin declares, `overrides` read
    /// instead of their pins.
    pub fn read(workspace: &Workspace, overrides: &[PluginOverride]) -> Self {
        Self {
            settings: gate_host::preset_settings(workspace, overrides),
        }
    }

    /// The settings the presets give the rules a check runs in the repository scope (`None`) or
    /// in a project's. For the project at the repository's top, whose section stands for the
    /// repository too, the repository scope's come first.
    fn of(&self, workspace: &Workspace, project: Option<&str>) -> Vec<&PresetSetting> {
        let scope = |project: Option<&str>| {
            self.settings
                .as_ref()
                .ok()
                .and_then(|settings| settings.get(&project.map(ToOwned::to_owned)))
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
        };
        let at_top = project
            .and_then(|name| workspace.projects().get(name))
            .is_some_and(|project| project.root.is_empty());
        let repository = if at_top { scope(None) } else { [].iter() };
        repository.chain(scope(project)).collect()
    }

    /// Why the presets' rules are not shown, when they could not be read.
    fn missing(&self) -> Option<String> {
        self.settings.as_ref().err().map(|error| {
            format!(
                "the rules the presets turn on are not shown: {}",
                error.message()
            )
        })
    }
}

/// One setting as `gate list` prints it.
struct Row<'a> {
    kind: &'static str,
    key: String,
    value: String,
    origin: &'a SettingOrigin,
    /// The preset that gives the setting, `<plugin>/<preset>`; `None` for what a `gate.toml`
    /// writes.
    preset: Option<&'a str>,
}

/// Every setting of `settings` and of the presets it extends (`presets`), one row each, in a
/// stable order: the presets' rules come before the families and the `[rules]` entries, which win
/// over them.
fn rows<'a>(settings: &'a Settings, presets: &[&'a PresetSetting]) -> Vec<Row<'a>> {
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
            preset: None,
        });
    }

    for (id, language) in &settings.languages {
        rows.push(Row {
            kind: "language",
            key: id.clone(),
            value: language.value.to_string(),
            origin: &language.origin,
            preset: None,
        });
    }

    for preset in &settings.extends {
        rows.push(Row {
            kind: "extends",
            key: preset.value.to_string(),
            value: String::new(),
            origin: &preset.origin,
            preset: None,
        });
    }

    for setting in presets {
        rows.push(Row {
            kind: "rule",
            key: setting.key.clone(),
            value: format!("{} (preset {})", setting.value, setting.preset),
            origin: &setting.origin,
            preset: Some(&setting.preset),
        });
    }

    for (plugin, setting) in &settings.families {
        rows.push(Row {
            kind: "rule",
            key: format!("{plugin}/*"),
            value: format!("level {}", setting.value),
            origin: &setting.origin,
            preset: None,
        });
    }

    for (rule, entry) in &settings.rules {
        if let Some(level) = &entry.level {
            rows.push(Row {
                kind: "rule",
                key: rule.to_string(),
                value: format!("level {}", level.value),
                origin: &level.origin,
                preset: None,
            });
        }
        if let Some(norm) = &entry.norm {
            rows.push(Row {
                kind: "rule",
                key: rule.to_string(),
                value: format!("norm {}", norm.value),
                origin: &norm.origin,
                preset: None,
            });
        }
        if let Some(options) = &entry.options {
            rows.push(Row {
                kind: "rule",
                key: rule.to_string(),
                value: describe_options(&options.value),
                origin: &options.origin,
                preset: None,
            });
        }
        for (aspect, level) in &entry.levels {
            rows.push(Row {
                kind: "rule",
                key: format!("{rule}#{aspect}"),
                value: format!("level {}", level.value),
                origin: &level.origin,
                preset: None,
            });
        }
        for (aspect, norm) in &entry.norms {
            rows.push(Row {
                kind: "rule",
                key: format!("{rule}#{aspect}"),
                value: format!("norm {}", norm.value),
                origin: &norm.origin,
                preset: None,
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
            preset: None,
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
            preset: None,
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
            preset: None,
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
/// then each project's merged settings; each with the settings its presets give the rules a check
/// runs there (at a top that is a project, the repository scope's rules as well).
fn scopes<'a>(
    workspace: &'a Workspace,
    presets: &'a Presets,
) -> Vec<(String, &'a Settings, Vec<&'a PresetSetting>)> {
    let mut scopes = Vec::new();

    if !workspace.root_is_project() {
        scopes.push((
            format!("repository ({})", workspace.root_config().path()),
            workspace.repository(),
            presets.of(workspace, None),
        ));
    }

    for project in workspace.projects().iter() {
        if let Some(settings) = workspace.settings(&project.name) {
            scopes.push((
                format!("project {} ({})", project.name, display_root(&project.root)),
                settings,
                presets.of(workspace, Some(&project.name)),
            ));
        }
    }

    scopes
}

/// The projects, then the settings of the repository and of each project, the rules their presets
/// turn on included; with `origin`, the file, line and section each setting came from (for a
/// preset's rule, its `extends` entry).
pub fn text(workspace: &Workspace, presets: &Presets, origin: bool) -> String {
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

    for (title, settings, preset_settings) in scopes(workspace, presets) {
        let _ = writeln!(out, "{title}:");
        let rows = rows(settings, &preset_settings);
        if rows.is_empty() {
            let _ = writeln!(out, "  nothing configured");
        }
        write_rows(&mut out, "  ", &rows, origin);
    }
    if let Some(missing) = presets.missing() {
        let _ = writeln!(out, "{missing}");
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

fn settings_json(settings: &Settings, presets: &[&PresetSetting]) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = rows(settings, presets)
        .iter()
        .map(|row| {
            let mut value = json!({
                "kind": row.kind,
                "key": row.key,
                "value": row.value,
                "origin": row.origin,
            });
            if let (Some(preset), Some(object)) = (row.preset, value.as_object_mut()) {
                object.insert("preset".to_owned(), json!(preset));
            }
            value
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

/// The same as [`text`], as JSON, origins included; a preset's rule carries its `preset`, and
/// `presets-error` says why the presets' rules are missing when they could not be read.
pub fn json(workspace: &Workspace, presets: &Presets) -> Result<String, gate_host::Error> {
    let settings: serde_json::Map<String, serde_json::Value> = workspace
        .projects()
        .iter()
        .filter_map(|project| {
            workspace.settings(&project.name).map(|settings| {
                (
                    project.name.clone(),
                    settings_json(settings, &presets.of(workspace, Some(&project.name))),
                )
            })
        })
        .collect();
    let value = json!({
        "projects": workspace.projects(),
        "repository": settings_json(workspace.repository(), &presets.of(workspace, None)),
        "project-settings": settings,
        "presets-error": presets.missing(),
    });
    let mut out = serde_json::to_string_pretty(&value)?;
    out.push('\n');
    Ok(out)
}

/// What the configuration says about `subject`, in every scope, with origins: the `gate.toml`
/// files and the presets they extend.
pub fn explain(workspace: &Workspace, presets: &Presets, subject: &Subject, name: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{name}");
    let mut found = false;

    for (title, settings, preset_settings) in scopes(workspace, presets) {
        let related: Vec<Row<'_>> = rows(settings, &preset_settings)
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

    match presets.missing() {
        Some(missing) => {
            if !found {
                let _ = writeln!(out, "  no gate.toml configures it");
            }
            let _ = writeln!(out, "  {missing}");
        }
        None if !found => {
            let _ = writeln!(out, "  no gate.toml or preset configures it");
        }
        None => {}
    }

    out
}

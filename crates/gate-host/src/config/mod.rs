//! Configuration: the root `gate.toml`, one per project, and the `gate.exceptions.toml` beside each.
//!
//! Loading is strict. A file is read from the snapshot (so `--staged` sees the staged configuration),
//! parsed into [`ConfigFile`], and checked; every error names the file, the line when it is known,
//! the key, and what is expected. What a file sets for projects becomes [`Settings`] with the origin
//! of each setting; [`crate::Workspace`] lays them over each other for each project.

mod file;
mod names;
mod settings;

use std::collections::BTreeMap;

pub use file::{
    Budget, CiGithub, CiJob, CiSection, CiStep, ConfigFile, DecisionPolicy, Exception,
    ExceptionsFile, GateSection, GenerateEntry, HooksSection, PluginSpec, Profile, ProjectSection,
    ProjectSources, RuleEntry, RunSection, Setting, Step, StepFiles, Suite, Tool, ToolTarget,
    WorkspaceSection,
};
pub use names::{
    CORE_PLUGIN, CORE_RULES, HOOKS, HookName, JobId, Name, PluginItem, PluginName, PluginSource,
    ProjectName, RelativePath, RepositoryPath, RuleId, RuleKey, RuleRef, Version, check_relative,
    is_kebab, is_version,
};
pub use settings::{
    LevelSetting, Options, PluginSettings, RuleSettings, SettingOrigin, Settings, Sourced,
};

use serde::Deserialize;
use toml::Spanned;

use crate::error::{Error, Result};
use crate::projects::ProjectGlobs;
use crate::snapshot::Snapshot;
use crate::sync::github::CHANGES_JOB;

/// The file name of every configuration file.
pub const FILE_NAME: &str = "gate.toml";

/// The file name of the exceptions file beside a `gate.toml`.
pub const EXCEPTIONS_FILE_NAME: &str = "gate.exceptions.toml";

/// Which file a [`Config`] is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// The repository root's `gate.toml`.
    Root,
    /// A project's `gate.toml`.
    Project,
}

/// One loaded and checked `gate.toml`.
#[derive(Debug)]
pub struct Config {
    path: String,
    directory: String,
    kind: Kind,
    file: ConfigFile,
    lines: Vec<usize>,
    positions: Positions,
    own: Settings,
    profiles: Vec<ProfileLayer>,
}

/// A decision a rule entry of a `gate.toml` names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrittenDecision {
    /// The entry's key as written: `<plugin>/<rule>`, or `<plugin>/*` for a family.
    pub key: String,
    /// The decision's id, trimmed.
    pub decision: String,
    /// The line of the entry's value.
    pub line: u32,
}

/// Where the sections that carry no span of their own start, for the line numbers of messages.
#[derive(Debug, Default)]
struct Positions {
    sections: BTreeMap<String, usize>,
    tools: BTreeMap<String, usize>,
    jobs: BTreeMap<String, usize>,
}

#[derive(Deserialize)]
struct NestedPositions {
    #[serde(default)]
    tools: BTreeMap<String, Spanned<serde::de::IgnoredAny>>,
    ci: Option<CiPositions>,
}

#[derive(Deserialize)]
struct CiPositions {
    github: Option<GithubPositions>,
}

#[derive(Deserialize)]
struct GithubPositions {
    #[serde(default)]
    jobs: BTreeMap<String, Spanned<serde::de::IgnoredAny>>,
}

impl Positions {
    /// Reads the positions from a document that already parsed; empty where it cannot.
    fn read(text: &str) -> Self {
        let start = |map: BTreeMap<String, Spanned<serde::de::IgnoredAny>>| {
            map.into_iter()
                .map(|(key, value)| (key, value.span().start))
                .collect()
        };
        let sections = toml::from_str::<BTreeMap<String, Spanned<serde::de::IgnoredAny>>>(text)
            .map(start)
            .unwrap_or_default();
        let nested = toml::from_str::<NestedPositions>(text).ok();
        let (tools, jobs) = match nested {
            Some(nested) => (
                start(nested.tools),
                nested
                    .ci
                    .and_then(|ci| ci.github)
                    .map(|github| start(github.jobs))
                    .unwrap_or_default(),
            ),
            None => (BTreeMap::new(), BTreeMap::new()),
        };
        Self {
            sections,
            tools,
            jobs,
        }
    }
}

/// One `[[profiles]]` entry of the root `gate.toml`, read.
#[derive(Debug)]
pub struct ProfileLayer {
    /// The line of its `[[profiles]]` header.
    pub line: usize,
    /// Its `projects` globs, as written.
    pub projects: Vec<String>,
    globs: ProjectGlobs,
    /// What it sets for the projects it matches.
    pub settings: Settings,
}

impl ProfileLayer {
    /// Whether the profile applies to the project rooted at `root` (repository-relative, `""` for
    /// the top).
    #[must_use]
    pub fn matches(&self, root: &str) -> bool {
        self.globs.matches(root)
    }
}

/// An exception as the core applies it: with its file, line and target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScopedException {
    /// The `gate.exceptions.toml` that declares it.
    pub file: String,
    /// Its line in that file.
    pub line: usize,
    /// The rule and optional aspect.
    pub rule: RuleRef,
    /// The target as written: compared with a finding's `target`.
    pub target: String,
    /// The target read as a path, relative to the repository root: compared with the path of a
    /// finding that has no target. `None` when the target is not a path inside the directory.
    pub path: Option<String>,
    /// Why the finding is accepted.
    pub reason: String,
    /// The decision record, when given.
    pub decision: Option<String>,
    /// The owning record, when given.
    pub owner: Option<String>,
    /// The project whose `gate.toml` it belongs to (the root project's for the root's file when the
    /// top is a project); `None` for the root's file otherwise.
    pub project: Option<String>,
}

impl ScopedException {
    /// Whether the root's `gate.exceptions.toml` declares it: it then also accepts
    /// repository-scope findings.
    #[must_use]
    pub fn is_root(&self) -> bool {
        !self.file.contains('/')
    }
}

/// The sections of one place that holds project settings: a file, or a profile.
struct Sections<'a> {
    plugins: &'a BTreeMap<PluginName, Spanned<PluginSpec>>,
    languages: &'a BTreeMap<Name, Spanned<serde_json::Value>>,
    extends: &'a [Spanned<PluginItem>],
    /// `[workspace] extends`, which only the root file holds.
    workspace_extends: &'a [Spanned<PluginItem>],
    rules: &'a BTreeMap<RuleKey, Spanned<RuleEntry>>,
    generate: &'a BTreeMap<PluginItem, Spanned<GenerateEntry>>,
    steps: &'a BTreeMap<Name, Spanned<Step>>,
    suites: &'a BTreeMap<Name, Spanned<Suite>>,
}

impl Config {
    /// Reads and checks the `gate.toml` at `path` in the snapshot.
    ///
    /// # Errors
    /// Fails when the file is not tracked, does not parse, or breaks a rule of its kind.
    pub fn load(snapshot: &Snapshot, path: &str, kind: Kind) -> Result<Self> {
        let Some(text) = snapshot.text(path)? else {
            return Err(Error::config(missing(snapshot, path)));
        };
        Self::parse(snapshot, path, kind, &text)
    }

    /// Parses and checks `text` as the `gate.toml` at `path`; options files are read from the
    /// snapshot.
    ///
    /// # Errors
    /// Fails when the text does not parse or breaks a rule of its kind.
    pub fn parse(snapshot: &Snapshot, path: &str, kind: Kind, text: &str) -> Result<Self> {
        let lines = line_starts(text);
        let file: ConfigFile = parse_toml(path, &lines, text)?;
        let directory = path
            .rsplit_once('/')
            .map(|(directory, _)| directory.to_owned())
            .unwrap_or_default();
        let mut config = Self {
            path: path.to_owned(),
            directory,
            kind,
            file,
            positions: Positions::read(text),
            lines,
            own: Settings::default(),
            profiles: Vec::new(),
        };
        config.check()?;

        let file = &config.file;
        let own = config.layer(
            snapshot,
            &Sections {
                plugins: &file.plugins,
                languages: &file.languages,
                extends: file
                    .project
                    .as_ref()
                    .map_or(&[], |project| project.extends.as_slice()),
                workspace_extends: file
                    .workspace
                    .as_ref()
                    .map_or(&[], |workspace| workspace.extends.as_slice()),
                rules: &file.rules,
                generate: &file.generate,
                steps: &file.steps,
                suites: &file.suites,
            },
            None,
        )?;
        let mut profiles = Vec::new();

        for profile in &file.profiles {
            let line = config.line_of(profile.span().start);
            let value = profile.get_ref();
            let globs = ProjectGlobs::compile(&value.projects).map_err(|message| {
                config.error(
                    Some(profile.span().start),
                    format!("[[profiles]] projects: {message}"),
                )
            })?;
            let settings = config.layer(
                snapshot,
                &Sections {
                    plugins: &value.plugins,
                    languages: &value.languages,
                    extends: &value.extends,
                    workspace_extends: &[],
                    rules: &value.rules,
                    generate: &value.generate,
                    steps: &value.steps,
                    suites: &value.suites,
                },
                Some(line),
            )?;
            profiles.push(ProfileLayer {
                line,
                projects: value.projects.clone(),
                globs,
                settings,
            });
        }

        config.own = own;
        config.profiles = profiles;
        Ok(config)
    }

    /// The file's path, relative to the repository root.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The directory the file sits in (`""` for the root file); relative paths in it start here.
    #[must_use]
    pub fn directory(&self) -> &str {
        &self.directory
    }

    /// Which kind of file it is.
    #[must_use]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The parsed file.
    #[must_use]
    pub fn file(&self) -> &ConfigFile {
        &self.file
    }

    /// Where the top-level section `key` starts, when the file has it.
    fn section_at(&self, key: &str) -> Option<usize> {
        self.positions.sections.get(key).copied()
    }

    /// The line of a byte offset in the file.
    #[must_use]
    pub fn line_of(&self, offset: usize) -> usize {
        position(&self.lines, offset).0
    }

    /// What the file itself sets: plugins, presets, rules, generators, steps and suites.
    #[must_use]
    pub fn own(&self) -> &Settings {
        &self.own
    }

    /// The root's `[[profiles]]`, in file order; empty for a project's file.
    #[must_use]
    pub fn profiles(&self) -> &[ProfileLayer] {
        &self.profiles
    }

    /// Every decision the file's rule entries name, as written: those of `[rules]`, then those of
    /// each `[[profiles]]` entry's `rules`, in file order of the profiles and key order within a
    /// table. An entry a later layer overrides still names its decision here.
    #[must_use]
    pub fn rule_decisions(&self) -> Vec<WrittenDecision> {
        let profiles = self
            .file
            .profiles
            .iter()
            .map(|profile| &profile.get_ref().rules);
        std::iter::once(&self.file.rules)
            .chain(profiles)
            .flatten()
            .filter_map(|(key, entry)| {
                let decision = entry.get_ref().decision.clone()?;
                Some(WrittenDecision {
                    key: key.to_string(),
                    decision,
                    line: u32::try_from(self.line_of(entry.span().start)).unwrap_or(u32::MAX),
                })
            })
            .collect()
    }

    /// Whether the file's `[decision-policy]` requires a decision; `None` when it has no policy.
    #[must_use]
    pub fn require_decision(&self) -> Option<bool> {
        self.file
            .decision_policy
            .map(|policy| policy.require_decision)
    }

    /// The path of the exceptions file beside this file.
    #[must_use]
    pub fn exceptions_path(&self) -> String {
        join(&self.directory, EXCEPTIONS_FILE_NAME)
    }

    /// Reads the `gate.exceptions.toml` beside this file from the snapshot; empty when there is
    /// none.
    ///
    /// # Errors
    /// Fails when the file does not parse, an exception has no reason, or a target is not a path
    /// inside the directory.
    pub fn load_exceptions(&self, snapshot: &Snapshot) -> Result<Vec<ScopedException>> {
        let path = self.exceptions_path();
        let Some(text) = snapshot.text(&path)? else {
            return Ok(Vec::new());
        };
        let lines = line_starts(&text);
        let file: ExceptionsFile = parse_toml(&path, &lines, &text)?;
        let mut exceptions = Vec::new();

        for spanned in &file.exception {
            let line = position(&lines, spanned.span().start).0;
            let value = spanned.get_ref();
            let fail = |message: String| {
                Error::config(format!(
                    "{path}:{line}: [[exception]] rule \"{}\": {message}",
                    value.rule
                ))
            };

            if value.reason.trim().is_empty() {
                return Err(fail(
                    "reason is empty; say why the finding is accepted".to_owned(),
                ));
            }

            if value
                .decision
                .as_deref()
                .is_some_and(|decision| decision.trim().is_empty())
            {
                return Err(fail(
                    "decision is empty; name the decision record or remove the key".to_owned(),
                ));
            }

            let target = value.target.trim();
            if target.is_empty() {
                return Err(fail(
                    "target is empty; give the finding's target, or its path".to_owned(),
                ));
            }

            if value
                .owner
                .as_deref()
                .is_some_and(|owner| owner.trim().is_empty())
            {
                return Err(fail(
                    "owner is empty; name the owning record or remove the key".to_owned(),
                ));
            }

            let path_part = target.split(':').next().unwrap_or(target);
            if path_part.starts_with('/') || path_part.split('/').any(|part| part == "..") {
                return Err(fail(format!(
                    "target {target:?} leaves the directory of {path}; write it relative to that directory, or move the exception to the gate.exceptions.toml that holds the path"
                )));
            }

            exceptions.push(ScopedException {
                file: path.clone(),
                line,
                rule: value.rule.clone(),
                target: target.to_owned(),
                path: check_relative(target)
                    .ok()
                    .map(|relative| join(&self.directory, &relative)),
                reason: value.reason.trim().to_owned(),
                decision: value.decision.as_deref().map(str::trim).map(str::to_owned),
                owner: value.owner.as_deref().map(str::trim).map(str::to_owned),
                project: None,
            });
        }

        Ok(exceptions)
    }

    pub(crate) fn error(&self, offset: Option<usize>, message: impl AsRef<str>) -> Error {
        let line = offset.map(|offset| format!(":{}", self.line_of(offset)));
        Error::config(format!(
            "{}{}: {}",
            self.path,
            line.unwrap_or_default(),
            message.as_ref()
        ))
    }

    fn check(&self) -> Result<()> {
        self.check_sections()?;
        for (index, profile) in self.file.profiles.iter().enumerate() {
            let value = profile.get_ref();
            if value.projects.is_empty() {
                return Err(self.error(
                    Some(profile.span().start),
                    format!(
                        "[[profiles]] {}: projects is empty; list globs of the project roots it applies to",
                        index + 1
                    ),
                ));
            }
            self.check_entries(&value.rules, &value.generate, "[[profiles]] ")?;
            self.check_steps(&value.steps, "[[profiles]] ")?;
        }
        self.check_entries(&self.file.rules, &self.file.generate, "")?;
        self.check_steps(&self.file.steps, "")?;
        self.check_commands()
    }

    fn check_sections(&self) -> Result<()> {
        let file = &self.file;

        match self.kind {
            Kind::Root => {
                if file.gate.is_none() {
                    return Err(self.error(
                        None,
                        "[gate] is missing; add [gate] version = \"<the core version>\"",
                    ));
                }
            }
            Kind::Project => {
                for (present, key, section) in [
                    (file.gate.is_some(), "gate", "[gate]"),
                    (file.workspace.is_some(), "workspace", "[workspace]"),
                    (!file.profiles.is_empty(), "profiles", "[[profiles]]"),
                    (file.run.is_some(), "run", "[run]"),
                    (file.hooks.is_some(), "hooks", "[hooks]"),
                    (file.ci.is_some(), "ci", "[ci]"),
                    (!file.tools.is_empty(), "tools", "[tools]"),
                ] {
                    if present {
                        return Err(self.error(
                            self.section_at(key),
                            format!(
                                "{section} is allowed only in the repository root's gate.toml; move it there"
                            ),
                        ));
                    }
                }
            }
        }

        if let Some(workspace) = &file.workspace {
            check_globs(&workspace.ignore).map_err(|message| {
                self.error(
                    self.section_at("workspace"),
                    format!("[workspace] ignore: {message}"),
                )
            })?;

            if let Some(ProjectSources::Globs(globs)) = &workspace.projects {
                if globs.is_empty() {
                    return Err(self.error(
                        self.section_at("workspace"),
                        "[workspace] projects is an empty list; list globs of project roots, or remove the key",
                    ));
                }
                check_globs(globs).map_err(|message| {
                    self.error(
                        self.section_at("workspace"),
                        format!("[workspace] projects: {message}"),
                    )
                })?;
            }
        }

        Ok(())
    }

    fn check_entries(
        &self,
        rules: &BTreeMap<RuleKey, Spanned<RuleEntry>>,
        generate: &BTreeMap<PluginItem, Spanned<GenerateEntry>>,
        prefix: &str,
    ) -> Result<()> {
        for (key, entry) in rules {
            let offset = Some(entry.span().start);
            let value = entry.get_ref();

            if let RuleKey::Family(_) = key {
                if value.norm.is_some()
                    || value.options.is_some()
                    || value.options_file.is_some()
                    || !value.levels.is_empty()
                    || !value.norms.is_empty()
                {
                    return Err(self.error(
                        offset,
                        format!(
                            "{prefix}[rules] \"{key}\": a family wildcard takes only a level and a decision; set norms, aspects and options on each rule"
                        ),
                    ));
                }
                if value.level.is_none() {
                    return Err(self.error(
                        offset,
                        format!("{prefix}[rules] \"{key}\": a family wildcard needs a level"),
                    ));
                }
            }
        }

        for (generator, entry) in generate {
            let value = entry.get_ref();
            if value.options.is_some() && value.options_file.is_some() {
                return Err(self.error(
                    Some(entry.span().start),
                    format!(
                        "{prefix}[generate] \"{generator}\": options and options-file are both set; keep one of them"
                    ),
                ));
            }
        }

        Ok(())
    }

    fn check_steps(&self, steps: &BTreeMap<Name, Spanned<Step>>, prefix: &str) -> Result<()> {
        for (name, step) in steps {
            let offset = Some(step.span().start);
            let value = step.get_ref();

            if value.run.trim().is_empty() {
                return Err(self.error(offset, format!("{prefix}[steps] {name}: run is empty")));
            }
            if value.timeout == Some(0) {
                return Err(self.error(
                    offset,
                    format!(
                        "{prefix}[steps] {name}: timeout is 0; give seconds above 0 or remove it"
                    ),
                ));
            }
            if let Some(glob) = &value.glob {
                if value.files.is_none() {
                    return Err(self.error(
                        offset,
                        format!(
                            "{prefix}[steps] {name}: glob filters the files a step is given; add files = \"staged\" or \"changed\", or remove glob"
                        ),
                    ));
                }
                globset::Glob::new(glob).map_err(|error| {
                    self.error(
                        offset,
                        format!("{prefix}[steps] {name}: glob {glob:?} is not a glob: {error}"),
                    )
                })?;
            }
        }

        Ok(())
    }

    /// The plugins and languages of one place, into `settings`.
    fn layer_plugins(
        &self,
        snapshot: &Snapshot,
        sections: &Sections<'_>,
        profile: Option<usize>,
        settings: &mut Settings,
    ) -> Result<()> {
        let section = |name: &str| section_label(profile, name);
        let origin = |offset: usize, name: &str| self.origin(offset, &section(name));

        for (name, spec) in sections.plugins {
            for read in &spec.get_ref().reads {
                if read.as_str().is_empty() {
                    return Err(self.error(
                        Some(spec.span().start),
                        format!("{}: {name}: reads has an empty path", section("plugins")),
                    ));
                }
            }
            let value = spec.get_ref();
            let options = self.options(
                snapshot,
                value.options.as_ref(),
                value.options_file.as_ref(),
                spec.span().start,
                &format!("{} {name}", section("plugins")),
            )?;
            settings.plugins.insert(
                name.clone(),
                Sourced {
                    value: PluginSettings {
                        source: value.source.clone(),
                        options,
                        reads: value.reads.clone(),
                    },
                    origin: origin(spec.span().start, "plugins"),
                },
            );
        }

        for (id, language) in sections.languages {
            if !language.get_ref().is_object() {
                return Err(self.error(
                    Some(language.span().start),
                    format!(
                        "{} {id}: expected a table of the language's settings, for example [languages.{id}]",
                        section("languages")
                    ),
                ));
            }
            settings.languages.insert(
                id.to_string(),
                Sourced {
                    value: language.get_ref().clone(),
                    origin: origin(language.span().start, "languages"),
                },
            );
        }

        Ok(())
    }

    fn origin(&self, offset: usize, section: &str) -> SettingOrigin {
        SettingOrigin {
            file: self.path.clone(),
            line: self.line_of(offset),
            section: section.to_owned(),
        }
    }

    /// Turns one place's sections into settings with origins, reading options files.
    fn layer(
        &self,
        snapshot: &Snapshot,
        sections: &Sections<'_>,
        profile: Option<usize>,
    ) -> Result<Settings> {
        let section = |name: &str| section_label(profile, name);
        let origin = |offset: usize, name: &str| self.origin(offset, &section(name));
        let mut settings = Settings::default();
        self.layer_plugins(snapshot, sections, profile, &mut settings)?;

        for preset in sections.extends {
            settings.extends.push(Sourced {
                value: preset.get_ref().clone(),
                origin: origin(preset.span().start, "extends"),
            });
        }
        for preset in sections.workspace_extends {
            settings.extends.push(Sourced {
                value: preset.get_ref().clone(),
                origin: self.origin(preset.span().start, WORKSPACE_EXTENDS),
            });
        }

        for (key, entry) in sections.rules {
            let at = origin(entry.span().start, "rules");
            let value = entry.get_ref();
            let rule = match key {
                RuleKey::Family(plugin) => {
                    if let Some(level) = value.level {
                        settings.families.insert(
                            plugin.clone(),
                            LevelSetting {
                                value: level,
                                origin: at,
                                decision: value.decision.clone(),
                            },
                        );
                    }
                    continue;
                }
                RuleKey::Rule(rule) => rule,
            };
            let options = self.options(
                snapshot,
                value.options.as_ref(),
                value.options_file.as_ref(),
                entry.span().start,
                &format!("{} \"{key}\"", section("rules")),
            )?;
            settings
                .rules
                .insert(rule.clone(), rule_settings(value, options, &at));
        }

        for (generator, entry) in sections.generate {
            let value = entry.get_ref();
            let options = self.options(
                snapshot,
                value.options.as_ref(),
                value.options_file.as_ref(),
                entry.span().start,
                &format!("{} \"{generator}\"", section("generate")),
            )?;
            settings.generate.insert(
                generator.clone(),
                Sourced {
                    value: options,
                    origin: origin(entry.span().start, "generate"),
                },
            );
        }

        for (name, step) in sections.steps {
            settings.steps.insert(
                name.clone(),
                Sourced {
                    value: step.get_ref().clone(),
                    origin: origin(step.span().start, "steps"),
                },
            );
        }

        for (name, suite) in sections.suites {
            if suite.get_ref().steps.is_empty() {
                return Err(self.error(
                    Some(suite.span().start),
                    format!(
                        "{} {name}: the suite is empty; list its steps",
                        section("suites")
                    ),
                ));
            }
            settings.suites.insert(
                name.clone(),
                Sourced {
                    value: suite.get_ref().clone(),
                    origin: origin(suite.span().start, "suites"),
                },
            );
        }

        Ok(settings)
    }

    fn check_commands(&self) -> Result<()> {
        let file = &self.file;

        if let Some(hooks) = &file.hooks {
            if let Some(problem) = hooks.budget.as_ref().and_then(check_budget) {
                return Err(self.error(
                    self.section_at("hooks"),
                    format!("[hooks] budget: {problem}"),
                ));
            }
            for (hook, commands) in &hooks.hooks {
                if commands.is_empty() {
                    return Err(self.error(
                        self.section_at("hooks"),
                        format!(
                            "[hooks] {}: list the gate commands the hook runs",
                            hook.as_str()
                        ),
                    ));
                }
                for command in commands {
                    check_gate_command(command).map_err(|message| {
                        self.error(
                            self.section_at("hooks"),
                            format!("[hooks] {}: {message}", hook.as_str()),
                        )
                    })?;
                }
            }
        }

        if let Some(wrapper) = file.run.as_ref().and_then(|run| run.heavy_wrapper.as_ref())
            && wrapper.trim().is_empty()
        {
            return Err(self.error(
                self.section_at("run"),
                "[run] heavy-wrapper is empty; name the command that queues heavy steps, or remove the key",
            ));
        }
        if let Some(github) = file.ci.as_ref().and_then(|ci| ci.github.as_ref()) {
            self.check_github(github)?;
        }

        for (name, tool) in &file.tools {
            if tool.targets.is_empty() {
                return Err(self.error(
                    self.positions.tools.get(name.as_str()).copied(),
                    format!("[tools] {name}: targets is empty; add one archive per platform"),
                ));
            }
            for (target, archive) in &tool.targets {
                if let Err(message) = crate::fetch::check_url(&archive.url) {
                    return Err(self.error(
                        self.positions.tools.get(name.as_str()).copied(),
                        format!("[tools] {name}.targets.{target}: {message}"),
                    ));
                }
                if archive
                    .bin
                    .as_deref()
                    .is_some_and(|bin| check_relative(bin).is_err())
                {
                    return Err(self.error(
                        None,
                        format!(
                            "[tools] {name}.targets.{target}: bin must be a relative path inside the archive"
                        ),
                    ));
                }
                if archive.bin.is_none() && is_archive(&archive.url) {
                    return Err(self.error(
                        None,
                        format!(
                            "[tools] {name}.targets.{target}: the url is an archive; add bin = \"<path of the executable inside it>\""
                        ),
                    ));
                }
                if archive.sha256.len() != 64
                    || !archive
                        .sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                {
                    return Err(self.error(
                        self.positions.tools.get(name.as_str()).copied(),
                        format!(
                            "[tools] {name}.targets.{target}: sha256 must be 64 lowercase hex digits"
                        ),
                    ));
                }
            }
        }

        Ok(())
    }

    fn check_github(&self, github: &CiGithub) -> Result<()> {
        let fail =
            |message: String| self.error(self.section_at("ci"), format!("[ci.github] {message}"));

        check_pinned(&github.checkout).map_err(|message| fail(format!("checkout: {message}")))?;
        if let Some(name) = &github.name
            && name.trim().is_empty()
        {
            return Err(fail(
                "name is empty; name the workflow or remove the key".to_owned(),
            ));
        }
        if github
            .branches
            .iter()
            .any(|branch| branch.trim().is_empty())
        {
            return Err(fail("branches has an empty name".to_owned()));
        }
        for (index, step) in github.setup.iter().enumerate() {
            check_ci_step(step, false)
                .map_err(|message| fail(format!("setup {}: {message}", index + 1)))?;
        }
        if github.jobs.is_empty() {
            return Err(fail(
                "jobs is empty; add [ci.github.jobs.<id>] with name and run".to_owned(),
            ));
        }
        if github.jobs.keys().any(|id| id.as_str() == CHANGES_JOB) {
            return Err(fail(
                "jobs.changes: the id is the generated job that lists the changed projects; choose another id"
                    .to_owned(),
            ));
        }
        check_aggregate(github).map_err(fail)?;

        for (id, job) in &github.jobs {
            let at = self
                .positions
                .jobs
                .get(id.as_str())
                .copied()
                .or(self.section_at("ci"));
            let fail =
                |message: String| self.error(at, format!("[ci.github] jobs.{id}: {message}"));
            check_ci_job(job).map_err(fail)?;
        }

        Ok(())
    }

    /// Inline options, or the options read from `options_file`; `None` when neither is set.
    fn options(
        &self,
        snapshot: &Snapshot,
        inline: Option<&serde_json::Value>,
        options_file: Option<&RelativePath>,
        offset: usize,
        what: &str,
    ) -> Result<Option<Options>> {
        match (inline, options_file) {
            (Some(options), _) => Ok(Some(Options {
                value: options.clone(),
                file: None,
            })),
            (None, Some(file)) => self
                .read_json(snapshot, file, offset, &format!("{what} options-file"))
                .map(Some),
            (None, None) => Ok(None),
        }
    }

    fn read_json(
        &self,
        snapshot: &Snapshot,
        file: &RelativePath,
        offset: usize,
        what: &str,
    ) -> Result<Options> {
        let path = join(&self.directory, file.as_str());
        let Some(text) = snapshot.text(&path)? else {
            return Err(self.error(
                Some(offset),
                format!("{what}: {path} is not a tracked file; add it or fix the path"),
            ));
        };

        let value = serde_json::from_str(&text).map_err(|error| {
            self.error(
                Some(offset),
                format!(
                    "{what}: {path}:{}:{}: {error}",
                    error.line(),
                    error.column()
                ),
            )
        })?;
        Ok(Options {
            value,
            file: Some(path),
        })
    }
}

/// The section an origin of `[workspace] extends` shows.
const WORKSPACE_EXTENDS: &str = "[workspace] extends";

/// The section name an origin shows: `[rules]`, `[project] extends`, `[[profiles]] rules`, ...
fn section_label(profile: Option<usize>, name: &str) -> String {
    match (profile, name) {
        (Some(_), _) => format!("[[profiles]] {name}"),
        (None, "extends") => "[project] extends".to_owned(),
        (None, _) => format!("[{name}]"),
    }
}

/// One rule entry's fields, each with the place it was written.
fn rule_settings(value: &RuleEntry, options: Option<Options>, at: &SettingOrigin) -> RuleSettings {
    let value_decision = value.decision.clone();
    let level = |value: Setting| LevelSetting {
        value,
        origin: at.clone(),
        decision: value_decision.clone(),
    };
    RuleSettings {
        level: value.level.map(level),
        norm: value.norm.clone().map(|norm| Sourced {
            value: norm,
            origin: at.clone(),
        }),
        levels: value
            .levels
            .iter()
            .map(|(aspect, setting)| (aspect.to_string(), level(*setting)))
            .collect(),
        norms: value
            .norms
            .iter()
            .map(|(aspect, norm)| {
                (
                    aspect.to_string(),
                    Sourced {
                        value: norm.clone(),
                        origin: at.clone(),
                    },
                )
            })
            .collect(),
        options: options.map(|options| Sourced {
            value: options,
            origin: at.clone(),
        }),
    }
}

/// Checks JSON values against JSON Schemas. Plugins declare the schemas of their rules' options;
/// the plugin runtime provides the implementation.
pub trait SchemaValidator {
    /// Every way `value` breaks `schema`, each as one line naming the location in the value; empty
    /// when it conforms.
    fn validate(&self, schema: &serde_json::Value, value: &serde_json::Value) -> Vec<String>;
}

/// The JSON Schema of `gate.toml`, generated from the configuration types.
#[must_use]
pub fn json_schema() -> serde_json::Value {
    let mut schema = schemars::schema_for!(ConfigFile).to_value();
    remove_null(&mut schema);
    schema
}

/// The JSON Schema of `gate.exceptions.toml`.
#[must_use]
pub fn exceptions_json_schema() -> serde_json::Value {
    let mut schema = schemars::schema_for!(ExceptionsFile).to_value();
    remove_null(&mut schema);
    schema
}

/// TOML has no null: an optional key is absent, never null. Drops the null alternatives the
/// generator adds for optional fields.
fn remove_null(value: &mut serde_json::Value) {
    use serde_json::Value;

    match value {
        Value::Object(object) => {
            if let Some(Value::Array(types)) = object.get_mut("type") {
                types.retain(|kind| kind != "null");
                if let [single] = types.as_slice() {
                    let single = single.clone();
                    object.insert("type".to_owned(), single);
                }
            }

            if let Some(Value::Array(choices)) = object.get_mut("enum") {
                choices.retain(|choice| !choice.is_null());
            }

            if let Some(Value::Array(alternatives)) = object.get_mut("anyOf") {
                alternatives
                    .retain(|alternative| alternative.get("type") != Some(&Value::from("null")));
                if let [single] = alternatives.as_slice() {
                    let single = single.clone();
                    object.remove("anyOf");
                    if let Value::Object(inner) = single {
                        for (key, inner_value) in inner {
                            object.entry(key).or_insert(inner_value);
                        }
                    }
                }
            }

            for inner in object.values_mut() {
                remove_null(inner);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(remove_null),
        _ => {}
    }
}

/// The message for a reference to a plugin the settings do not declare.
pub(crate) fn undeclared(plugin: &str, scope: &str) -> String {
    if plugin == CORE_PLUGIN {
        format!("\"{CORE_PLUGIN}/...\" rules are the core's own and cannot be configured")
    } else {
        format!(
            "plugin {plugin} is not declared for {scope}; add [plugins.{plugin}] source = \"<owner>/<repo>:{plugin}@<version>\""
        )
    }
}

fn missing(snapshot: &Snapshot, path: &str) -> String {
    if snapshot.git().root().join(path).is_file() {
        format!("{path} is not tracked; gate reads tracked files only, run git add {path}")
    } else {
        format!("{path} is missing; run gate init to write one")
    }
}

/// Parses TOML into `T`, with errors as `path:line:column: message`.
fn parse_toml<T: serde::de::DeserializeOwned>(
    path: &str,
    lines: &[usize],
    text: &str,
) -> Result<T> {
    toml::from_str(text).map_err(|error| {
        let at = error
            .span()
            .map(|span| {
                let (line, column) = position(lines, span.start);
                format!(":{line}:{column}")
            })
            .unwrap_or_default();
        Error::config(format!("{path}{at}: {}", hint(error.message().trim_end())))
    })
}

/// Joins a directory and a relative path with `/`.
#[must_use]
pub fn join(directory: &str, path: &str) -> String {
    let path = path.strip_prefix("./").unwrap_or(path);

    if directory.is_empty() {
        path.to_owned()
    } else {
        format!("{directory}/{path}")
    }
}

/// What is wrong with a `budget` table, if anything: it sets no threshold, one is 0, or
/// `fail-after` is not above `warn-after`.
fn check_budget(budget: &Budget) -> Option<String> {
    match (budget.warn_after, budget.fail_after) {
        (None, None) => {
            Some("set warn-after, fail-after or both, in seconds, or remove budget".to_owned())
        }
        (Some(0), _) => Some("warn-after is 0; give seconds above 0 or remove it".to_owned()),
        (_, Some(0)) => Some("fail-after is 0; give seconds above 0 or remove it".to_owned()),
        (Some(warn), Some(fail)) if fail <= warn => Some(format!(
            "fail-after ({fail}) is not above warn-after ({warn}); a check warns first and fails later, so raise fail-after or lower warn-after"
        )),
        _ => None,
    }
}

/// Checks a gate command as `[hooks]` and CI jobs write it: `check ...`, `sync --check` or
/// `run <step or suite> [--staged | --push]`, gate arguments only (no shell syntax).
///
/// # Errors
/// Fails, saying what is expected, when the command is empty, names another command, holds shell
/// syntax, or combines `--commit-message` with a flag that picks a tree.
pub fn check_gate_command(command: &str) -> std::result::Result<(), String> {
    let words: Vec<&str> = command.split_whitespace().collect();
    if let Some(word) = words.iter().find(|word| {
        word.chars()
            .any(|character| SHELL_SYNTAX.contains(&character))
    }) {
        return Err(format!(
            "{command:?}: {word:?} is shell syntax; a hook or job entry holds gate arguments only, a shell command belongs in [steps]"
        ));
    }
    for flag in ["--push", "--commit-message"] {
        let attached = words.iter().any(|word| {
            word.strip_prefix(flag)
                .is_some_and(|rest| rest.starts_with('='))
        });
        let separate = words
            .iter()
            .position(|word| *word == flag)
            .is_some_and(|at| words.get(at + 1).is_some_and(|next| !next.starts_with('-')));
        if attached || separate {
            return Err(format!(
                "{command:?}: write {flag} without a value; the hook or the job supplies it"
            ));
        }
    }
    if words.contains(&"--commit-message")
        && let Some(selector) = words.iter().find_map(|word| {
            ["--staged", "--changed", "--base", "--push", "--rev"]
                .into_iter()
                .find(|flag| {
                    word == flag
                        || word
                            .strip_prefix(flag)
                            .is_some_and(|rest| rest.starts_with('='))
                })
        })
    {
        return Err(format!(
            "{command:?}: remove {selector}; --commit-message checks only the message being written, and the tree is checked by the pre-commit hook (check --staged), the pre-push hook (check --push) and CI"
        ));
    }
    if words
        .iter()
        .any(|word| *word == "--hook" || word.starts_with("--hook="))
    {
        return Err(format!(
            "{command:?}: remove --hook; the generated hooks pass it to every check, and a job is not a hook"
        ));
    }
    if words
        .iter()
        .any(|word| *word == "--target-ref" || word.starts_with("--target-ref="))
    {
        return Err(format!(
            "{command:?}: remove --target-ref; the hook or the job passes it with --push"
        ));
    }
    match words.as_slice() {
        [] => Err("an entry is empty; write a gate command such as \"check --staged\"".to_owned()),
        ["check", ..] | ["sync", "--check"] => Ok(()),
        ["run", name, flags @ ..] if flags.len() <= 1 => {
            Name::try_from((*name).to_owned())
                .map_err(|message| format!("{command:?}: {message}"))?;
            match flags {
                [] | ["--staged" | "--push"] => Ok(()),
                _ => Err(format!(
                    "{command:?}: after the step or suite, run takes --staged or --push"
                )),
            }
        }
        ["run", ..] => Err(format!(
            "{command:?}: run takes the name of one step or suite, then --staged or --push"
        )),
        [other, ..] => Err(format!(
            "{command:?}: {other:?} is not a command an entry may run; use check, run <step> or sync --check"
        )),
    }
}

/// Characters a gate command never holds, so that it can be written into a hook or a workflow
/// unquoted.
const SHELL_SYNTAX: &[char] = &[
    ';', '|', '&', '$', '<', '>', '`', '\'', '"', '\\', '(', ')', '*', '?', '[', ']', '{', '}',
    '#', '~',
];

/// Checks an action reference: pinned by commit, optionally followed by ` # ` and the release the
/// commit is, which the workflow carries as a comment.
fn check_pinned(uses: &str) -> std::result::Result<(), String> {
    let (pin, release) = uses
        .split_once(" # ")
        .map_or((uses, None), |(pin, release)| (pin, Some(release)));
    if let Some(release) = release
        && (release.trim().is_empty() || release.chars().any(char::is_control))
    {
        return Err(format!(
            "{uses:?}: the release after \" # \" is empty or spans lines; write it on one line, as v4.2.2"
        ));
    }
    let pinned = pin.split_once('@').is_some_and(|(action, commit)| {
        let mut parts = action.split('/');
        let owner = parts.next().unwrap_or_default();
        let repo = parts.next().unwrap_or_default();
        !owner.is_empty()
            && !repo.is_empty()
            && parts.all(|part| !part.is_empty())
            && commit.len() == 40
            && commit
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    });
    if pinned {
        Ok(())
    } else {
        Err(format!(
            "{uses:?} is not pinned by commit; write <owner>/<repo>@<40 lowercase hex digits of the commit> # <release>"
        ))
    }
}

/// The `[ci.github] aggregate` id is a job of its own: not the generated `changes` job, not a job
/// of `[ci.github.jobs]`.
fn check_aggregate(github: &CiGithub) -> std::result::Result<(), String> {
    let Some(aggregate) = &github.aggregate else {
        return Ok(());
    };
    if aggregate.as_str() == CHANGES_JOB {
        return Err(format!(
            "aggregate = \"{aggregate}\": the id is the generated job that lists the changed projects; choose another id"
        ));
    }
    if github.jobs.contains_key(aggregate) {
        return Err(format!(
            "aggregate = \"{aggregate}\": the id is a job of [ci.github.jobs]; the aggregate is a job of its own, so choose an id no job has"
        ));
    }
    Ok(())
}

/// The checks of one `[ci.github.jobs.<id>]` table; the error is the message after the job's
/// name.
fn check_ci_job(job: &CiJob) -> std::result::Result<(), String> {
    if job.name.trim().is_empty() {
        return Err("name is empty".to_owned());
    }
    if job.run.is_empty() {
        return Err("run is empty; list the gate commands the job runs".to_owned());
    }
    for command in &job.run {
        check_gate_command(command)?;
        if command
            .split_whitespace()
            .any(|word| word == "--commit-message" || word.starts_with("--commit-message="))
        {
            return Err(format!(
                "{command:?}: a job has no commit message file; --commit-message belongs in a [hooks] commit-msg entry"
            ));
        }
    }
    if let Some(problem) = job.budget.as_ref().and_then(check_budget) {
        return Err(format!("budget: {problem}"));
    }
    match job.timeout_minutes {
        None => {
            return Err(
                "timeout-minutes is not set; give the job's time limit in minutes \
                 (timeout-minutes = 30), a little above its usual time"
                    .to_owned(),
            );
        }
        Some(0) => {
            return Err("timeout-minutes is 0; give minutes above 0".to_owned());
        }
        Some(_) => {}
    }
    if job
        .environment
        .as_ref()
        .is_some_and(|environment| environment.trim().is_empty())
    {
        return Err("environment is empty; name the environment or remove the key".to_owned());
    }
    if job.projects.iter().any(|project| project.trim().is_empty()) {
        return Err("projects has an empty name".to_owned());
    }
    for (index, step) in job.steps.iter().enumerate() {
        check_ci_step(step, false).map_err(|message| format!("steps {}: {message}", index + 1))?;
    }
    for (index, step) in job.after.iter().enumerate() {
        check_ci_step(step, true).map_err(|message| format!("after {}: {message}", index + 1))?;
    }
    for service in job.services.values() {
        if !service.is_object() {
            return Err(
                "services: each service is a table (image, ports, env, options)".to_owned(),
            );
        }
    }

    Ok(())
}

fn check_ci_step(step: &CiStep, after: bool) -> std::result::Result<(), String> {
    if let Some(condition) = &step.condition {
        if !after {
            return Err(
                "if belongs to an after step; a step before the gate commands always runs"
                    .to_owned(),
            );
        }
        if condition.trim().is_empty() {
            return Err("if is empty; write a GitHub expression or remove it".to_owned());
        }
    }
    if step.working_directory.is_some() && step.run.is_none() {
        return Err(
            "working-directory belongs to a run step; an action takes its directory as an input"
                .to_owned(),
        );
    }
    match (&step.uses, &step.run) {
        (Some(uses), None) => check_pinned(uses),
        (None, Some(run)) if !run.trim().is_empty() => {
            if step.with.is_empty() {
                Ok(())
            } else {
                Err("with belongs to a uses step".to_owned())
            }
        }
        (None, Some(_)) => Err("run is empty".to_owned()),
        (Some(_), Some(_)) => Err("set uses or run, not both".to_owned()),
        (None, None) => Err("set uses (an action pinned by commit) or run (a command)".to_owned()),
    }
}

/// Whether a tool's URL is an archive gate unpacks.
#[must_use]
pub fn is_archive(url: &str) -> bool {
    [".tar.gz", ".tgz", ".tar.xz", ".tar.bz2", ".tar", ".zip"]
        .iter()
        .any(|extension| url.ends_with(extension))
}

fn check_globs(globs: &[String]) -> std::result::Result<(), String> {
    for glob in globs {
        let pattern = glob.strip_prefix('!').unwrap_or(glob);
        globset::Glob::new(pattern).map_err(|error| format!("{glob:?} is not a glob: {error}"))?;
    }
    Ok(())
}

fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(index, _)| index + 1))
        .collect()
}

/// The 1-based line and column of a byte offset.
fn position(lines: &[usize], offset: usize) -> (usize, usize) {
    let line = lines.partition_point(|start| *start <= offset);
    let start = lines.get(line.saturating_sub(1)).copied().unwrap_or(0);
    (line.max(1), offset.saturating_sub(start) + 1)
}

/// Adds a hint to the parser's message where the parser's wording alone leaves the fix unclear.
fn hint(message: &str) -> String {
    if message.contains("unknown field `extends`") {
        format!(
            "{message}; presets go in [project] extends, in [workspace] extends at a root that is not a project, or in a [[profiles]] entry"
        )
    } else if message.contains("unknown field `exceptions`") {
        format!(
            "{message}; exceptions live in gate.exceptions.toml beside this file, as [[exception]] entries"
        )
    } else {
        message.to_owned()
    }
}

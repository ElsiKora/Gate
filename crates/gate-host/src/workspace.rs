//! A repository opened for a run: its snapshot, its configuration files, its projects with the
//! settings that apply to each, its exceptions and its baseline.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use crate::baseline::Baseline;
use crate::config::{
    self, CORE_PLUGIN, CORE_RULES, Config, FILE_NAME, Kind, Name, PluginItem, PluginName,
    PluginSource, RuleId, RuleKey, RuleSettings, SchemaValidator, ScopedException, Setting,
    SettingOrigin, Settings, Sourced, Tool, WORKSPACE_EXTENDS, undeclared,
    undeclared_workspace_preset,
};
use crate::error::{Error, Result};
use crate::findings::RuleFilter;
use crate::git::Git;
use crate::projects::{self, Projects};
use crate::snapshot::{Snapshot, Source};

/// Where a rule runs, as its plugin declares it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleScope {
    /// Once for the whole repository.
    Repository,
    /// Once per project, over the project's subtree.
    Project,
}

/// What a plugin declares about one of its rules: what configuration is checked against.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleDeclaration {
    /// Where the rule runs.
    pub scope: RuleScope,
    /// The rule's aspects; empty when it has none.
    pub aspects: Vec<String>,
    /// The JSON Schema of the rule's options.
    pub options_schema: serde_json::Value,
}

/// The options the presets a scope extends give a rule, which the run uses when the rule's
/// `[rules]` entry gives none.
#[derive(Clone, Debug, PartialEq)]
pub struct PresetOptions {
    /// The options.
    pub options: serde_json::Value,
    /// The `extends` entry of the preset that gives them, the latest that sets the rule.
    pub preset: Sourced<PluginItem>,
}

/// A repository opened for a run. A clone shares its snapshot and configuration.
#[derive(Clone, Debug)]
pub struct Workspace {
    snapshot: Arc<Snapshot>,
    root: Arc<Config>,
    projects: Projects,
    configs: Arc<BTreeMap<String, Config>>,
    settings: BTreeMap<String, Settings>,
    exceptions: Vec<ScopedException>,
    baseline: Baseline,
    inputs: Inputs,
}

/// The files a run's configuration was read from: a change to one selects more than the project
/// that owns it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Inputs {
    /// Read by the root's configuration (its `gate.toml`, profiles' and plugins' options files),
    /// by project discovery outside every project (workspace manifests), and the baseline: a
    /// change to one selects every project.
    pub everywhere: BTreeSet<String>,
    /// By project name: the files its own configuration was read from, and the paths its plugins
    /// read (`reads`, a file or a directory): a change to one selects the project.
    pub projects: BTreeMap<String, BTreeSet<String>>,
}

impl Inputs {
    /// The first changed path (in `changes`, sorted) that selects every project.
    #[must_use]
    pub fn everywhere_changed<'a>(&self, changes: &'a BTreeSet<String>) -> Option<&'a str> {
        changes
            .iter()
            .find(|path| self.everywhere.contains(*path))
            .map(String::as_str)
    }

    /// The first changed path that is an input of the project named `name`.
    #[must_use]
    pub fn project_changed<'a>(
        &self,
        name: &str,
        changes: &'a BTreeSet<String>,
    ) -> Option<&'a str> {
        let inputs = self.projects.get(name)?;
        changes
            .iter()
            .find(|path| {
                inputs.iter().any(|input| {
                    *path == input
                        || path
                            .strip_prefix(input.as_str())
                            .is_some_and(|rest| rest.starts_with('/'))
                })
            })
            .map(String::as_str)
    }
}

impl Workspace {
    /// Opens the repository that contains `start` at `source`.
    ///
    /// # Errors
    /// Fails when the repository, a revision or a file cannot be read (environment), or a
    /// configuration file is wrong (configuration).
    pub fn open(start: &Path, source: Source) -> Result<Self> {
        let git = Git::discover(start)?;
        let snapshot = Snapshot::open(&git, source)?;
        Self::from_snapshot(snapshot)
    }

    /// Loads the configuration, projects, exceptions and baseline of an open snapshot.
    ///
    /// # Errors
    /// Fails when a configuration file is wrong, refers to a plugin its project does not declare,
    /// names hooks or CI jobs `gate sync` would refuse, or a workspace file the configuration names
    /// is missing.
    pub fn from_snapshot(snapshot: Snapshot) -> Result<Self> {
        let root = Config::load(&snapshot, FILE_NAME, Kind::Root)?;
        let mut everywhere = snapshot.take_asked();
        snapshot.record();
        let workspace = root.file().workspace.as_ref();
        let mut by_directory = BTreeMap::new();
        let mut asked_by_directory = BTreeMap::new();

        for path in projects::project_config_paths(&snapshot, workspace)? {
            let loaded = Config::load(&snapshot, &path, Kind::Project)?;
            asked_by_directory.insert(loaded.directory().to_owned(), snapshot.take_asked());
            snapshot.record();
            by_directory.insert(loaded.directory().to_owned(), loaded);
        }

        let projects = projects::discover(&snapshot, &root, &by_directory)?;
        let unowned = |path: &String| {
            projects
                .owner(path)
                .is_none_or(|project| project.root.is_empty())
        };
        everywhere.extend(
            snapshot
                .take_asked()
                .into_iter()
                .filter(|path| unowned(path)),
        );
        snapshot.record();
        let root_is_project = projects.iter().any(|project| project.root.is_empty());

        if root_is_project {
            check_root_project(&root)?;
        } else {
            check_workspace_root(&root)?;
        }

        let mut configs = BTreeMap::new();
        for (directory, loaded) in by_directory {
            if let Some(project) = projects.iter().find(|project| project.root == directory) {
                configs.insert(project.name.clone(), loaded);
            }
        }

        check_settings(root.own(), &root.file().tools, "the repository")?;

        let (settings, inputs) = merge_settings(&root, &projects, &configs, asked_by_directory)?;

        let mut opened = Self {
            snapshot: Arc::new(snapshot),
            root: Arc::new(root),
            projects,
            configs: Arc::new(configs),
            settings,
            exceptions: Vec::new(),
            baseline: Baseline::default(),
            inputs: Inputs::default(),
        };
        opened.exceptions = opened.load_exceptions()?;
        opened.baseline = Baseline::load(&opened.snapshot)?;
        // gate.lock pins every plugin: a moved pin selects every project.
        opened.snapshot.read(crate::lock::LOCK_FILE)?;
        let asked = opened.snapshot.take_asked();
        everywhere.extend(asked.into_iter().filter(|path| {
            opened
                .projects
                .owner(path)
                .is_none_or(|project| project.root.is_empty())
        }));
        opened.inputs = Inputs {
            everywhere,
            projects: inputs,
        };
        crate::sync::check_core_outputs(&opened)?;
        Ok(opened)
    }

    /// This configuration over the files of another snapshot: `git.sync-state` at a commit whose
    /// own configuration this core does not read runs the run's configuration on that commit.
    #[must_use]
    pub(crate) fn with_snapshot(&self, snapshot: Snapshot) -> Self {
        Self {
            snapshot: Arc::new(snapshot),
            ..self.clone()
        }
    }

    /// The snapshot of tracked files.
    #[must_use]
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// The snapshot, shared with the plugin runtime's calls.
    #[must_use]
    pub fn shared_snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&self.snapshot)
    }

    /// The repository root's configuration.
    #[must_use]
    pub fn root_config(&self) -> &Config {
        &self.root
    }

    /// Whether the repository top is a project (no `[workspace]`, or one that lists `"."`).
    #[must_use]
    pub fn root_is_project(&self) -> bool {
        self.projects.iter().any(|project| project.root.is_empty())
    }

    /// The projects.
    #[must_use]
    pub fn projects(&self) -> &Projects {
        &self.projects
    }

    /// The `gate.toml` of the project named `name`, when it has one (the root's for the top).
    #[must_use]
    pub fn project_config(&self, name: &str) -> Option<&Config> {
        match self.projects.get(name) {
            Some(project) if project.root.is_empty() => Some(&*self.root),
            _ => self.configs.get(name),
        }
    }

    /// Every configuration file: the root's, then the projects' by project name, each with its
    /// project's name (`None` for the root).
    pub fn configs(&self) -> impl Iterator<Item = (Option<&str>, &Config)> {
        std::iter::once((None, &*self.root)).chain(
            self.configs
                .iter()
                .map(|(name, loaded)| (Some(name.as_str()), loaded)),
        )
    }

    /// The `gate.toml` whose `[decision-policy]` requires a decision for what the configuration
    /// file `file` writes (its rule entries, and the exceptions beside it): the file's own policy
    /// when it is a project's file that sets one, else the root's. `None` when no policy requires
    /// one.
    #[must_use]
    pub fn decision_policy(&self, file: &str) -> Option<String> {
        let own = self
            .configs()
            .find(|(_, loaded)| loaded.path() == file && !loaded.directory().is_empty())
            .and_then(|(_, loaded)| loaded.require_decision());
        match own {
            Some(required) => required.then(|| file.to_owned()),
            None => self
                .root
                .require_decision()
                .is_some_and(|required| required)
                .then(|| FILE_NAME.to_owned()),
        }
    }

    /// What the root `gate.toml` sets: the repository's plugins and repository-scope rules (and,
    /// when the root is a project, its project settings too).
    #[must_use]
    pub fn repository(&self) -> &Settings {
        self.root.own()
    }

    /// The settings of the project named `name`: the root's `[plugins]`, the matching profiles in
    /// order, then its own `gate.toml`.
    #[must_use]
    pub fn settings(&self, name: &str) -> Option<&Settings> {
        self.settings.get(name)
    }

    /// Every exception of every `gate.exceptions.toml`, the root's first, then by project name.
    #[must_use]
    pub fn exceptions(&self) -> &[ScopedException] {
        &self.exceptions
    }

    /// The baseline.
    #[must_use]
    pub fn baseline(&self) -> &Baseline {
        &self.baseline
    }

    /// The files the configuration was read from, for change selection.
    #[must_use]
    pub fn inputs(&self) -> &Inputs {
        &self.inputs
    }

    /// Checks that every `--rule` and `--skip-rule` key of `filter` names a configured rule: its
    /// plugin is declared in some scope, and for a single rule, a `[rules]` entry of such a scope
    /// names it or the plugin declares it (`declaration`, the plugin runtime's answer).
    ///
    /// # Errors
    /// Fails on the first key that names a core rule or no configured rule.
    pub fn check_rule_filter(
        &self,
        filter: &RuleFilter,
        declaration: &dyn Fn(&PluginName, &PluginSource, &str) -> Option<RuleDeclaration>,
    ) -> Result<()> {
        let scopes: Vec<&Settings> = std::iter::once(self.repository())
            .chain(self.settings.values())
            .collect();

        let keys = filter
            .keys()
            .iter()
            .map(|key| ("--rule", key))
            .chain(filter.skip_keys().iter().map(|key| ("--skip-rule", key)));
        for (flag, key) in keys {
            let plugin = key.plugin();
            if plugin == CORE_PLUGIN {
                return Err(Error::config(format!(
                    "{flag} {key}: the core's own rules always run; name a plugin's rule or <plugin>/*"
                )));
            }
            let declared: Vec<(&Settings, &PluginName, &PluginSource)> = scopes
                .iter()
                .filter_map(|settings| {
                    settings
                        .plugins
                        .iter()
                        .find(|(name, _)| name.as_str() == plugin)
                        .map(|(name, spec)| (*settings, name, &spec.value.source))
                })
                .collect();
            if declared.is_empty() {
                return Err(Error::config(format!(
                    "{flag} {key} names no configured rule: no gate.toml declares plugin {plugin}; gate list prints the configured rules"
                )));
            }
            if let RuleKey::Rule(rule) = key {
                let configured = declared.iter().any(|(settings, name, source)| {
                    settings.rules.contains_key(rule)
                        || declaration(name, source, rule.rule()).is_some()
                });
                if !configured {
                    return Err(Error::config(format!(
                        "{flag} {rule} names no configured rule: no [rules] entry names it and plugin {plugin} declares no rule {}; gate list prints the configured rules",
                        rule.rule()
                    )));
                }
            }
        }

        Ok(())
    }

    /// Checks the `[plugins.<name>] options` of every declared plugin entry, in the root and in
    /// each project, against the schema its plugin declares (`schema`, `None` for a plugin that did
    /// not load, which is skipped). An entry without options is checked as `{}`, so a required
    /// option is reported missing.
    ///
    /// # Errors
    /// Fails on the first entry whose options break the schema, naming the file, line and plugin.
    pub fn validate_plugin_options(
        &self,
        schema: &dyn Fn(&PluginName) -> Option<serde_json::Value>,
        validator: &dyn SchemaValidator,
    ) -> Result<()> {
        let empty = serde_json::Value::Object(serde_json::Map::new());
        for settings in std::iter::once(self.repository()).chain(self.settings.values()) {
            for (name, entry) in &settings.plugins {
                let Some(schema) = schema(name) else {
                    continue;
                };
                let options = entry.value.options.as_ref();
                let value = options.map_or(&empty, |options| &options.value);
                let violations = validator.validate(&schema, value);
                if violations.is_empty() {
                    continue;
                }
                let from = options
                    .and_then(|options| options.file.as_ref())
                    .map(|file| format!(" (from {file})"))
                    .unwrap_or_default();
                let problem = missing_required(name, &entry.origin.file, &schema, value)
                    .unwrap_or_else(|| violations.join("; "));
                return Err(at(
                    Some(&entry.origin),
                    format!("plugin {name} options{from}: {problem}"),
                ));
            }
        }
        Ok(())
    }

    /// Checks the configuration against what the plugins declare.
    ///
    /// `declaration` returns what the plugin with the given name and source declares about a rule,
    /// or `None` when it has no such rule. `presets` returns the options the presets a scope
    /// extends give a rule, or `None` when they give none. Unknown rules, aspects that a rule does
    /// not have, rules configured where their scope does not run, and options that break their
    /// schema are configuration errors naming the file, line and section. The options checked are
    /// the ones the run uses: the entry's own, else the presets', else `{}`.
    ///
    /// # Errors
    /// Fails on the first such error, or the first error of `presets`.
    pub fn validate(
        &self,
        declaration: &dyn Fn(&PluginName, &PluginSource, &str) -> Option<RuleDeclaration>,
        presets: &dyn Fn(&Settings, &RuleId) -> Result<Option<PresetOptions>>,
        validator: &dyn SchemaValidator,
    ) -> Result<()> {
        self.validate_except(declaration, presets, validator, &BTreeSet::new())
    }

    /// [`Workspace::validate`] without the rules and exceptions of the plugins in `skipped`, whose
    /// declarations are unknown (their component did not load).
    ///
    /// # Errors
    /// As [`Workspace::validate`].
    pub fn validate_except(
        &self,
        declaration: &dyn Fn(&PluginName, &PluginSource, &str) -> Option<RuleDeclaration>,
        presets: &dyn Fn(&Settings, &RuleId) -> Result<Option<PresetOptions>>,
        validator: &dyn SchemaValidator,
        skipped: &BTreeSet<PluginName>,
    ) -> Result<()> {
        let root_is_project = self.root_is_project();
        let repository = self.repository();
        let skip = |rule: &RuleId| {
            rule.plugin() == CORE_PLUGIN
                || skipped.iter().any(|name| name.as_str() == rule.plugin())
        };

        for (rule, entry) in &repository.rules {
            if skip(rule) {
                continue;
            }
            let found = lookup(repository, declaration, rule, entry.origins().next())?;
            if found.scope == RuleScope::Project && !root_is_project {
                return Err(at(
                    entry.origins().next(),
                    format!(
                        "\"{rule}\" is a project-scope rule; the root [rules] holds repository-scope rules only, set it for projects in [[profiles]]"
                    ),
                ));
            }
            let preset = presets(repository, rule)?;
            check_rule(repository, rule, entry, &found, preset.as_ref(), validator)?;
        }

        for (name, settings) in &self.settings {
            for (rule, entry) in &settings.rules {
                if skip(rule) {
                    continue;
                }
                let found = lookup(settings, declaration, rule, entry.origins().next())?;
                if found.scope == RuleScope::Repository {
                    if let Some(origin) = entry.origins().find(|origin| !is_root_rules(origin)) {
                        return Err(at(
                            Some(origin),
                            format!(
                                "\"{rule}\" is a repository-scope rule and runs once for the whole repository, not for project {name}; set it in the root gate.toml's [rules]"
                            ),
                        ));
                    }
                    continue;
                }
                let preset = presets(settings, rule)?;
                check_rule(settings, rule, entry, &found, preset.as_ref(), validator)?;
            }
        }

        for exception in &self.exceptions {
            let scope = exception
                .project
                .as_ref()
                .and_then(|name| self.settings.get(name))
                .unwrap_or(repository);
            let rule = &exception.rule.rule;
            if skipped.iter().any(|name| name.as_str() == rule.plugin()) {
                continue;
            }
            let place = format!(
                "{}:{}: [[exception]] rule \"{}\"",
                exception.file, exception.line, exception.rule
            );
            let Some((name, spec)) = scope.plugins.get_key_value(rule.plugin()) else {
                return Err(Error::config(format!(
                    "{place}: plugin {} is not declared where this file applies; declare it in [plugins] or move the exception",
                    rule.plugin()
                )));
            };
            let Some(found) = declaration(name, &spec.value.source, rule.rule()) else {
                return Err(Error::config(format!(
                    "{place}: plugin {} has no rule {}; run gate list to see the configured rules",
                    rule.plugin(),
                    rule.rule()
                )));
            };
            if let Some(aspect) = &exception.rule.aspect
                && !found.aspects.contains(aspect)
            {
                return Err(Error::config(format!(
                    "{place}: {rule} has no aspect {aspect}; {}",
                    aspects_hint(&found)
                )));
            }
            match found.scope {
                RuleScope::Repository if !exception.is_root() => {
                    return Err(Error::config(format!(
                        "{place}: {rule} is a repository-scope rule; its exceptions belong in the root {}",
                        config::EXCEPTIONS_FILE_NAME
                    )));
                }
                RuleScope::Project if exception.project.is_none() => {
                    return Err(Error::config(format!(
                        "{place}: {rule} is a project-scope rule; its exceptions belong in the {} beside the project's gate.toml",
                        config::EXCEPTIONS_FILE_NAME
                    )));
                }
                RuleScope::Repository | RuleScope::Project => {}
            }
        }

        Ok(())
    }

    fn load_exceptions(&self) -> Result<Vec<ScopedException>> {
        let mut exceptions = Vec::new();
        let workspace = self.root.file().workspace.as_ref();
        let directories: BTreeSet<&str> = self
            .configs()
            .map(|(_, loaded)| loaded.directory())
            .collect();

        for path in
            projects::tracked_named(&self.snapshot, workspace, config::EXCEPTIONS_FILE_NAME)?
        {
            let directory = path.rsplit_once('/').map_or("", |(directory, _)| directory);
            if !directories.contains(directory) {
                return Err(Error::config(format!(
                    "{path} has no gate.toml beside it; exceptions belong to the gate.toml of their directory, add one or move the file"
                )));
            }
        }

        let root_project = self
            .projects
            .iter()
            .find(|project| project.root.is_empty())
            .map(|project| project.name.as_str());

        for (project, loaded) in self.configs() {
            // The root's exceptions belong to the root project when the top is one.
            let project = project.or(root_project);
            let (settings, scope) = match project {
                None => (self.repository(), "the repository".to_owned()),
                Some(name) => (
                    self.settings.get(name).unwrap_or(self.repository()),
                    format!("project {name}"),
                ),
            };
            let required = self.decision_policy(loaded.path());

            for mut exception in loaded.load_exceptions(&self.snapshot)? {
                exception.project = project.map(str::to_owned);
                let plugin = exception.rule.rule.plugin();
                let place = format!(
                    "{}:{}: [[exception]] rule \"{}\"",
                    exception.file, exception.line, exception.rule
                );
                if !settings.plugins.keys().any(|name| name.as_str() == plugin) {
                    return Err(Error::config(format!(
                        "{place}: {}",
                        undeclared(plugin, &scope)
                    )));
                }
                if let Some(policy) = &required
                    && exception.decision.is_none()
                {
                    return Err(Error::config(format!(
                        "{place}: the exception has no decision, and {policy} [decision-policy] requires one; add decision = \"<record>\""
                    )));
                }
                exceptions.push(exception);
            }
        }

        Ok(exceptions)
    }
}

/// Checks that every rule, preset and generator of `settings` names a declared plugin, that every
/// suite names defined steps, and that every step names tools of the root's `[tools]`.
fn check_settings(settings: &Settings, tools: &BTreeMap<Name, Tool>, scope: &str) -> Result<()> {
    let declared = |plugin: &str| settings.plugins.keys().any(|name| name.as_str() == plugin);
    let mut references: Vec<(&SettingOrigin, String, &str)> = Vec::new();

    // A root's `[workspace] extends` entry gets a message of its own, which names both places a
    // preset of an undeclared plugin can go, since its scope is not known yet. A core preset
    // falls through to the message that the core's rules cannot be configured.
    for preset in &settings.extends {
        let plugin = preset.value.plugin();
        if preset.origin.section == WORKSPACE_EXTENDS && plugin != CORE_PLUGIN && !declared(plugin)
        {
            return Err(at(
                Some(&preset.origin),
                format!(
                    "\"{}\": {}",
                    preset.value,
                    undeclared_workspace_preset(plugin)
                ),
            ));
        }
    }

    for (plugin, setting) in &settings.families {
        references.push((&setting.origin, format!("{plugin}/*"), plugin));
    }
    for (rule, entry) in &settings.rules {
        if rule.plugin() == CORE_PLUGIN {
            check_core_rule(rule, entry)?;
            continue;
        }
        if let Some(origin) = entry.origins().next() {
            references.push((origin, rule.to_string(), rule.plugin()));
        }
    }
    for preset in &settings.extends {
        references.push((
            &preset.origin,
            preset.value.to_string(),
            preset.value.plugin(),
        ));
    }
    for (generator, entry) in &settings.generate {
        references.push((&entry.origin, generator.to_string(), generator.plugin()));
    }

    for (origin, key, plugin) in references {
        if !declared(plugin) {
            return Err(at(
                Some(origin),
                format!("\"{key}\": {}", undeclared(plugin, scope)),
            ));
        }
    }

    for (name, suite) in &settings.suites {
        for step in &suite.value.steps {
            if !settings.steps.contains_key(step) {
                return Err(at(
                    Some(&suite.origin),
                    format!(
                        "suite {name}: step {step} is not defined for {scope}; add it to [steps]"
                    ),
                ));
            }
        }
    }

    for (name, step) in &settings.steps {
        if let Some(tool) = step
            .value
            .tools
            .iter()
            .find(|tool| !tools.contains_key(*tool))
        {
            return Err(at(
                Some(&step.origin),
                format!(
                    "step {name}: tool {tool} is not declared; add [tools.{tool}] to the root gate.toml or remove it from tools"
                ),
            ));
        }
    }

    Ok(())
}

/// A core rule's entry: a core rule, in the root `[rules]`, with only `norm` and `norms`.
fn check_core_rule(rule: &RuleId, entry: &RuleSettings) -> Result<()> {
    let origin = entry.origins().next();
    if !CORE_RULES.contains(&rule.rule()) {
        return Err(at(
            origin,
            format!(
                "\"{rule}\" is not a core rule; the core's own rules are {}",
                CORE_RULES
                    .iter()
                    .map(|name| format!("{CORE_PLUGIN}/{name}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    if let Some(level) = entry
        .level
        .as_ref()
        .or_else(|| entry.levels.values().next())
    {
        return Err(at(
            Some(&level.origin),
            format!(
                "\"{rule}\": the core's own rules keep the gate honest, so they cannot be turned off or given another level; set only norm and norms"
            ),
        ));
    }
    if let Some(options) = &entry.options {
        return Err(at(
            Some(&options.origin),
            format!("\"{rule}\": the core's own rules take no options; set only norm and norms"),
        ));
    }
    if let Some(origin) = entry.origins().find(|origin| !is_root_rules(origin)) {
        return Err(at(
            Some(origin),
            format!(
                "\"{rule}\": the core's own rules report for the repository; set their norm and norms in the root gate.toml's [rules]"
            ),
        ));
    }
    Ok(())
}

fn lookup(
    settings: &Settings,
    declaration: &dyn Fn(&PluginName, &PluginSource, &str) -> Option<RuleDeclaration>,
    rule: &RuleId,
    origin: Option<&SettingOrigin>,
) -> Result<RuleDeclaration> {
    let found = settings.plugins.get_key_value(rule.plugin()).and_then(
        |(name, spec): (&PluginName, &Sourced<config::PluginSettings>)| {
            declaration(name, &spec.value.source, rule.rule())
        },
    );
    found.ok_or_else(|| {
        at(
            origin,
            format!(
                "\"{rule}\": plugin {} has no rule {}; run gate list to see the configured rules",
                rule.plugin(),
                rule.rule()
            ),
        )
    })
}

/// Checks one `[rules]` entry against what its plugin declares: its aspects, and the options the
/// run uses, which are the entry's own, else `preset` (what the scope's presets give the rule),
/// else `{}`. Options a preset gives that break the schema are the plugin's error, named by the
/// preset and the `extends` entry, not the entry's.
fn check_rule(
    settings: &Settings,
    rule: &RuleId,
    entry: &RuleSettings,
    found: &RuleDeclaration,
    preset: Option<&PresetOptions>,
    validator: &dyn SchemaValidator,
) -> Result<()> {
    for (aspect, origin) in entry
        .levels
        .iter()
        .map(|(aspect, level)| (aspect, &level.origin))
        .chain(
            entry
                .norms
                .iter()
                .map(|(aspect, norm)| (aspect, &norm.origin)),
        )
    {
        if !found.aspects.contains(aspect) {
            return Err(at(
                Some(origin),
                format!("\"{rule}\" has no aspect {aspect}; {}", aspects_hint(found)),
            ));
        }
    }

    // A rule off in every aspect needs no options, so a rule a preset enables can always be
    // turned off; options it does carry are still checked. Off is the level the run resolves (the
    // rule's, else its family's, as laid over every layer) with no aspect turned on.
    let off = settings.setting(rule, "") == Some(Setting::Off)
        && !entry
            .levels
            .values()
            .any(|level| matches!(level.value, Setting::On(_)));
    if off && entry.options.is_none() {
        return Ok(());
    }

    let empty = serde_json::Value::Object(serde_json::Map::new());
    let (options, given) = match (&entry.options, preset) {
        (Some(own), _) => (&own.value.value, None),
        (None, Some(preset)) => (&preset.options, Some(&preset.preset)),
        (None, None) => (&empty, None),
    };
    let violations = validator.validate(&found.options_schema, options);
    if violations.is_empty() {
        return Ok(());
    }
    let violations = violations.join("; ");
    if let Some(preset) = given {
        return Err(Error::environment(format!(
            "{}: \"{rule}\" options the preset {} gives: {violations}; the plugin is broken, report it to its authors",
            preset.origin, preset.value
        )));
    }
    let origin = entry
        .options
        .as_ref()
        .map_or_else(|| entry.origins().next(), |own| Some(&own.origin));
    Err(at(origin, format!("\"{rule}\" options: {violations}")))
}

/// What to say of the first property the plugin's options schema requires and `options` lacks:
/// where to set it and, when its schema lists the values it takes, those values.
fn missing_required(
    name: &PluginName,
    file: &str,
    schema: &serde_json::Value,
    options: &serde_json::Value,
) -> Option<String> {
    let key = schema
        .get("required")?
        .as_array()?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .find(|key| options.get(key).is_none())?;
    let values = schema
        .get("properties")
        .and_then(|properties| properties.get(key))
        .map(|property| allowed_values(schema, property))
        .unwrap_or_default();
    let choices = if values.is_empty() {
        String::new()
    } else {
        format!(", one of {}", values.join(", "))
    };
    Some(format!(
        "{key} is required; set it in the [plugins.{name}] options in {file}{choices}"
    ))
}

/// The values a property's schema allows, through a local `$ref`: its `enum`, or the one value of
/// each branch of its `oneOf` or `anyOf`; empty when the schema lists none.
fn allowed_values(root: &serde_json::Value, property: &serde_json::Value) -> Vec<String> {
    let resolved = property
        .get("$ref")
        .and_then(serde_json::Value::as_str)
        .and_then(|reference| reference.strip_prefix('#'))
        .and_then(|pointer| root.pointer(pointer))
        .unwrap_or(property);
    if let Some(values) = resolved.get("enum").and_then(serde_json::Value::as_array) {
        return values.iter().map(shown_value).collect();
    }
    for keyword in ["oneOf", "anyOf"] {
        let Some(branches) = resolved.get(keyword).and_then(serde_json::Value::as_array) else {
            continue;
        };
        let values: Option<Vec<String>> = branches.iter().map(single_value).collect();
        if let Some(values) = values {
            return values;
        }
    }
    Vec::new()
}

/// The one value a schema branch allows: its `const`, or an `enum` of one value.
fn single_value(branch: &serde_json::Value) -> Option<String> {
    if let Some(value) = branch.get("const") {
        return Some(shown_value(value));
    }
    let [value] = branch.get("enum")?.as_array()?.as_slice() else {
        return None;
    };
    Some(shown_value(value))
}

/// A JSON value as a person writes it in gate.toml: a string without its quotes.
fn shown_value(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

fn aspects_hint(found: &RuleDeclaration) -> String {
    if found.aspects.is_empty() {
        "the rule has no aspects; set level and norm on the rule".to_owned()
    } else {
        format!("its aspects are {}", found.aspects.join(", "))
    }
}

/// Whether a setting was written in the root `gate.toml`'s own `[rules]`.
fn is_root_rules(origin: &SettingOrigin) -> bool {
    origin.file == FILE_NAME && origin.section == "[rules]"
}

fn at(origin: Option<&SettingOrigin>, message: String) -> Error {
    match origin {
        Some(origin) => Error::config(format!("{origin}: {message}")),
        None => Error::config(message),
    }
}

/// A root that is not a project holds no project section; its presets go in `[workspace] extends`.
fn check_workspace_root(root: &Config) -> Result<()> {
    let extends = root
        .file()
        .project
        .as_ref()
        .is_some_and(|project| !project.extends.is_empty());
    for (present, section) in [
        (root.file().project.is_some(), "[project]"),
        (!root.file().languages.is_empty(), "[languages]"),
    ] {
        if present {
            let presets = if extends {
                "; the root's presets go in [workspace] extends"
            } else {
                ""
            };
            return Err(Error::config(format!(
                "gate.toml: {section} belongs in a project's gate.toml or a [[profiles]] entry; the root is a project only without [workspace], or when [workspace] projects lists \".\"{presets}"
            )));
        }
    }
    Ok(())
}

/// A root that is a project writes its presets in `[project] extends`, which applies them to the
/// repository's rules too; `[workspace] extends` is for a root that is not a project.
fn check_root_project(root: &Config) -> Result<()> {
    let first = root
        .file()
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.extends.first());
    if let Some(preset) = first {
        return Err(root.error(
            Some(preset.span().start),
            "[workspace] extends: the root is a project, so its presets go in [project] extends, which applies them to the repository's rules too",
        ));
    }
    Ok(())
}

/// Paths by project name.
type PathsByProject = BTreeMap<String, BTreeSet<String>>;

/// The settings of each project (the root's plugins, matching profiles, its own file) and the files
/// each project's configuration was read from.
fn merge_settings(
    root: &Config,
    projects: &Projects,
    configs: &BTreeMap<String, Config>,
    mut asked_by_directory: BTreeMap<String, BTreeSet<String>>,
) -> Result<(BTreeMap<String, Settings>, PathsByProject)> {
    let mut settings = BTreeMap::new();
    let mut inputs = BTreeMap::new();
    let mut matched = vec![false; root.profiles().len()];

    for project in projects.iter() {
        let mut merged = Settings {
            plugins: root.own().plugins.clone(),
            ..Settings::default()
        };
        for (index, profile) in root.profiles().iter().enumerate() {
            if profile.matches(&project.root) {
                matched[index] = true;
                merged.overlay(&profile.settings);
            }
        }
        let own = if project.root.is_empty() {
            Some(root)
        } else {
            configs.get(&project.name)
        };
        if let Some(own) = own {
            merged.overlay(own.own());
        }
        check_settings(
            &merged,
            &root.file().tools,
            &format!("project {}", project.name),
        )?;
        let mut own_inputs = asked_by_directory.remove(&project.root).unwrap_or_default();
        for plugin in merged.plugins.values() {
            own_inputs.extend(
                plugin
                    .value
                    .reads
                    .iter()
                    .map(|read| read.as_str().to_owned()),
            );
        }
        inputs.insert(project.name.clone(), own_inputs);
        settings.insert(project.name.clone(), merged);
    }

    if let Some((profile, _)) = root
        .profiles()
        .iter()
        .zip(&matched)
        .find(|(_, matched)| !**matched)
    {
        return Err(Error::config(format!(
            "gate.toml:{}: [[profiles]] projects = {:?} matches no project; fix the globs or remove the profile",
            profile.line, profile.projects
        )));
    }

    Ok((settings, inputs))
}

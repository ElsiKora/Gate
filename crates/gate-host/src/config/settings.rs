//! Settings as they apply to one project, or to the repository: merged from the root's `[plugins]`,
//! the matching `[[profiles]]` and the project's own `gate.toml`, each setting with the file, line
//! and section it came from.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

use super::file::{Setting, Step, Suite};
use super::names::{Name, PluginItem, PluginName, PluginSource, RepositoryPath, RuleId};

/// Where a setting was written: a file, a line and a section.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SettingOrigin {
    /// The file, relative to the repository root.
    pub file: String,
    /// The line of the entry.
    pub line: usize,
    /// The section: `[rules]`, `[project] extends`, `[[profiles]] rules`, ...
    pub section: String,
}

impl fmt::Display for SettingOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{} {}", self.file, self.line, self.section)
    }
}

/// A value with the place it was written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sourced<T> {
    /// The value.
    pub value: T,
    /// Where it was written.
    pub origin: SettingOrigin,
}

/// A level with the place it was written and the decision the entry that wrote it names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LevelSetting {
    /// The level.
    pub value: Setting,
    /// Where it was written.
    pub origin: SettingOrigin,
    /// The decision record the entry names; it travels with the level when files layer.
    pub decision: Option<String>,
}

/// Options of a rule or a generator, inline or read from a file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Options {
    /// The options.
    pub value: serde_json::Value,
    /// The options file they were read from, relative to the repository root; `None` when inline.
    pub file: Option<String>,
}

/// A declared plugin: its source, the options its rules share, and extra read grants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginSettings {
    /// Where the plugin is fetched from.
    pub source: PluginSource,
    /// The options shared by all of the plugin's rules, inline or read from their file.
    pub options: Option<Options>,
    /// Paths outside the project the plugin may read, relative to the repository root.
    pub reads: Vec<RepositoryPath>,
}

/// What configuration says about one rule, field by field.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuleSettings {
    /// The level of the aspects `levels` does not name.
    pub level: Option<LevelSetting>,
    /// The norm of the aspects `norms` does not name.
    pub norm: Option<Sourced<String>>,
    /// Levels of single aspects.
    pub levels: BTreeMap<String, LevelSetting>,
    /// Norms of single aspects.
    pub norms: BTreeMap<String, Sourced<String>>,
    /// The options.
    pub options: Option<Sourced<Options>>,
}

impl RuleSettings {
    /// Every place a field of this rule was written.
    pub fn origins(&self) -> impl Iterator<Item = &SettingOrigin> {
        self.level
            .iter()
            .map(|level| &level.origin)
            .chain(self.norm.iter().map(|norm| &norm.origin))
            .chain(self.levels.values().map(|level| &level.origin))
            .chain(self.norms.values().map(|norm| &norm.origin))
            .chain(self.options.iter().map(|options| &options.origin))
    }

    fn overlay(&mut self, layer: &Self) {
        if layer.level.is_some() {
            self.level.clone_from(&layer.level);
        }
        if layer.norm.is_some() {
            self.norm.clone_from(&layer.norm);
        }
        if layer.options.is_some() {
            self.options.clone_from(&layer.options);
        }
        for (aspect, level) in &layer.levels {
            self.levels.insert(aspect.clone(), level.clone());
        }
        for (aspect, norm) in &layer.norms {
            self.norms.insert(aspect.clone(), norm.clone());
        }
    }
}

/// The settings of one project (or of the repository): plugins, presets, rules, generators, steps
/// and suites.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Settings {
    /// The plugins, by the name rule ids use.
    pub plugins: BTreeMap<PluginName, Sourced<PluginSettings>>,
    /// Language settings, by language id: opaque JSON objects for the plugins that declare the
    /// language.
    pub languages: BTreeMap<String, Sourced<serde_json::Value>>,
    /// The presets, in order, without repeats.
    pub extends: Vec<Sourced<PluginItem>>,
    /// Family wildcards: `"<plugin>/*"` levels, by plugin.
    pub families: BTreeMap<String, LevelSetting>,
    /// Rules.
    pub rules: BTreeMap<RuleId, RuleSettings>,
    /// Generators and their options.
    pub generate: BTreeMap<PluginItem, Sourced<Option<Options>>>,
    /// Steps.
    pub steps: BTreeMap<Name, Sourced<Step>>,
    /// Suites.
    pub suites: BTreeMap<Name, Sourced<Suite>>,
}

impl Settings {
    /// The level configuration gives a rule's aspect: the aspect's level, else the rule's, else the
    /// rule's family's. `None` when nothing says (the preset's level applies, else `error`);
    /// `aspect` is `""` for a rule without aspects.
    #[must_use]
    pub fn setting(&self, rule: &RuleId, aspect: &str) -> Option<Setting> {
        let entry = self.rules.get(rule);
        entry
            .and_then(|entry| entry.levels.get(aspect))
            .or_else(|| entry.and_then(|entry| entry.level.as_ref()))
            .or_else(|| self.families.get(rule.plugin()))
            .map(|setting| setting.value)
    }

    /// The norm configuration maps a rule's aspect to: the aspect's norm, else the rule's.
    #[must_use]
    pub fn norm(&self, rule: &RuleId, aspect: &str) -> Option<&str> {
        let entry = self.rules.get(rule)?;
        entry
            .norms
            .get(aspect)
            .or(entry.norm.as_ref())
            .map(|norm| norm.value.as_str())
    }

    /// A rule's options, inline or read from its options file.
    #[must_use]
    pub fn options(&self, rule: &RuleId) -> Option<&serde_json::Value> {
        self.rules
            .get(rule)
            .and_then(|entry| entry.options.as_ref())
            .map(|options| &options.value.value)
    }

    /// The options a plugin's rules share, inline or read from their file.
    #[must_use]
    pub fn plugin_options(&self, plugin: &PluginName) -> Option<&serde_json::Value> {
        self.plugins
            .get(plugin)
            .and_then(|entry| entry.value.options.as_ref())
            .map(|options| &options.value)
    }

    /// A language's settings, as configured.
    #[must_use]
    pub fn language(&self, id: &str) -> Option<&serde_json::Value> {
        self.languages.get(id).map(|entry| &entry.value)
    }

    /// A generator's options, inline or read from its options file.
    #[must_use]
    pub fn generator_options(&self, generator: &PluginItem) -> Option<&serde_json::Value> {
        self.generate
            .get(generator)
            .and_then(|entry| entry.value.as_ref())
            .map(|options| &options.value)
    }

    /// Whether nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Lays `layer` over these settings: a key the layer sets (a plugin, a language, a
    /// generator, a step, a suite) replaces the same key here; a rule's
    /// fields are replaced one by one; presets are appended; a family level (`"<plugin>/*"`)
    /// drops the levels earlier layers gave that plugin's rules.
    pub(crate) fn overlay(&mut self, layer: &Self) {
        for (name, plugin) in &layer.plugins {
            self.plugins.insert(name.clone(), plugin.clone());
        }
        for (id, language) in &layer.languages {
            self.languages.insert(id.clone(), language.clone());
        }
        for preset in &layer.extends {
            if !self
                .extends
                .iter()
                .any(|present| present.value == preset.value)
            {
                self.extends.push(preset.clone());
            }
        }
        for (plugin, setting) in &layer.families {
            self.families.insert(plugin.clone(), setting.clone());
            // A later layer's "<plugin>/*" overrides the levels earlier layers gave its rules; the
            // layer's own rule levels, laid next, stay more specific than its family.
            for (rule, settings) in &mut self.rules {
                if rule.plugin() == plugin.as_str() {
                    settings.level = None;
                    settings.levels.clear();
                }
            }
        }
        for (rule, settings) in &layer.rules {
            self.rules
                .entry(rule.clone())
                .or_default()
                .overlay(settings);
        }
        for (generator, options) in &layer.generate {
            self.generate.insert(generator.clone(), options.clone());
        }
        for (name, step) in &layer.steps {
            self.steps.insert(name.clone(), step.clone());
        }
        for (name, suite) in &layer.suites {
            self.suites.insert(name.clone(), suite.clone());
        }
    }
}

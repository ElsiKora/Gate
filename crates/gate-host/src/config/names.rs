//! The validated strings of `gate.toml`: names, rule ids, plugin sources, paths.
//!
//! Each type checks itself while the file is read, so a malformed value fails with the file, line
//! and column of the value.

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::Deserialize;

/// Whether `value` is lowercase kebab case: `[a-z0-9]+(-[a-z0-9]+)*`.
#[must_use]
pub fn is_kebab(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

const KEBAB: &str = "^[a-z0-9]+(-[a-z0-9]+)*$";

macro_rules! string_schema {
    ($type:ty, $name:literal, $pattern:expr, $description:literal) => {
        impl JsonSchema for $type {
            fn schema_name() -> Cow<'static, str> {
                Cow::Borrowed($name)
            }

            fn inline_schema() -> bool {
                true
            }

            fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
                json_schema!({
                    "type": "string",
                    "pattern": $pattern,
                    "description": $description,
                })
            }
        }
    };
}

/// A lowercase kebab name: a step, a suite, a tool, a plugin.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(try_from = "String")]
pub struct Name(String);

impl Name {
    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Name {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if is_kebab(&value) {
            Ok(Self(value))
        } else {
            Err(format!(
                "{value:?} is not a valid name; use lowercase letters, digits and single dashes"
            ))
        }
    }
}

impl fmt::Display for Name {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

string_schema!(
    Name,
    "Name",
    KEBAB,
    "Lowercase letters, digits and single dashes."
);

/// The id of a job of the generated workflow: a [`Name`] that starts with a letter, since GitHub
/// rejects a workflow whose job id starts with a digit.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(try_from = "String")]
pub struct JobId(String);

impl JobId {
    /// The id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for JobId {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if is_kebab(&value) && value.starts_with(|first: char| first.is_ascii_lowercase()) {
            Ok(Self(value))
        } else {
            Err(format!(
                "{value:?} is not a valid job id; start with a lowercase letter, then use lowercase letters, digits and single dashes (GitHub refuses a job id that starts with a digit)"
            ))
        }
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

string_schema!(
    JobId,
    "JobId",
    "^[a-z][a-z0-9]*(-[a-z0-9]+)*$",
    "A lowercase letter, then lowercase letters, digits and single dashes."
);

/// The name of a plugin as `[plugins]` declares it; the first part of every rule id.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(try_from = "String")]
pub struct PluginName(String);

/// The plugin name the core keeps for its own pseudo-rules (`gate/exceptions`, `gate/sync`).
pub const CORE_PLUGIN: &str = "gate";

/// The core's own rules, without the `gate/` prefix: they always run and always report at their
/// own level; configuration may give them only a `norm` and `norms`.
pub const CORE_RULES: &[&str] = &["baseline", "budget", "exceptions", "sync"];

impl PluginName {
    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for PluginName {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if !is_kebab(&value) {
            return Err(format!(
                "{value:?} is not a valid plugin name; use lowercase letters, digits and single dashes"
            ));
        }

        if value == CORE_PLUGIN {
            return Err(format!(
                "\"{CORE_PLUGIN}\" is the core's own name; give the plugin another name"
            ));
        }

        Ok(Self(value))
    }
}

impl std::borrow::Borrow<str> for PluginName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PluginName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

string_schema!(
    PluginName,
    "PluginName",
    KEBAB,
    "The name rules of this plugin are prefixed with."
);

/// A rule id: `<plugin>/<rule>`, both lowercase kebab.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuleId {
    plugin: String,
    rule: String,
}

impl RuleId {
    /// Parses `<plugin>/<rule>`.
    ///
    /// # Errors
    /// Fails, with a message saying what is expected, when `value` is not a rule id.
    pub fn parse(value: &str) -> Result<Self, String> {
        let Some((plugin, rule)) = value.split_once('/') else {
            return Err(format!(
                "{value:?} is not a rule id; write <plugin>/<rule>, for example \"git/branch-name\""
            ));
        };

        if !is_kebab(plugin) || !is_kebab(rule) {
            return Err(format!(
                "{value:?} is not a rule id; <plugin>/<rule> are lowercase letters, digits and single dashes"
            ));
        }

        Ok(Self {
            plugin: plugin.to_owned(),
            rule: rule.to_owned(),
        })
    }

    /// The plugin part.
    #[must_use]
    pub fn plugin(&self) -> &str {
        &self.plugin
    }

    /// The rule part, without the plugin.
    #[must_use]
    pub fn rule(&self) -> &str {
        &self.rule
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.plugin, self.rule)
    }
}

/// A key of `[rules]`: a rule, or every rule of a plugin. Aspects take their level and norm inside
/// the rule's entry (`levels`, `norms`).
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(try_from = "String")]
pub enum RuleKey {
    /// `"<plugin>/*"`: every rule of the plugin. Takes only a level.
    Family(String),
    /// `"<plugin>/<rule>"`.
    Rule(RuleId),
}

impl RuleKey {
    /// The plugin the key names.
    #[must_use]
    pub fn plugin(&self) -> &str {
        match self {
            Self::Family(plugin) => plugin,
            Self::Rule(rule) => rule.plugin(),
        }
    }

    /// Parses a key.
    ///
    /// # Errors
    /// Fails, with a message saying what is expected, when `value` is not a key.
    pub fn parse(value: &str) -> Result<Self, String> {
        if let Some(plugin) = value.strip_suffix("/*") {
            return if is_kebab(plugin) {
                Ok(Self::Family(plugin.to_owned()))
            } else {
                Err(format!(
                    "{value:?} is not a family wildcard; write <plugin>/*, for example \"layers/*\""
                ))
            };
        }

        if let Some((rule, aspect)) = value.split_once('#') {
            return Err(format!(
                "{value:?}: an aspect is not a key of its own; write \"{rule}\" = {{ levels = {{ {aspect} = \"...\" }}, norms = {{ {aspect} = \"...\" }} }}"
            ));
        }

        RuleId::parse(value).map(Self::Rule)
    }
}

impl TryFrom<String> for RuleKey {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl fmt::Display for RuleKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Family(plugin) => write!(formatter, "{plugin}/*"),
            Self::Rule(rule) => write!(formatter, "{rule}"),
        }
    }
}

/// A rule id with an optional aspect: the `rule` of an exception.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(try_from = "String")]
pub struct RuleRef {
    /// The rule.
    pub rule: RuleId,
    /// The aspect; `None` matches every aspect of the rule.
    pub aspect: Option<String>,
}

impl RuleRef {
    /// Parses `<plugin>/<rule>` or `<plugin>/<rule>#<aspect>`.
    ///
    /// # Errors
    /// Fails, with a message saying what is expected, when `value` is neither.
    pub fn parse(value: &str) -> Result<Self, String> {
        if value.ends_with("/*") {
            return Err(format!(
                "{value:?}: an exception names one rule, not a family; turn the family off in [rules] instead"
            ));
        }

        match value.split_once('#') {
            Some((rule, aspect)) => {
                let rule = RuleId::parse(rule)?;
                if is_kebab(aspect) {
                    Ok(Self {
                        rule,
                        aspect: Some(aspect.to_owned()),
                    })
                } else {
                    Err(format!(
                        "{value:?} has an invalid aspect; write <plugin>/<rule>#<aspect> in lowercase kebab case"
                    ))
                }
            }
            None => RuleId::parse(value).map(|rule| Self { rule, aspect: None }),
        }
    }
}

impl TryFrom<String> for RuleRef {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl fmt::Display for RuleRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.aspect {
            None => write!(formatter, "{}", self.rule),
            Some(aspect) => write!(formatter, "{}#{aspect}", self.rule),
        }
    }
}

const RULE_ID: &str = "^[a-z0-9]+(-[a-z0-9]+)*/[a-z0-9]+(-[a-z0-9]+)*$";
const RULE_KEY: &str = "^[a-z0-9]+(-[a-z0-9]+)*/(\\*|[a-z0-9]+(-[a-z0-9]+)*)$";
const RULE_REF: &str = "^[a-z0-9]+(-[a-z0-9]+)*/[a-z0-9]+(-[a-z0-9]+)*(#[a-z0-9]+(-[a-z0-9]+)*)?$";

string_schema!(
    RuleKey,
    "RuleKey",
    RULE_KEY,
    "<plugin>/<rule>, or <plugin>/* for every rule of a plugin."
);
string_schema!(
    RuleRef,
    "RuleRef",
    RULE_REF,
    "<plugin>/<rule> or <plugin>/<rule>#<aspect>."
);

/// `<plugin>/<name>`: a preset in `extends` or a generator in `[generate]`.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(try_from = "String")]
pub struct PluginItem(RuleId);

impl PluginItem {
    /// The plugin part.
    #[must_use]
    pub fn plugin(&self) -> &str {
        self.0.plugin()
    }

    /// The item's name inside the plugin.
    #[must_use]
    pub fn name(&self) -> &str {
        self.0.rule()
    }
}

impl TryFrom<String> for PluginItem {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        RuleId::parse(&value).map(Self).map_err(|_| {
            format!(
                "{value:?} is not <plugin>/<name>; for example \"layers/recommended\" or \"agents/adapters\""
            )
        })
    }
}

impl fmt::Display for PluginItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

string_schema!(
    PluginItem,
    "PluginItem",
    RULE_ID,
    "<plugin>/<name>: a preset or a generator of a declared plugin."
);

/// Where a plugin is fetched from: `<owner>/<repo>:<plugin>@<version>`, the asset `<plugin>.wasm` of
/// the GitHub release `plugin-<plugin>-v<version>` of that repository.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PluginSource {
    /// The GitHub owner.
    pub owner: String,
    /// The GitHub repository.
    pub repo: String,
    /// The plugin's name in that repository's releases.
    pub plugin: String,
    /// The version, without `v`.
    pub version: String,
}

impl PluginSource {
    /// Parses a source.
    ///
    /// # Errors
    /// Fails, with a message saying what is expected, when `value` is not a plugin source.
    pub fn parse(value: &str) -> Result<Self, String> {
        let expected = || {
            format!(
                "{value:?} is not a plugin source; write <owner>/<repo>:<plugin>@<version>, for example \"ElsiKora/Gate:git@{}\"",
                env!("CARGO_PKG_VERSION")
            )
        };
        let (repository, rest) = value.split_once(':').ok_or_else(expected)?;
        let (owner, repo) = repository.split_once('/').ok_or_else(expected)?;
        let (plugin, version) = rest.split_once('@').ok_or_else(expected)?;
        let segment = |part: &str| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        };

        if !(segment(owner) && segment(repo) && is_kebab(plugin) && is_version(version)) {
            return Err(expected());
        }

        Ok(Self {
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            plugin: plugin.to_owned(),
            version: version.to_owned(),
        })
    }

    /// The tag of the release that carries the plugin: `plugin-<plugin>-v<version>`.
    #[must_use]
    pub fn release_tag(&self) -> String {
        format!("plugin-{}-v{}", self.plugin, self.version)
    }

    /// The file name of the release asset: `<plugin>.wasm`.
    #[must_use]
    pub fn asset(&self) -> String {
        format!("{}.wasm", self.plugin)
    }
}

impl fmt::Display for PluginSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}/{}:{}@{}",
            self.owner, self.repo, self.plugin, self.version
        )
    }
}

impl TryFrom<String> for PluginSource {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl<'de> Deserialize<'de> for PluginSource {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

string_schema!(
    PluginSource,
    "PluginSource",
    "^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+:[a-z0-9]+(-[a-z0-9]+)*@[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.-]+)?$",
    "<owner>/<repo>:<plugin>@<version>: the asset <plugin>.wasm of the release plugin-<plugin>-v<version>."
);

/// Whether `value` is `<major>.<minor>.<patch>` with an optional `-<pre-release>`.
#[must_use]
pub fn is_version(value: &str) -> bool {
    let (core, pre) = match value.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (value, None),
    };
    let numbers: Vec<&str> = core.split('.').collect();
    numbers.len() == 3
        && numbers
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && pre.is_none_or(|pre| {
            !pre.is_empty()
                && pre
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
        })
}

/// A version `<major>.<minor>.<patch>[-<pre-release>]`.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(try_from = "String")]
pub struct Version(String);

impl Version {
    /// The version.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Version {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if is_version(&value) {
            Ok(Self(value))
        } else {
            Err(format!(
                "{value:?} is not a version; write <major>.<minor>.<patch>, for example \"0.1.0\""
            ))
        }
    }
}

string_schema!(
    Version,
    "Version",
    "^[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.-]+)?$",
    "<major>.<minor>.<patch>[-<pre-release>]"
);

/// A path relative to the directory of the `gate.toml` that names it: `/` separators, no `..`, not
/// absolute.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(try_from = "String")]
pub struct RelativePath(String);

impl RelativePath {
    /// The path as written, without a leading `./`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RelativePath {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        check_relative(&value).map(Self)
    }
}

/// Checks that `value` is a relative path that stays inside its directory; returns it without a
/// leading `./` or trailing `/`.
///
/// # Errors
/// Fails, with a message saying what is expected, when it is not.
pub fn check_relative(value: &str) -> Result<String, String> {
    normalize(value).ok_or_else(|| {
        format!(
            "{value:?} is not a relative path inside the project; use / separators, no .., no leading /"
        )
    })
}

/// `value` without a leading `./` or trailing `/`; `None` when it is empty, absolute, uses backslashes, or
/// has an empty, `.` or `..` part.
fn normalize(value: &str) -> Option<String> {
    let trimmed = value
        .strip_prefix("./")
        .unwrap_or(value)
        .trim_end_matches('/');

    if trimmed.is_empty()
        || trimmed.starts_with('/')
        || trimmed.contains('\\')
        || trimmed
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }

    Some(trimmed.to_owned())
}

/// A path relative to the repository root, wherever the `gate.toml` that names it is: `/`
/// separators, no `..`, not absolute. A plugin's `reads` are these.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(try_from = "String")]
pub struct RepositoryPath(String);

impl RepositoryPath {
    /// The path as written, without a leading `./` or trailing `/`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RepositoryPath {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        normalize(&value).map(Self).ok_or_else(|| {
            format!(
                "{value:?} is not a path relative to the repository root; write it from the top of the repository with / separators, no .., no leading /"
            )
        })
    }
}

string_schema!(
    RepositoryPath,
    "RepositoryPath",
    "^(?!/)(?!.*\\.\\.)[^\\\\]+$",
    "A path relative to the repository root, whichever gate.toml names it, with / separators."
);

string_schema!(
    RelativePath,
    "RelativePath",
    "^(?!/)(?!.*\\.\\.)[^\\\\]+$",
    "A path relative to the directory of this gate.toml, with / separators."
);

/// A git hook gate can install: `pre-commit`, `commit-msg`, `pre-push`, ...
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(try_from = "String")]
pub struct HookName(String);

/// The client-side git hooks a `[hooks]` table may name.
pub const HOOKS: &[&str] = &[
    "applypatch-msg",
    "commit-msg",
    "post-checkout",
    "post-commit",
    "post-merge",
    "post-rewrite",
    "pre-applypatch",
    "pre-commit",
    "pre-merge-commit",
    "pre-push",
    "pre-rebase",
    "prepare-commit-msg",
];

impl HookName {
    /// The hook's name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for HookName {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if HOOKS.contains(&value.as_str()) {
            Ok(Self(value))
        } else {
            Err(format!(
                "{value:?} is not a git hook; expected one of {}",
                HOOKS.join(", ")
            ))
        }
    }
}

impl JsonSchema for HookName {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("HookName")
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string", "enum": HOOKS })
    }
}

/// A project name: letters, digits and `._@/-`, as package names are.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(try_from = "String")]
pub struct ProjectName(String);

impl ProjectName {
    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `value` is a valid project name.
    #[must_use]
    pub fn is_valid(value: &str) -> bool {
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._@/-".contains(&byte))
    }
}

impl TryFrom<String> for ProjectName {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if Self::is_valid(&value) {
            Ok(Self(value))
        } else {
            Err(format!(
                "{value:?} is not a valid project name; use letters, digits and . _ @ / -"
            ))
        }
    }
}

string_schema!(
    ProjectName,
    "ProjectName",
    "^[A-Za-z0-9._@/-]+$",
    "The project's name in reports and in --project."
);

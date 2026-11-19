//! The component side of `wit/gate.wit`: the generated bindings, the [`Host`] over the imports, and
//! the conversions [`crate::plugin!`] calls from the exports. Not part of the public API.

use crate::git::{Commit, CommitRange, SyncState};
use crate::host::{GitHost, Host, TreeHost};
use crate::plugin::{self, PluginSpec};
use crate::report::{FileLocation, Location, Span};
use crate::rule::{GeneratorInfo, RuleInfo, Scope};

wit_bindgen::generate!({
    world: "plugin",
    path: "../../wit",
    pub_export_macro: true,
    export_macro_name: "__export_plugin",
    default_bindings_module: "gate_sdk::bindings",
});

use self::gate::plugin::{git as wit_git, tree as wit_tree, types as wit};

/// The host as a component sees it: every call is an import.
#[derive(Debug, Default)]
pub struct ComponentHost;

impl TreeHost for ComponentHost {
    fn list(&self, prefix: &str) -> Vec<String> {
        wit_tree::paths(prefix)
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        wit_tree::read(path)
    }

    fn changed(&self) -> Option<Vec<String>> {
        wit_tree::changed()
    }
}

impl GitHost for ComponentHost {
    fn head(&self) -> Option<String> {
        wit_git::head()
    }

    fn branch(&self) -> Option<String> {
        wit_git::branch()
    }

    fn range(&self) -> Option<CommitRange> {
        wit_git::range().map(|range| CommitRange {
            base: range.base,
            head: range.head,
            target_ref: range.target_ref,
        })
    }

    fn resolve(&self, rev: &str) -> Option<String> {
        wit_git::resolve(rev)
    }

    fn tree(&self, commit: &str) -> String {
        wit_git::tree(commit)
    }

    fn merge_tree(&self, first: &str, second: &str) -> Option<String> {
        wit_git::merge_tree(first, second)
    }

    fn log(&self, range: &str) -> Vec<Commit> {
        wit_git::log(range)
            .into_iter()
            .map(|commit| Commit {
                id: commit.id,
                parents: commit.parents,
                message: commit.message,
                author_date: commit.author_date,
            })
            .collect()
    }

    fn changed_paths(&self, parent: &str, commit: &str) -> Vec<String> {
        wit_git::changed_paths(parent, commit)
    }

    fn read_at(&self, commit: &str, path: &str) -> Option<Vec<u8>> {
        wit_git::read_at(commit, path)
    }

    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        wit_git::is_ancestor(ancestor, descendant)
    }

    fn sync_state(&self, commit: &str) -> SyncState {
        let state = wit_git::sync_state(commit);
        SyncState {
            produced: state.produced,
            drift: state.drift,
        }
    }

    fn message(&self) -> Option<String> {
        wit_git::message()
    }

    fn hook(&self) -> bool {
        wit_git::hook()
    }
}

impl Host for ComponentHost {
    fn git(&self) -> Option<&dyn GitHost> {
        Some(self)
    }

    fn log(&self, message: &str) {
        log(message);
    }
}

/// The JSON Schema of `T`, for [`crate::plugin!`].
#[must_use]
pub fn schema<T: schemars::JsonSchema>() -> String {
    crate::rule::schema::<T>()
}

/// The `describe` export.
#[must_use]
pub fn describe(spec: &PluginSpec) -> wit::PluginInfo {
    let info = plugin::describe(spec);
    wit::PluginInfo {
        name: info.name,
        version: info.version,
        options_schema: info.options_schema,
        rules: info.rules.into_iter().map(rule_info).collect(),
        generators: info.generators.into_iter().map(generator_info).collect(),
        presets: info
            .presets
            .into_iter()
            .map(|preset| wit::Preset {
                name: preset.name,
                rules: preset.rules,
            })
            .collect(),
    }
}

/// The `check` export.
///
/// # Errors
/// The SDK's error message, which the host reports with the plugin's name.
pub fn check(spec: &PluginSpec, request: wit::CheckRequest) -> Result<Vec<wit::Finding>, String> {
    let request = plugin::CheckRequest {
        project: request.project,
        rules: request
            .rules
            .into_iter()
            .map(|rule| plugin::RuleRun {
                name: rule.name,
                options: rule.options,
                options_file: rule.options_file,
                exceptions: rule.exceptions,
            })
            .collect(),
        plugin_options: request.plugin_options,
        languages: request.languages,
        norms: request.norms,
    };
    plugin::check(spec, &ComponentHost, &request)
        .map(|findings| findings.into_iter().map(finding).collect())
        .map_err(|error| error.message().to_owned())
}

/// The `generate` export.
///
/// # Errors
/// The SDK's error message, which the host reports with the plugin's name.
pub fn generate(
    spec: &PluginSpec,
    name: &str,
    project: &str,
    options: &str,
) -> Result<wit::GenerateResult, String> {
    plugin::generate(spec, &ComponentHost, name, project, options)
        .map(|output| wit::GenerateResult {
            files: output
                .files
                .into_iter()
                .map(|file| wit::GeneratedFile {
                    path: file.path,
                    contents: file.contents,
                })
                .collect(),
            findings: output.findings.into_iter().map(finding).collect(),
        })
        .map_err(|error| error.message().to_owned())
}

fn rule_info(info: RuleInfo) -> wit::RuleInfo {
    wit::RuleInfo {
        name: info.name,
        title: info.title,
        aspects: info.aspects,
        scope: match info.scope {
            Scope::Project => wit::Scope::Project,
            Scope::Repository => wit::Scope::Repository,
        },
        reads_history: info.reads_history,
        languages: info.languages,
        options_schema: info.options_schema,
    }
}

fn generator_info(info: GeneratorInfo) -> wit::GeneratorInfo {
    wit::GeneratorInfo {
        name: info.name,
        title: info.title,
        options_schema: info.options_schema,
        owns: info.owns,
    }
}

fn finding(finding: crate::Finding) -> wit::Finding {
    wit::Finding {
        rule: finding.rule,
        aspect: finding.aspect,
        message: finding.message,
        location: finding.location.map(location),
        target: finding.target,
        help: finding.help,
        key: finding.key,
    }
}

fn location(location: Location) -> wit::Location {
    let FileLocation { path, span } = match location {
        Location::File(file) => file,
        Location::Commit(id) => return wit::Location::Commit(id),
    };
    wit::Location::File(wit::FileLocation {
        path,
        span: span.map(
            |Span {
                 line,
                 column,
                 end_line,
                 end_column,
             }| wit::Span {
                line,
                column,
                end_line,
                end_column,
            },
        ),
    })
}

/// Declares the plugin: its name, version, options, rules, generators and presets. Defines
/// `pub static PLUGIN: gate_sdk::PluginSpec` and, when built for `wasm32`, the component's exports.
///
/// `options` is the type of the `[plugins.<name>] options` the plugin's rules share
/// (`Deserialize + JsonSchema`); `describe` answers its schema and the host validates the
/// configuration against it when the plugin loads. A plugin without options of its own names
/// [`crate::NoOptions`].
///
/// ```ignore
/// gate_sdk::plugin! {
///     name: "hello",
///     version: env!("CARGO_PKG_VERSION"),
///     options: gate_sdk::NoOptions,
///     rules: [NoTodo],
///     generators: [],
///     presets: [],
/// }
/// ```
#[macro_export]
macro_rules! plugin {
    (
        name: $name:expr,
        version: $version:expr,
        options: $options:ty,
        rules: [$($rule:ty),* $(,)?],
        generators: [$($generator:ty),* $(,)?],
        presets: [$($preset:expr),* $(,)?] $(,)?
    ) => {
        /// The plugin's declaration.
        pub static PLUGIN: $crate::PluginSpec = $crate::PluginSpec {
            name: $name,
            version: $version,
            options_schema: $crate::bindings::schema::<$options>,
            rules: &[$($crate::RuleEntry::of::<$rule>()),*],
            generators: &[$($crate::GeneratorEntry::of::<$generator>()),*],
            presets: &[$($preset),*],
        };

        #[cfg(target_arch = "wasm32")]
        const _: () = {
            struct Component;

            impl $crate::bindings::Guest for Component {
                fn describe() -> $crate::bindings::gate::plugin::types::PluginInfo {
                    $crate::bindings::describe(&PLUGIN)
                }

                fn check(
                    request: $crate::bindings::gate::plugin::types::CheckRequest,
                ) -> ::core::result::Result<
                    ::std::vec::Vec<$crate::bindings::gate::plugin::types::Finding>,
                    ::std::string::String,
                > {
                    $crate::bindings::check(&PLUGIN, request)
                }

                fn generate(
                    name: ::std::string::String,
                    project: ::std::string::String,
                    options: ::std::string::String,
                ) -> ::core::result::Result<
                    $crate::bindings::gate::plugin::types::GenerateResult,
                    ::std::string::String,
                > {
                    $crate::bindings::generate(&PLUGIN, &name, &project, &options)
                }
            }

            $crate::bindings::__export_plugin!(Component with_types_in $crate::bindings);
        };
    };
}

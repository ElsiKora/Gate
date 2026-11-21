//! What each plugin declares when it describes itself, stored in the cache by the plugin's
//! sha256 (architecture §9). Every run checks the whole configuration against what every declared
//! plugin declares, and a run loads only the plugins it runs: the others answer from here.
//!
//! An entry is written whenever a plugin loads, and read only by a core of the same version, for
//! the same plugin interface and entry format. An entry that is missing, cannot be read or does
//! not parse is not an error: the run loads the plugin to get its declaration again.

use std::io::Write as _;

use gate_sdk::{GeneratorInfo, PluginInfo, PresetInfo, RuleInfo, Scope};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::cache::{Cache, Sha256Sum};

/// Raised when an entry written before would be read differently.
const FORMAT: u32 = 1;

/// The name of this core's entry in a plugin's directory, so no other core reads it.
fn key() -> String {
    key_for(FORMAT, gate_runtime::INTERFACE, env!("CARGO_PKG_VERSION"))
}

/// The name of the entry of a core with this entry format, plugin interface and version: the
/// digest of the three.
fn key_for(format: u32, interface: &str, version: &str) -> String {
    let digest = Sha256::digest(format!("{format}\0{interface}\0{version}\0"));
    Sha256Sum::from_digest(&digest).as_str().to_owned()
}

/// What the plugin with this sha256 declared when it last loaded under this core; `None` when no
/// entry is stored or it cannot be read.
pub(super) fn read(cache: &Cache, plugin: &Sha256Sum) -> Option<PluginInfo> {
    let path = cache.declaration(plugin, &key());
    let bytes = std::fs::read(&path).ok()?;
    let stored: Stored = serde_json::from_slice(&bytes).ok()?;
    // Used now: `gate cache prune --older-than` keeps it.
    crate::cache::touch(&path);
    Some(stored.into())
}

/// Whether the cache holds a declaration of `plugin` under this core and plugin interface that
/// can be read: the plugin has loaded under this core, so its module is compiled. An entry that
/// does not parse is not one; the next load writes it again.
pub(crate) fn stored(cache: &Cache, plugin: &Sha256Sum) -> bool {
    read(cache, plugin).is_some()
}

/// Stores what the plugin with this sha256 declares, in place of any entry there: one that does
/// not parse, or names another plugin, is replaced by the run that loaded the plugin because of
/// it. A cache that cannot be written costs the next run a load of the plugin, not a failure, so
/// nothing is reported.
pub(super) fn write(cache: &Cache, plugin: &Sha256Sum, info: &PluginInfo) {
    let key = key();
    let Ok(bytes) = serde_json::to_vec(&Stored::from(info)) else {
        return;
    };
    let Ok(mut temporary) = cache.temporary() else {
        return;
    };
    if temporary.file().write_all(&bytes).is_ok() {
        // Best effort, as above: the temporary file is removed when it is not moved into place.
        drop(cache.insert_declaration(temporary, plugin, &key));
    }
}

/// [`PluginInfo`] as an entry holds it.
#[derive(Debug, Deserialize, Serialize)]
struct Stored {
    name: String,
    version: String,
    options_schema: String,
    rules: Vec<StoredRule>,
    generators: Vec<StoredGenerator>,
    presets: Vec<StoredPreset>,
}

/// [`RuleInfo`] as an entry holds it.
#[derive(Debug, Deserialize, Serialize)]
struct StoredRule {
    name: String,
    title: String,
    aspects: Vec<String>,
    /// Whether its scope is the repository; otherwise it is a project.
    repository: bool,
    reads_history: bool,
    reads_message: bool,
    languages: Vec<String>,
    options_schema: String,
}

/// [`GeneratorInfo`] as an entry holds it.
#[derive(Debug, Deserialize, Serialize)]
struct StoredGenerator {
    name: String,
    title: String,
    options_schema: String,
    owns: Vec<String>,
}

/// [`PresetInfo`] as an entry holds it.
#[derive(Debug, Deserialize, Serialize)]
struct StoredPreset {
    name: String,
    rules: String,
}

impl From<&PluginInfo> for Stored {
    fn from(info: &PluginInfo) -> Self {
        Self {
            name: info.name.clone(),
            version: info.version.clone(),
            options_schema: info.options_schema.clone(),
            rules: info
                .rules
                .iter()
                .map(|rule| StoredRule {
                    name: rule.name.clone(),
                    title: rule.title.clone(),
                    aspects: rule.aspects.clone(),
                    repository: rule.scope == Scope::Repository,
                    reads_history: rule.reads_history,
                    reads_message: rule.reads_message,
                    languages: rule.languages.clone(),
                    options_schema: rule.options_schema.clone(),
                })
                .collect(),
            generators: info
                .generators
                .iter()
                .map(|generator| StoredGenerator {
                    name: generator.name.clone(),
                    title: generator.title.clone(),
                    options_schema: generator.options_schema.clone(),
                    owns: generator.owns.clone(),
                })
                .collect(),
            presets: info
                .presets
                .iter()
                .map(|preset| StoredPreset {
                    name: preset.name.clone(),
                    rules: preset.rules.clone(),
                })
                .collect(),
        }
    }
}

impl From<Stored> for PluginInfo {
    fn from(stored: Stored) -> Self {
        Self {
            name: stored.name,
            version: stored.version,
            options_schema: stored.options_schema,
            rules: stored
                .rules
                .into_iter()
                .map(|rule| RuleInfo {
                    name: rule.name,
                    title: rule.title,
                    aspects: rule.aspects,
                    scope: if rule.repository {
                        Scope::Repository
                    } else {
                        Scope::Project
                    },
                    reads_history: rule.reads_history,
                    reads_message: rule.reads_message,
                    languages: rule.languages,
                    options_schema: rule.options_schema,
                })
                .collect(),
            generators: stored
                .generators
                .into_iter()
                .map(|generator| GeneratorInfo {
                    name: generator.name,
                    title: generator.title,
                    options_schema: generator.options_schema,
                    owns: generator.owns,
                })
                .collect(),
            presets: stored
                .presets
                .into_iter()
                .map(|preset| PresetInfo {
                    name: preset.name,
                    rules: preset.rules,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> PluginInfo {
        PluginInfo {
            name: "acme".to_owned(),
            version: "1.2.3".to_owned(),
            options_schema: "{\"type\":\"object\"}".to_owned(),
            rules: vec![
                RuleInfo {
                    name: "one".to_owned(),
                    title: "The first".to_owned(),
                    aspects: vec!["a".to_owned(), "b".to_owned()],
                    scope: Scope::Project,
                    reads_history: false,
                    reads_message: false,
                    languages: vec!["typescript".to_owned()],
                    options_schema: "true".to_owned(),
                },
                RuleInfo {
                    name: "two".to_owned(),
                    title: "The second".to_owned(),
                    aspects: Vec::new(),
                    scope: Scope::Repository,
                    reads_history: true,
                    reads_message: true,
                    languages: Vec::new(),
                    options_schema: "{\"x-gate-needs\":[]}".to_owned(),
                },
            ],
            generators: vec![GeneratorInfo {
                name: "write".to_owned(),
                title: "Writes".to_owned(),
                options_schema: "true".to_owned(),
                owns: vec!["gen/**".to_owned()],
            }],
            presets: vec![PresetInfo {
                name: "recommended".to_owned(),
                rules: "{\"one\":{\"level\":\"error\"}}".to_owned(),
            }],
        }
    }

    #[test]
    fn a_stored_declaration_reads_back_as_written_and_only_under_its_plugin() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let cache = Cache::at(directory.path());
        let plugin = Sha256Sum::from_digest(&Sha256::digest(b"plugin"));
        let other = Sha256Sum::from_digest(&Sha256::digest(b"other"));
        assert_eq!(read(&cache, &plugin), None);

        write(&cache, &plugin, &info());
        assert_eq!(read(&cache, &plugin), Some(info()));
        assert_eq!(read(&cache, &other), None);
    }

    #[test]
    fn a_stored_declaration_is_replaced_when_written_again() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let cache = Cache::at(directory.path());
        let plugin = Sha256Sum::from_digest(&Sha256::digest(b"plugin"));
        let path = cache.declaration(&plugin, &key());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directories");
        std::fs::write(&path, b"{\"name\":").expect("entry");

        write(&cache, &plugin, &info());
        assert_eq!(read(&cache, &plugin), Some(info()));
    }

    #[test]
    fn the_entry_name_changes_with_the_format_the_interface_and_the_core_version() {
        let name = key_for(1, "gate:plugin@0.2.0", "0.2.1");
        assert_eq!(name, key_for(1, "gate:plugin@0.2.0", "0.2.1"));
        let changed = [
            key_for(2, "gate:plugin@0.2.0", "0.2.1"),
            key_for(1, "gate:plugin@0.3.0", "0.2.1"),
            key_for(1, "gate:plugin@0.2.0", "0.2.2"),
        ];
        for other in changed {
            assert_ne!(other, name);
        }
        assert_eq!(
            key(),
            key_for(FORMAT, gate_runtime::INTERFACE, env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn a_read_marks_the_entry_used() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let cache = Cache::at(directory.path());
        let plugin = Sha256Sum::from_digest(&Sha256::digest(b"plugin"));
        write(&cache, &plugin, &info());
        let path = cache.declaration(&plugin, &key());
        crate::cache::used_days_ago(&path, 40);

        assert_eq!(read(&cache, &plugin), Some(info()));
        let used = std::fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .expect("the entry's time");
        let since = std::time::SystemTime::now()
            .duration_since(used)
            .unwrap_or_default();
        assert!(
            since < std::time::Duration::from_secs(3600),
            "read {since:?} ago"
        );
    }

    #[test]
    fn an_entry_that_does_not_parse_reads_as_missing() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let cache = Cache::at(directory.path());
        let plugin = Sha256Sum::from_digest(&Sha256::digest(b"plugin"));
        let path = cache.declaration(&plugin, &key());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directories");
        std::fs::write(&path, b"{\"name\":").expect("entry");
        assert_eq!(read(&cache, &plugin), None);
    }
}

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

/// The name of this core's entry in a plugin's directory: the digest of the entry format, the
/// plugin interface and the core version, so no other core reads it.
fn key() -> String {
    let digest = Sha256::digest(format!(
        "{FORMAT}\0{}\0{}\0",
        gate_runtime::INTERFACE,
        env!("CARGO_PKG_VERSION")
    ));
    Sha256Sum::from_digest(&digest).as_str().to_owned()
}

/// What the plugin with this sha256 declared when it last loaded under this core; `None` when no
/// entry is stored or it cannot be read.
pub(super) fn read(cache: &Cache, plugin: &Sha256Sum) -> Option<PluginInfo> {
    let bytes = std::fs::read(cache.declaration(plugin, &key())).ok()?;
    let stored: Stored = serde_json::from_slice(&bytes).ok()?;
    Some(stored.into())
}

/// Stores what the plugin with this sha256 declares, unless an entry is there already. A cache
/// that cannot be written costs the next run a load of the plugin, not a failure, so nothing is
/// reported.
pub(super) fn write(cache: &Cache, plugin: &Sha256Sum, info: &PluginInfo) {
    let key = key();
    if cache.declaration(plugin, &key).exists() {
        return;
    }
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
                    languages: vec!["typescript".to_owned()],
                    options_schema: "true".to_owned(),
                },
                RuleInfo {
                    name: "two".to_owned(),
                    title: "The second".to_owned(),
                    aspects: Vec::new(),
                    scope: Scope::Repository,
                    reads_history: true,
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

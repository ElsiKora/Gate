//! The pins of Gate's own files that a release commit of `git/main-history` may move: the `source`
//! of one plugin in each `gate.toml`, and its entries in the root `gate.lock`; with
//! `release-all-pins`, also the core's pin and every other plugin's.
//!
//! The files are compared as parsed TOML: everything but the pins the release may move must be
//! equal at the parent and at the commit.

use std::collections::BTreeMap;

use gate_sdk::{Error, Result};
use toml::{Table, Value};

/// The configuration files whose plugin pins a release may move, at any depth.
pub(crate) const CONFIG: &str = "gate.toml";

/// The lock file, at the repository root only.
pub(crate) const LOCK: &str = "gate.lock";

/// What a release commit may move.
pub(crate) struct Pins {
    /// The plugin's source without its version, `<owner>/<repo>:<plugin>`.
    plugin: String,
    /// Whether every other pin may move too.
    all: bool,
}

/// What is wrong with one pin file of a release commit.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Problem {
    /// It changes more than the pins the release may move.
    Beyond,
    /// It moves the plugin to this version, which is not the release's.
    Version(String),
    /// It does not read as TOML at the parent or at the commit.
    Unreadable,
}

impl Pins {
    /// The pins of `release-plugin` and `release-all-pins`, checked against `release-files`.
    ///
    /// # Errors
    /// When `release-all-pins` is set without `release-plugin`, `release-plugin` is not a source
    /// without its version, or `release-files` names a pin file `release-plugin` limits.
    pub(crate) fn new(
        plugin: Option<&str>,
        all: bool,
        release_files: &[String],
    ) -> Result<Option<Self>> {
        let Some(plugin) = plugin else {
            if all {
                return Err(Error::config(
                    "git/main-history release-all-pins needs release-plugin, the plugin whose \
                     release moves the pins; set release-plugin or remove release-all-pins",
                ));
            }
            return Ok(None);
        };
        if !is_plugin(plugin) {
            return Err(Error::config(format!(
                "git/main-history release-plugin {plugin:?} is not a plugin source without its \
                 version; write <owner>/<repo>:<plugin>, such as \"acme/rules:product\""
            )));
        }
        if let Some(file) = release_files.iter().find(|file| is_pin_file(file)) {
            return Err(Error::config(format!(
                "git/main-history release-files names {file}, which release-plugin admits limited \
                 to its pins; remove {file} from release-files"
            )));
        }
        Ok(Some(Self {
            plugin: plugin.to_owned(),
            all,
        }))
    }

    /// The plugin, `<owner>/<repo>:<plugin>`.
    pub(crate) fn plugin(&self) -> &str {
        &self.plugin
    }

    /// Whether every other pin may move too.
    pub(crate) fn all(&self) -> bool {
        self.all
    }

    /// The problems of the pin file `path` of a release of `version`, from its bytes at the
    /// parent and at the commit (`None` where it is absent).
    pub(crate) fn judge(
        &self,
        path: &str,
        before: Option<&[u8]>,
        after: Option<&[u8]>,
        version: &str,
    ) -> Vec<Problem> {
        let (Some(mut before), Some(mut after)) = (before.and_then(table), after.and_then(table))
        else {
            return vec![Problem::Unreadable];
        };
        let (old, new) = if path == LOCK {
            (self.lock(&mut before), self.lock(&mut after))
        } else {
            (self.config(&mut before), self.config(&mut after))
        };
        let mut problems: Vec<Problem> = new
            .into_iter()
            .filter(|(place, moved)| old.get(place) != Some(moved) && moved != version)
            .map(|(_, moved)| Problem::Version(moved))
            .collect();
        problems.dedup();
        if before != after {
            problems.insert(0, Problem::Beyond);
        }
        problems
    }

    /// `config` without the pins the release may move; the versions of the plugin's sources, by
    /// where each is.
    fn config(&self, config: &mut Table) -> BTreeMap<String, String> {
        let mut versions = BTreeMap::new();
        self.sources(config, "", &mut versions);
        if self.all
            && let Some(gate) = config.get_mut("gate").and_then(Value::as_table_mut)
        {
            gate.remove("version");
        }
        versions
    }

    /// Every `source` under `table` that names a plugin at a version loses its version when it is
    /// the plugin's, or any plugin's with `all`; the plugin's versions go to `versions`.
    fn sources(&self, table: &mut Table, at: &str, versions: &mut BTreeMap<String, String>) {
        for (key, value) in table.iter_mut() {
            let place = format!("{at}.{key}");
            match value {
                Value::String(text) if key == "source" => {
                    if let Some((plugin, version)) = split(text) {
                        if plugin == self.plugin {
                            versions.insert(place, version.to_owned());
                        }
                        if plugin == self.plugin || self.all {
                            *text = plugin.to_owned();
                        }
                    }
                }
                Value::Table(inner) => self.sources(inner, &place, versions),
                Value::Array(items) => {
                    for (index, item) in items.iter_mut().enumerate() {
                        if let Value::Table(inner) = item {
                            self.sources(inner, &format!("{place}[{index}]"), versions);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// `lock` without the pins the release may move: the plugin's entries, and with `all` every
    /// plugin's and the core's, are keyed by the plugin without its version and lose `url` and
    /// `sha256`. The plugin's versions, each keyed by itself.
    fn lock(&self, lock: &mut Table) -> BTreeMap<String, String> {
        let mut versions = BTreeMap::new();
        if self.all {
            lock.remove("core");
        }
        if let Some(plugins) = lock.get_mut("plugins").and_then(Value::as_table_mut) {
            let mut kept = Table::new();
            for (source, mut entry) in std::mem::take(plugins) {
                match split(&source) {
                    Some((plugin, version)) if plugin == self.plugin || self.all => {
                        if plugin == self.plugin {
                            versions.insert(version.to_owned(), version.to_owned());
                        }
                        if let Some(fields) = entry.as_table_mut() {
                            fields.remove("url");
                            fields.remove("sha256");
                        }
                        kept.insert(plugin.to_owned(), entry);
                    }
                    _ => {
                        kept.insert(source, entry);
                    }
                }
            }
            *plugins = kept;
        }
        versions
    }
}

/// `bytes` read as a TOML table.
fn table(bytes: &[u8]) -> Option<Table> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

/// A plugin source, `<owner>/<repo>:<plugin>@<version>`, split at its version.
fn split(source: &str) -> Option<(&str, &str)> {
    let (plugin, version) = source.rsplit_once('@')?;
    (is_plugin(plugin) && !version.is_empty()).then_some((plugin, version))
}

/// `<owner>/<repo>:<plugin>`, each part non-empty, without `@` or white space.
fn is_plugin(text: &str) -> bool {
    let Some((repository, plugin)) = text.split_once(':') else {
        return false;
    };
    let Some((owner, repo)) = repository.split_once('/') else {
        return false;
    };
    [owner, repo, plugin].iter().all(|part| {
        !part.is_empty() && !part.contains(['@', '/', ':']) && !part.contains(char::is_whitespace)
    })
}

/// A file a release of a plugin moves its pins in: any `gate.toml`, and the root `gate.lock`.
pub(crate) fn is_pin_file(path: &str) -> bool {
    path == LOCK || path == CONFIG || path.ends_with(&format!("/{CONFIG}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG_TEXT: &str = r#"
[gate]
version = "0.2.0"

[plugins.product]
source = "acme/rules:product@1.1.0"
options = { strict = true }

[plugins.git]
source = "ElsiKora/Gate:git@0.2.0"
"#;

    const LOCK_TEXT: &str = r#"
[core]
version = "0.2.0"

[core.targets.x86_64-unknown-linux-musl]
url = "https://example.test/gate-0.2.0.tar.gz"
sha256 = "aa"

[plugins."acme/rules:product@1.1.0"]
url = "https://example.test/product-1.1.0.wasm"
sha256 = "bb"

[plugins."ElsiKora/Gate:git@0.2.0"]
url = "https://example.test/git-0.2.0.wasm"
sha256 = "cc"
"#;

    fn judge(all: bool, path: &str, before: &str, after: &str) -> Vec<Problem> {
        let pins = Pins::new(Some("acme/rules:product"), all, &[])
            .expect("valid")
            .expect("set");
        pins.judge(
            path,
            Some(before.as_bytes()),
            Some(after.as_bytes()),
            "1.2.0",
        )
    }

    #[test]
    fn only_the_pins_the_release_may_move_change() {
        let moved = CONFIG_TEXT.replace("product@1.1.0", "product@1.2.0");
        assert_eq!(judge(false, "gate.toml", CONFIG_TEXT, &moved), []);
        assert_eq!(judge(false, "apps/a/gate.toml", CONFIG_TEXT, &moved), []);
        let wrong = CONFIG_TEXT.replace("product@1.1.0", "product@1.3.0");
        assert_eq!(
            judge(false, "gate.toml", CONFIG_TEXT, &wrong),
            [Problem::Version("1.3.0".to_owned())]
        );
        let options = moved.replace("strict = true", "strict = false");
        assert_eq!(
            judge(false, "gate.toml", CONFIG_TEXT, &options),
            [Problem::Beyond]
        );
        let everything = moved
            .replace("version = \"0.2.0\"", "version = \"0.3.0\"")
            .replace("git@0.2.0", "git@0.3.0");
        assert_eq!(
            judge(false, "gate.toml", CONFIG_TEXT, &everything),
            [Problem::Beyond]
        );
        assert_eq!(judge(true, "gate.toml", CONFIG_TEXT, &everything), []);

        let lock = LOCK_TEXT
            .replace("product@1.1.0", "product@1.2.0")
            .replace("product-1.1.0", "product-1.2.0")
            .replace("\"bb\"", "\"dd\"");
        assert_eq!(judge(false, "gate.lock", LOCK_TEXT, &lock), []);
        let core = lock
            .replace("gate-0.2.0", "gate-0.3.0")
            .replace("\"aa\"", "\"ee\"");
        assert_eq!(
            judge(false, "gate.lock", LOCK_TEXT, &core),
            [Problem::Beyond]
        );
        assert_eq!(judge(true, "gate.lock", LOCK_TEXT, &core), []);
        let removed = LOCK_TEXT.replace(
            "[plugins.\"ElsiKora/Gate:git@0.2.0\"]\nurl = \"https://example.test/git-0.2.0.wasm\"\nsha256 = \"cc\"\n",
            "",
        );
        assert_eq!(
            judge(true, "gate.lock", LOCK_TEXT, &removed),
            [Problem::Beyond]
        );
        assert_eq!(
            judge(false, "gate.lock", LOCK_TEXT, "not toml ["),
            [Problem::Unreadable]
        );
    }
}

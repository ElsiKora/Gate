//! The pins of Gate's own files that a release commit of `git/main-history` may move: the `source`
//! of one plugin in each `gate.toml`, and its entries in the root `gate.lock`; with
//! `release-all-pins`, also the core's pin and every other plugin's.
//!
//! The files are compared as parsed TOML, so a change of layout or comments alone is not seen.
//! Everything but a moved pin must be equal at the parent and at the commit. A pin moves when its
//! version does: a `[plugins.<name>] source` from `<plugin>@<old>` to `<plugin>@<new>`, a lock
//! entry's key from `<plugin>@<old>` to `<plugin>@<new>` (each old key giving way to one new key,
//! never kept beside it), the lock's `[core] version`. Only a moved pin may change its `url` and
//! `sha256`; an entry whose key is unchanged compares in full.

use std::collections::{BTreeMap, BTreeSet};

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
    /// It changes more than the pins the release may move, or the commit adds or deletes it.
    Beyond,
    /// It moves the plugin to this version, which is not the release's.
    Version(String),
    /// It is not UTF-8 TOML at the parent or at the commit.
    Unreadable,
}

/// A lock's entries of the plugins whose pins may move: by plugin, then by version.
type Entries = BTreeMap<String, BTreeMap<String, Value>>;

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
        // Added or deleted: more than a pin moved.
        let (Some(before), Some(after)) = (before, after) else {
            return vec![Problem::Beyond];
        };
        let (Some(mut before), Some(mut after)) = (table(before), table(after)) else {
            return vec![Problem::Unreadable];
        };
        let mut moved = BTreeSet::new();
        let mut beyond = if path == LOCK {
            self.lock(&mut before, &mut after, &mut moved)
        } else {
            self.config(&mut before, &mut after, &mut moved);
            false
        };
        beyond |= before != after;
        let mut problems: Vec<Problem> = moved
            .into_iter()
            .filter(|moved| moved != version)
            .map(Problem::Version)
            .collect();
        if beyond {
            problems.insert(0, Problem::Beyond);
        }
        problems
    }

    /// Whether the pin of `plugin` may move.
    fn movable(&self, plugin: &str) -> bool {
        self.all || plugin == self.plugin
    }

    /// Takes the moved pins that may move out of both sides of a configuration: each
    /// `[plugins.<name>] source` whose version moved loses it on both sides, and with `all` the
    /// `[gate] version`. The versions the plugin moved to go to `moved`.
    fn config(&self, before: &mut Table, after: &mut Table, moved: &mut BTreeSet<String>) {
        if self.all {
            for side in [&mut *before, &mut *after] {
                if let Some(gate) = side.get_mut("gate").and_then(Value::as_table_mut) {
                    gate.remove("version");
                }
            }
        }
        let (Some(old), Some(new)) = (plugins(before), plugins(after)) else {
            return;
        };
        for (name, entry) in new.iter_mut() {
            let (Some(was), Some(now)) = (source(old.get_mut(name)), source(Some(entry))) else {
                continue;
            };
            let (Some((plugin, from)), Some((same, to))) = (owned(was), owned(now)) else {
                continue;
            };
            if plugin != same || from == to || !self.movable(&plugin) {
                continue;
            }
            if plugin == self.plugin {
                moved.insert(to);
            }
            was.clone_from(&plugin);
            *now = plugin;
        }
    }

    /// Takes the moved pins that may move out of both sides of the lock: with `all`, the core's
    /// pin when its version moved, and the entries of each plugin whose pin may move. Whether
    /// those entries change beyond their moves: a kept key whose entry changes, an old key not
    /// given way to exactly one new key, or a new entry that differs from the old one in more
    /// than `url` and `sha256`. The versions the plugin moved to go to `moved`.
    fn lock(&self, before: &mut Table, after: &mut Table, moved: &mut BTreeSet<String>) -> bool {
        if self.all {
            let core = |side: &Table| {
                side.get("core")
                    .and_then(Value::as_table)
                    .and_then(|core| core.get("version"))
                    .cloned()
            };
            if core(&*before) != core(&*after) {
                for side in [&mut *before, &mut *after] {
                    if let Some(core) = side.get_mut("core") {
                        if let Some(fields) = core.as_table_mut() {
                            fields.remove("version");
                        }
                        unpin(core);
                    }
                }
            }
        }
        let (Some(old), Some(new)) = (plugins(before), plugins(after)) else {
            return false;
        };
        let (mut old, mut new) = (self.entries(old), self.entries(new));
        let names: BTreeSet<String> = old.keys().chain(new.keys()).cloned().collect();
        let mut beyond = false;
        for name in names {
            let was = old.remove(&name).unwrap_or_default();
            let now = new.remove(&name).unwrap_or_default();
            beyond |= was
                .iter()
                .any(|(version, entry)| now.get(version).is_some_and(|kept| kept != entry));
            let gone: Vec<&Value> = was
                .iter()
                .filter(|(version, _)| !now.contains_key(*version))
                .map(|(_, entry)| entry)
                .collect();
            let came: Vec<(&String, &Value)> = now
                .iter()
                .filter(|(version, _)| !was.contains_key(*version))
                .collect();
            beyond |= gone.len() != came.len()
                || gone
                    .iter()
                    .zip(&came)
                    .any(|(gone, (_, came))| stripped(gone) != stripped(came));
            if name == self.plugin {
                moved.extend(came.into_iter().map(|(version, _)| version.clone()));
            }
        }
        beyond
    }

    /// The entries of `plugins` whose pins may move, taken out of it; the others stay, to be
    /// compared in full.
    fn entries(&self, plugins: &mut Table) -> Entries {
        let keys: Vec<String> = plugins
            .keys()
            .filter(|key| split(key).is_some_and(|(plugin, _)| self.movable(plugin)))
            .cloned()
            .collect();
        let mut entries = Entries::new();
        for key in keys {
            if let (Some((plugin, version)), Some(entry)) = (split(&key), plugins.remove(&key)) {
                entries
                    .entry(plugin.to_owned())
                    .or_default()
                    .insert(version.to_owned(), entry);
            }
        }
        entries
    }
}

/// `bytes` read as a TOML table.
fn table(bytes: &[u8]) -> Option<Table> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

/// The `[plugins]` table of a configuration or lock.
fn plugins(table: &mut Table) -> Option<&mut Table> {
    table.get_mut("plugins").and_then(Value::as_table_mut)
}

/// The `source` string of a configuration's plugin entry.
fn source(entry: Option<&mut Value>) -> Option<&mut String> {
    match entry?.as_table_mut()?.get_mut("source")? {
        Value::String(text) => Some(text),
        _ => None,
    }
}

/// A plugin source split at its version, owned.
fn owned(source: &str) -> Option<(String, String)> {
    split(source).map(|(plugin, version)| (plugin.to_owned(), version.to_owned()))
}

/// A lock entry without its `url` and `sha256`.
fn stripped(entry: &Value) -> Value {
    let mut entry = entry.clone();
    if let Some(fields) = entry.as_table_mut() {
        fields.remove("url");
        fields.remove("sha256");
    }
    entry
}

/// `value` without any `url` or `sha256`, at every depth.
fn unpin(value: &mut Value) {
    if let Some(fields) = value.as_table_mut() {
        fields.remove("url");
        fields.remove("sha256");
        for inner in fields.values_mut() {
            unpin(inner);
        }
    }
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
            .replace("[core]\nversion = \"0.2.0\"", "[core]\nversion = \"0.3.0\"")
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

    /// A section appended to `text`.
    fn with(text: &str, section: &str) -> String {
        format!("{text}\n{section}\n")
    }

    #[test]
    fn a_pin_changes_its_url_or_sha256_only_when_its_version_moves() {
        let swapped = LOCK_TEXT.replace("\"bb\"", "\"ff\"");
        assert_eq!(
            judge(false, "gate.lock", LOCK_TEXT, &swapped),
            [Problem::Beyond]
        );
        assert_eq!(
            judge(true, "gate.lock", LOCK_TEXT, &swapped),
            [Problem::Beyond]
        );
        let url = LOCK_TEXT.replace("git-0.2.0.wasm", "git-0.2.0-b.wasm");
        assert_eq!(judge(true, "gate.lock", LOCK_TEXT, &url), [Problem::Beyond]);
        // The core's checksum, its version unchanged.
        let core = LOCK_TEXT.replace("\"aa\"", "\"ee\"");
        assert_eq!(
            judge(true, "gate.lock", LOCK_TEXT, &core),
            [Problem::Beyond]
        );
    }

    #[test]
    fn an_old_lock_key_gives_way_to_one_new_key() {
        let beside = with(
            LOCK_TEXT,
            "[plugins.\"acme/rules:product@1.2.0\"]\nurl = \"https://example.test/product-1.2.0.wasm\"\nsha256 = \"dd\"",
        );
        assert_eq!(
            judge(false, "gate.lock", LOCK_TEXT, &beside),
            [Problem::Beyond]
        );
        let second = with(
            LOCK_TEXT,
            "[plugins.\"ElsiKora/Gate:git@0.1.0\"]\nurl = \"https://example.test/git-0.1.0.wasm\"\nsha256 = \"11\"",
        );
        assert_eq!(
            judge(true, "gate.lock", LOCK_TEXT, &second),
            [Problem::Beyond]
        );
        // A plugin added, even with release-all-pins.
        let added = with(
            LOCK_TEXT,
            "[plugins.\"acme/other:extra@1.0.0\"]\nurl = \"https://example.test/extra-1.0.0.wasm\"\nsha256 = \"22\"",
        );
        assert_eq!(
            judge(true, "gate.lock", LOCK_TEXT, &added),
            [Problem::Beyond]
        );
        let added = with(
            CONFIG_TEXT,
            "[plugins.extra]\nsource = \"acme/other:extra@1.0.0\"",
        );
        assert_eq!(
            judge(true, "gate.toml", CONFIG_TEXT, &added),
            [Problem::Beyond]
        );
    }

    #[test]
    fn only_a_plugin_s_source_is_a_pin_and_layout_is_not_a_change() {
        let option = CONFIG_TEXT.replace(
            "options = { strict = true }",
            "options = { strict = true, source = \"acme/rules:product@1.1.0\" }",
        );
        let moved = option.replace(
            "source = \"acme/rules:product@1.1.0\" }",
            "source = \"acme/rules:product@1.2.0\" }",
        );
        assert_eq!(
            judge(false, "gate.toml", &option, &moved),
            [Problem::Beyond]
        );
        assert_eq!(judge(true, "gate.toml", &option, &moved), [Problem::Beyond]);
        // The core's version alone, without release-all-pins.
        let core = CONFIG_TEXT.replace("version = \"0.2.0\"", "version = \"0.3.0\"");
        assert_eq!(
            judge(false, "gate.toml", CONFIG_TEXT, &core),
            [Problem::Beyond]
        );
        assert_eq!(judge(true, "gate.toml", CONFIG_TEXT, &core), []);
        // White space and comments.
        let layout = format!(
            "# The pins.\n{}",
            CONFIG_TEXT.replace("source = ", "source   =   ")
        );
        assert_eq!(judge(false, "gate.toml", CONFIG_TEXT, &layout), []);
        let layout = LOCK_TEXT.replace("\n\n", "\n\n# An entry.\n\n");
        assert_eq!(judge(false, "gate.lock", LOCK_TEXT, &layout), []);
    }

    #[test]
    fn an_added_or_deleted_pin_file_is_beyond_and_a_broken_one_unreadable() {
        let pins = Pins::new(Some("acme/rules:product"), false, &[])
            .expect("valid")
            .expect("set");
        let text = Some(CONFIG_TEXT.as_bytes());
        assert_eq!(
            pins.judge("gate.toml", None, text, "1.2.0"),
            [Problem::Beyond]
        );
        assert_eq!(
            pins.judge("gate.toml", text, None, "1.2.0"),
            [Problem::Beyond]
        );
        assert_eq!(
            pins.judge("gate.toml", text, Some([0xff_u8, 0xfe].as_slice()), "1.2.0"),
            [Problem::Unreadable]
        );
    }
}

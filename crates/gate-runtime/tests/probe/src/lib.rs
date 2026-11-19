//! A plugin that probes the runtime: what it may read, what git and WASI answer, and how it fails.
//! Each rule reports what it saw as finding messages, which the runtime's tests compare.

use gate_sdk::{Generate, Generator, Location, Rule, Run, schemars};

/// The `[plugins.probe] options`: every key optional, so a configuration that sets none passes.
#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ProbeOptions {
    /// Reported by `read` as `label <label>`.
    label: Option<String>,
}

/// Reads the `paths` of its options and lists `list`, one finding per answer, and reports the
/// plugin's `label` when it is set; reports one more finding at `at` (with aspect `aspect`) when it
/// is set, one at commit `commit`, and, with `at_options_file`, one at the file its options came
/// from, whose message is what reading that file answers; with `fail`, fails with that text after
/// reading, behind the path of its options file as the rule addresses it. The finding at `at` reads
/// `at_message` (default `at`) and carries the key `key` when it is set; with `line_of` as well, the
/// finding is at the first line of `at` that holds `line_of`, reads `<at>:<line> <line_of>` and
/// carries the key `line_of`.
#[derive(Debug, Default)]
pub struct Read;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadOptions {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    list: Option<String>,
    #[serde(default)]
    at: Option<String>,
    #[serde(default)]
    aspect: String,
    #[serde(default)]
    at_message: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    line_of: Option<String>,
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    at_options_file: bool,
    #[serde(default)]
    fail: Option<String>,
}

impl Rule for Read {
    type Options = ReadOptions;
    const NAME: &'static str = "read";
    const TITLE: &'static str = "Reports what reading and listing answer";

    fn check(&self, run: &mut Run<'_, ReadOptions>) -> gate_sdk::Result<()> {
        let tree = run.tree();
        let mut messages = Vec::new();
        for path in &run.options().paths {
            messages.push(match tree.bytes(path) {
                Some(bytes) => format!("{path}: {}", String::from_utf8_lossy(&bytes)),
                None => format!("{path}: none"),
            });
        }
        if let Some(prefix) = &run.options().list {
            messages.push(format!("list {prefix}: {}", tree.under(prefix).join(",")));
        }
        if let Some(label) = run.plugin_options::<ProbeOptions>()?.label {
            messages.push(format!("label {label}"));
        }
        for message in messages {
            run.report().finding("", Location::file("probe"), message);
        }
        if let (Some(at), Some(needle)) = (run.options().at.clone(), run.options().line_of.clone())
        {
            let text = tree.text(&at).unwrap_or_default();
            let line = text
                .lines()
                .position(|line| line.contains(needle.as_str()))
                .map_or(1, |index| index + 1);
            let aspect = run.options().aspect.clone();
            run.report()
                .finding(
                    &aspect,
                    Location::line(at.clone(), line),
                    format!("{at}:{line} {needle}"),
                )
                .key(needle);
        } else if let Some(at) = run.options().at.clone() {
            let aspect = run.options().aspect.clone();
            let message = run.options().at_message.clone();
            let key = run.options().key.clone();
            let finding = run.report().finding(
                &aspect,
                Location::file(at),
                message.unwrap_or_else(|| "at".to_owned()),
            );
            if let Some(key) = key {
                finding.key(key);
            }
        }
        if let Some(commit) = run.options().commit.clone() {
            run.report().finding("", Location::commit(commit), "commit");
        }
        if run.options().at_options_file {
            report_options_file(run);
        }
        match run.options().fail.clone() {
            Some(message) => {
                let file = run.options_file().unwrap_or("no options file");
                Err(gate_sdk::Error::new(format!("{file}: {message}")))
            }
            None => Ok(()),
        }
    }
}

/// Reports a finding at the file the rule's options came from, whose message is what reading that
/// file answers; nothing when its options came from no file.
fn report_options_file<O>(run: &mut Run<'_, O>) {
    let Some(file) = run.options_file().map(str::to_owned) else {
        return;
    };
    let message = match run.tree().text(&file) {
        Some(text) => format!("options file {file}: {}", text.trim()),
        None => format!("options file {file}: none"),
    };
    run.report().finding("", Location::file(file), message);
}

/// Allocates far more memory than the runtime's tests allow.
#[derive(Debug, Default)]
pub struct Hog;

impl Rule for Hog {
    type Options = gate_sdk::serde_json::Value;
    const NAME: &'static str = "hog";
    const TITLE: &'static str = "Allocates 256 MiB";

    fn check(&self, run: &mut Run<'_, Self::Options>) -> gate_sdk::Result<()> {
        let block = std::hint::black_box(vec![1_u8; 256 << 20]);
        run.report()
            .finding("", None, format!("allocated {}", block.len()));
        Ok(())
    }
}

/// Declares `reads-history` and reports what git answers: head, `main` resolved, the head's tree,
/// the length of its log and the run's range.
#[derive(Debug, Default)]
pub struct History;

impl Rule for History {
    type Options = gate_sdk::serde_json::Value;
    const NAME: &'static str = "history";
    const TITLE: &'static str = "Reports the head commit";
    const READS_HISTORY: bool = true;

    fn check(&self, run: &mut Run<'_, Self::Options>) -> gate_sdk::Result<()> {
        let git = run.git()?;
        let head = git.head().unwrap_or_default();
        let main = git.resolve("main").unwrap_or_default();
        let tree = git.tree(&head);
        let log = git.log("HEAD");
        let range = git.range().map_or_else(
            || "none".to_owned(),
            |range| {
                format!(
                    "{}..{} to {}",
                    range.base.as_deref().unwrap_or("none"),
                    range.head,
                    range.target_ref.as_deref().unwrap_or("none")
                )
            },
        );
        let messages = [
            format!("head: {head}"),
            format!("main: {main}"),
            format!("tree: {tree}"),
            format!("log: {}", log.len()),
            format!("range: {range}"),
        ];
        for message in messages {
            run.report().finding("", Location::commit(&head), message);
        }
        Ok(())
    }
}

/// Declares `reads-history` and reports what git answers for each question of its options, with
/// every revision resolved first; `raw_tree` goes to `git.tree` as it is.
#[derive(Debug, Default)]
pub struct GitProbe;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GitOptions {
    /// Revision and path.
    #[serde(default)]
    read_at: Vec<[String; 2]>,
    /// Parent and commit revisions.
    #[serde(default)]
    changed_paths: Vec<[String; 2]>,
    #[serde(default)]
    merge_tree: Vec<[String; 2]>,
    /// Ancestor and descendant revisions.
    #[serde(default)]
    is_ancestor: Vec<[String; 2]>,
    /// A range whose commit messages to report.
    #[serde(default)]
    log: Option<String>,
    #[serde(default)]
    raw_tree: Option<String>,
    /// Revisions whose sync state (produced and drifting paths) to report.
    #[serde(default)]
    sync_state: Vec<String>,
    /// A range of commits to ask sync-state for each: reports how many commits drift.
    #[serde(default)]
    sync_state_log: Option<String>,
}

impl Rule for GitProbe {
    type Options = GitOptions;
    const NAME: &'static str = "git";
    const TITLE: &'static str = "Reports what git answers";
    const READS_HISTORY: bool = true;

    fn check(&self, run: &mut Run<'_, GitOptions>) -> gate_sdk::Result<()> {
        let git = run.git()?;
        let id = |rev: &str| git.resolve(rev).unwrap_or_else(|| rev.to_owned());
        let options = run.options();
        let mut messages = Vec::new();
        for [rev, path] in &options.read_at {
            let text = git.read_at(&id(rev), path).map_or_else(
                || "none".to_owned(),
                |bytes| String::from_utf8_lossy(&bytes).trim_end().to_owned(),
            );
            messages.push(format!("read-at {rev}:{path}: {text}"));
        }
        for [parent, commit] in &options.changed_paths {
            let paths = git.changed_paths(&id(parent), &id(commit));
            messages.push(format!(
                "changed-paths {parent} {commit}: {}",
                paths.join(",")
            ));
        }
        for [first, second] in &options.merge_tree {
            let merged = git.merge_tree(&id(first), &id(second));
            let answer = if merged.is_some() {
                "clean"
            } else {
                "conflict"
            };
            messages.push(format!("merge-tree {first} {second}: {answer}"));
        }
        for [ancestor, descendant] in &options.is_ancestor {
            let answer = git.is_ancestor(&id(ancestor), &id(descendant));
            messages.push(format!("is-ancestor {ancestor} {descendant}: {answer}"));
        }
        if let Some(range) = &options.log {
            for commit in git.log(range) {
                messages.push(format!("log: {:?}", commit.message));
            }
        }
        let list = |paths: &[String]| {
            if paths.is_empty() {
                "none".to_owned()
            } else {
                paths.join(",")
            }
        };
        for rev in &options.sync_state {
            let state = git.sync_state(&id(rev));
            messages.push(format!(
                "sync-state {rev}: produced {}; drift {}",
                list(&state.produced),
                list(&state.drift)
            ));
        }
        if let Some(range) = &options.sync_state_log {
            let commits = git.log(range);
            let drifting = commits
                .iter()
                .filter(|commit| !git.sync_state(&commit.id).drift.is_empty())
                .count();
            messages.push(format!(
                "sync-state-log {range}: {} commits, {drifting} drift",
                commits.len()
            ));
        }
        if let Some(raw) = &options.raw_tree {
            messages.push(format!("tree {raw}: {}", git.tree(raw)));
        }
        for message in messages {
            run.report().finding("", None, message);
        }
        Ok(())
    }
}

/// Logs `log` lines and prints `stdout` lines of 1 KiB, then reports `done`.
#[derive(Debug, Default)]
pub struct Flood;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FloodOptions {
    #[serde(default)]
    log: usize,
    #[serde(default)]
    stdout: usize,
}

impl Rule for Flood {
    type Options = FloodOptions;
    const NAME: &'static str = "flood";
    const TITLE: &'static str = "Logs and prints more than the core keeps";

    fn check(&self, run: &mut Run<'_, FloodOptions>) -> gate_sdk::Result<()> {
        let line = "x".repeat(1023);
        for _ in 0..run.options().log {
            run.log(&line);
        }
        for _ in 0..run.options().stdout {
            println!("{line}");
        }
        run.report().finding("", None, "done");
        Ok(())
    }
}

/// Calls git without declaring `reads-history`, past the SDK, straight through the import.
#[derive(Debug, Default)]
pub struct Sneak;

impl Rule for Sneak {
    type Options = gate_sdk::serde_json::Value;
    const NAME: &'static str = "sneak";
    const TITLE: &'static str = "Calls git without declaring it";

    fn check(&self, run: &mut Run<'_, Self::Options>) -> gate_sdk::Result<()> {
        #[cfg(target_arch = "wasm32")]
        let head = gate_sdk::bindings::gate::plugin::git::head().unwrap_or_default();
        #[cfg(not(target_arch = "wasm32"))]
        let head = String::new();
        run.report().finding("", None, format!("head: {head}"));
        Ok(())
    }
}

/// Panics.
#[derive(Debug, Default)]
pub struct Panic;

impl Rule for Panic {
    type Options = gate_sdk::serde_json::Value;
    const NAME: &'static str = "panic";
    const TITLE: &'static str = "Panics";

    fn check(&self, _: &mut Run<'_, Self::Options>) -> gate_sdk::Result<()> {
        panic!("the probe panics on purpose")
    }
}

/// Never ends.
#[derive(Debug, Default)]
pub struct Spin;

impl Rule for Spin {
    type Options = gate_sdk::serde_json::Value;
    const NAME: &'static str = "spin";
    const TITLE: &'static str = "Never ends";

    fn check(&self, _: &mut Run<'_, Self::Options>) -> gate_sdk::Result<()> {
        let mut counter = 0_u64;
        loop {
            counter = std::hint::black_box(counter.wrapping_add(1));
        }
    }
}

/// Reports what the environment, arguments, clocks, hash-map order and file system answer.
#[derive(Debug, Default)]
pub struct Ambient;

impl Rule for Ambient {
    type Options = gate_sdk::serde_json::Value;
    const NAME: &'static str = "ambient";
    const TITLE: &'static str = "Reports the ambient answers";

    fn check(&self, run: &mut Run<'_, Self::Options>) -> gate_sdk::Result<()> {
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(u128::MAX, |duration| duration.as_nanos());
        let order: Vec<u32> = (0..16)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let messages = [
            format!("env: {}", std::env::vars().count()),
            format!("args: {}", std::env::args().count()),
            format!("wall: {since}"),
            format!("hash order: {order:?}"),
            format!("fs: {}", std::fs::read_dir("/").is_ok()),
        ];
        println!("probe stdout");
        eprintln!("probe stderr");
        run.log("probe log");
        for message in messages {
            run.report().finding("", None, message);
        }
        Ok(())
    }
}

/// Writes each `files` entry of its options (path to text) and each path the tracked file
/// `list` names (one per line, text "listed"), reports `report` as a finding at `gate.toml`, or
/// fails with `fail`, and owns `gen/**` and `GENERATED.md`.
#[derive(Debug, Default)]
pub struct Write;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct WriteOptions {
    #[serde(default)]
    files: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    fail: bool,
    #[serde(default)]
    list: Option<String>,
    #[serde(default)]
    report: Option<String>,
}

impl Generator for Write {
    type Options = WriteOptions;
    const NAME: &'static str = "write";
    const TITLE: &'static str = "Writes the files its options name";
    const OWNS: &'static [&'static str] = &["gen/**", "GENERATED.md"];

    fn generate(&self, run: &mut Generate<'_, WriteOptions>) -> gate_sdk::Result<()> {
        if run.options().fail {
            return Err(gate_sdk::Error::new("probe/write fails as its options ask"));
        }
        let files = run.options().files.clone();
        for (path, text) in files {
            run.write(path, text);
        }
        if let Some(list) = run.options().list.clone() {
            let listed = run.tree().text(&list).unwrap_or_default();
            for path in listed.lines().filter(|line| !line.is_empty()) {
                run.write(path.to_owned(), "listed\n");
            }
        }
        if let Some(message) = run.options().report.clone() {
            run.report()
                .finding("", Location::file("gate.toml"), message);
        }
        Ok(())
    }
}

/// Owns `*.toml`, which in a project covers its `gate.toml`.
#[derive(Debug, Default)]
pub struct Toml;

impl Generator for Toml {
    type Options = gate_sdk::serde_json::Value;
    const NAME: &'static str = "toml";
    const TITLE: &'static str = "Owns every TOML file of its root";
    const OWNS: &'static [&'static str] = &["*.toml"];

    fn generate(&self, _: &mut Generate<'_, Self::Options>) -> gate_sdk::Result<()> {
        Ok(())
    }
}

/// Reports its required option `pattern`, and, with `at_options_file`, what reading the file its
/// options came from answers, at that file.
#[derive(Debug, Default)]
pub struct Needs;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NeedsOptions {
    pattern: String,
    #[serde(default)]
    at_options_file: bool,
}

impl Rule for Needs {
    type Options = NeedsOptions;
    const NAME: &'static str = "needs";
    const TITLE: &'static str = "Needs the option pattern";

    fn check(&self, run: &mut Run<'_, NeedsOptions>) -> gate_sdk::Result<()> {
        let message = format!("needs {}", run.options().pattern);
        run.report().finding("", None, message);
        if run.options().at_options_file {
            report_options_file(run);
        }
        Ok(())
    }
}

/// Reports one finding of each of its aspects, `loud` and `quiet`.
#[derive(Debug, Default)]
pub struct Aspects;

impl Rule for Aspects {
    type Options = gate_sdk::NoOptions;
    const NAME: &'static str = "aspects";
    const TITLE: &'static str = "Reports one finding of each aspect";
    const ASPECTS: &'static [&'static str] = &["loud", "quiet"];

    fn check(&self, run: &mut Run<'_, Self::Options>) -> gate_sdk::Result<()> {
        for aspect in Self::ASPECTS {
            run.report()
                .finding(aspect, None, format!("aspect {aspect}"));
        }
        Ok(())
    }
}

/// Reports the norms its call carries.
#[derive(Debug, Default)]
pub struct Norms;

impl Rule for Norms {
    type Options = gate_sdk::serde_json::Value;
    const NAME: &'static str = "norms";
    const TITLE: &'static str = "Reports the norms of the call";

    fn check(&self, run: &mut Run<'_, Self::Options>) -> gate_sdk::Result<()> {
        let message = format!("norms: {}", run.norms().join(","));
        run.report().finding("", None, message);
        Ok(())
    }
}

/// Options whose schema cannot be built: `describe` traps building it, or never ends.
#[cfg(any(feature = "describe-traps", feature = "describe-spins"))]
#[derive(Debug, serde::Deserialize)]
pub struct Unschemable {}

#[cfg(any(feature = "describe-traps", feature = "describe-spins"))]
impl schemars::JsonSchema for Unschemable {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Unschemable".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        if cfg!(feature = "describe-spins") {
            let mut counter = 0_u64;
            loop {
                counter = std::hint::black_box(counter.wrapping_add(1));
            }
        }
        panic!("the probe's describe traps on purpose")
    }
}

/// A rule whose declaration traps or never ends.
#[cfg(any(feature = "describe-traps", feature = "describe-spins"))]
#[derive(Debug, Default)]
pub struct Undescribable;

#[cfg(any(feature = "describe-traps", feature = "describe-spins"))]
impl Rule for Undescribable {
    type Options = Unschemable;
    const NAME: &'static str = "undescribable";
    const TITLE: &'static str = "Traps while the plugin describes itself";

    fn check(&self, _: &mut Run<'_, Self::Options>) -> gate_sdk::Result<()> {
        Ok(())
    }
}

#[cfg(any(feature = "describe-traps", feature = "describe-spins"))]
gate_sdk::plugin! {
    name: "other",
    version: env!("CARGO_PKG_VERSION"),
    options: gate_sdk::NoOptions,
    rules: [Undescribable],
    generators: [],
    presets: [],
}

#[cfg(not(any(feature = "describe-traps", feature = "describe-spins")))]
gate_sdk::plugin! {
    name: "probe",
    version: env!("CARGO_PKG_VERSION"),
    options: ProbeOptions,
    rules: [Read, History, GitProbe, Sneak, Panic, Spin, Ambient, Hog, Flood, Needs, Norms, Aspects],
    generators: [Write, Toml],
    presets: [
        gate_sdk::Preset {
            name: "needs",
            rules: r#"{"needs": {"level": "error", "options": {"pattern": "from the preset"}}}"#,
        },
        gate_sdk::Preset {
            name: "needs-level",
            rules: r#"{"needs": {"level": "error"}}"#,
        },
        gate_sdk::Preset {
            name: "quiet",
            rules: r#"{"aspects": {"levels": {"quiet": "warning"}}}"#,
        },
    ],
}

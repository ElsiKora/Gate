//! `node/formatter`: one Prettier configuration, a root `.prettierrc.json` with the standard
//! settings, and no `.editorconfig` that changes what Prettier writes.

use std::collections::{BTreeMap, BTreeSet};

use gate_sdk::{Error, NoOptions, Result, Rule, Run, Scope, Tree};
use globset::GlobBuilder;

use crate::repo::{self, directory, file_name};
use crate::settings::Settings;
use crate::standard::PRETTIER;

#[derive(Debug, Default)]
pub(crate) struct Formatter;

/// The one form of the configuration.
const CONFIG: &str = ".prettierrc.json";

impl Rule for Formatter {
    type Options = NoOptions;
    const NAME: &'static str = "formatter";
    const TITLE: &'static str =
        "One Prettier configuration: a root .prettierrc.json with the standard settings";
    const ASPECTS: &'static [&'static str] = &["form", "nested", "settings", "editorconfig"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, NoOptions>) -> Result<()> {
        Settings::read(run)?;
        let tree = run.tree();
        let manifests = repo::manifests(&tree);
        if manifests.is_empty() {
            return Ok(());
        }
        for path in tree.paths() {
            if path == CONFIG || !is_config(file_name(&path)) {
                continue;
            }
            let (aspect, message) = if directory(&path).is_empty() {
                (
                    "form",
                    format!("{path} configures Prettier in another form; the one form is {CONFIG}"),
                )
            } else {
                (
                    "nested",
                    format!(
                        "{path} configures Prettier below the root; a repository has one configuration"
                    ),
                )
            };
            crate::report(
                run.report(),
                aspect,
                (&path, None),
                file_name(&path),
                message,
                "remove it; the root .prettierrc.json applies to every file of the repository",
            );
        }
        for path in &manifests {
            let Some(manifest) = repo::read_json(&tree, path)? else {
                continue;
            };
            if let Some(member) = manifest.member("prettier") {
                let aspect = if path == "package.json" {
                    "form"
                } else {
                    "nested"
                };
                crate::report(
                    run.report(),
                    aspect,
                    (path, Some(member.line)),
                    "prettier",
                    format!(
                        "{path} configures Prettier in its \"prettier\" key; the one form is the root {CONFIG}"
                    ),
                    "move the settings to the root .prettierrc.json and remove the key",
                );
            }
        }
        let set = settings(run, &tree)?;
        for path in tree.paths() {
            if file_name(&path) == ".editorconfig"
                && !path.split('/').any(|segment| segment == "node_modules")
            {
                editorconfig(run, &tree, &path, &set)?;
            }
        }
        Ok(())
    }
}

/// A file of each extension Prettier formats; a section applies to Prettier's files when its
/// pattern matches one of them.
const FORMATTED: [&str; 22] = [
    "x.js",
    "x.mjs",
    "x.cjs",
    "x.jsx",
    "x.ts",
    "x.mts",
    "x.cts",
    "x.tsx",
    "x.json",
    "x.json5",
    "x.jsonc",
    "x.md",
    "x.mdx",
    "x.yaml",
    "x.yml",
    "x.css",
    "x.scss",
    "x.less",
    "x.html",
    "x.vue",
    "x.graphql",
    "x.hbs",
];

/// One section of an `.editorconfig`: its pattern, and each key with its value and line.
struct Section {
    pattern: String,
    keys: BTreeMap<String, (String, usize)>,
}

/// `editorconfig`: Prettier 3 reads `.editorconfig` and maps six of its keys to its options, and
/// an option set in `.prettierrc.json` wins over the key. So a key is reported only when the
/// option it maps to is absent from `.prettierrc.json` (the keys in `set`) and the value Prettier
/// takes from it differs from the standard. Only sections that cover files Prettier formats are
/// read, each on its own.
fn editorconfig(
    run: &mut Run<'_, NoOptions>,
    tree: &Tree<'_>,
    path: &str,
    set: &BTreeSet<String>,
) -> Result<()> {
    let Some(bytes) = tree.bytes(path) else {
        return Ok(());
    };
    let text = String::from_utf8(bytes)
        .map_err(|_| Error::new(format!("{path} is not UTF-8 text; save it as UTF-8")))?;
    let mut sections: Vec<Section> = Vec::new();
    for (index, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(pattern) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            sections.push(Section {
                pattern: pattern.to_owned(),
                keys: BTreeMap::new(),
            });
            continue;
        }
        let (Some((key, value)), Some(section)) = (line.split_once('='), sections.last_mut())
        else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        if value.eq_ignore_ascii_case("unset") {
            section.keys.remove(&key);
        } else {
            section.keys.insert(key, (value.to_owned(), index + 1));
        }
    }
    for section in sections.iter().filter(|section| formats(&section.pattern)) {
        for (key, value, standard) in prettier_reads(section, set) {
            let line = section.keys.get(key).map_or(0, |(_, line)| *line);
            crate::report(
                run.report(),
                "editorconfig",
                (path, Some(line)),
                &format!("[{}]{key}", section.pattern),
                format!(
                    "{path} sets {key} = {value} for [{}], and Prettier reads it; the standard \
                     formats with {standard}",
                    section.pattern
                ),
                &format!(
                    "remove the key or write {key} = {standard}; .prettierrc.json is the one \
                     source of Prettier's settings"
                ),
            );
        }
    }
    Ok(())
}

/// The keys of `section` that change what Prettier writes, each with its value and the
/// standard's, as Prettier 3.10.1 maps them (`editorconfig-to-prettier.js`, after the editorconfig
/// parser's defaults: `tab_width` defaults to a numeric `indent_size`, and `indent_size = tab`
/// takes `tab_width` when there is one). An option already in `.prettierrc.json` (`set`) wins, so
/// its keys are not read.
fn prettier_reads<'s>(
    section: &'s Section,
    set: &BTreeSet<String>,
) -> Vec<(&'static str, &'s str, &'static str)> {
    let value = |key: &str| section.keys.get(key).map(|(value, _)| value.as_str());
    let lower = |key: &str| value(key).map(str::to_ascii_lowercase);
    let number = |key: &str| value(key).filter(|text| text.parse::<u32>().is_ok_and(|n| n > 0));
    let style = lower("indent_style");
    let size = lower("indent_size");
    let mut found = Vec::new();
    if !set.contains("useTabs") {
        if style.as_deref() == Some("tab") {
            found.push((
                "indent_style",
                value("indent_style").unwrap_or_default(),
                "space",
            ));
        } else if style.is_none() && size.as_deref() == Some("tab") && value("tab_width").is_none()
        {
            found.push(("indent_size", value("indent_size").unwrap_or_default(), "2"));
        }
    }
    if !set.contains("tabWidth") {
        let width = if style.as_deref() == Some("space") && number("indent_size").is_some() {
            Some("indent_size")
        } else if number("tab_width").is_some() {
            Some("tab_width")
        } else {
            number("indent_size").map(|_| "indent_size")
        };
        if let Some(key) = width
            && let Some(text) = value(key)
            && text != "2"
        {
            found.push((key, text, "2"));
        }
    }
    if !set.contains("printWidth")
        && let Some(text) = value("max_line_length")
        && (text == "off" || number("max_line_length").is_some())
        && text != "120"
    {
        found.push(("max_line_length", text, "120"));
    }
    if !set.contains("singleQuote") && value("quote_type") == Some("single") {
        found.push(("quote_type", "single", "double"));
    }
    if !set.contains("endOfLine")
        && let Some(text) = value("end_of_line")
        && matches!(lower("end_of_line").as_deref(), Some("crlf" | "cr"))
    {
        found.push(("end_of_line", text, "lf"));
    }
    found.sort_by_key(|(key, _, _)| section.keys.get(*key).map_or(0, |(_, line)| *line));
    found
}

/// Whether the section `pattern` covers a file Prettier formats. A pattern with a directory is
/// matched by its last segment, so a section for some directory's files counts too.
fn formats(pattern: &str) -> bool {
    let last = pattern.rsplit('/').next().unwrap_or(pattern);
    // A pattern globset cannot read (an editorconfig number range) is taken to cover them.
    match GlobBuilder::new(last).literal_separator(false).build() {
        Ok(glob) => {
            let matcher = glob.compile_matcher();
            FORMATTED.iter().any(|file| matcher.is_match(file))
        }
        Err(_) => true,
    }
}

/// Whether a file of this name configures Prettier.
fn is_config(name: &str) -> bool {
    name == ".prettierrc"
        || name
            .strip_prefix(".prettierrc")
            .or_else(|| name.strip_prefix("prettier.config"))
            .is_some_and(|rest| {
                matches!(
                    rest,
                    ".json"
                        | ".json5"
                        | ".yaml"
                        | ".yml"
                        | ".toml"
                        | ".js"
                        | ".cjs"
                        | ".mjs"
                        | ".ts"
                        | ".cts"
                        | ".mts"
                )
            })
}

/// `settings`: the root configuration holds exactly the standard. Returns the keys it sets.
fn settings(run: &mut Run<'_, NoOptions>, tree: &Tree<'_>) -> Result<BTreeSet<String>> {
    let Some(config) = repo::read_json(tree, CONFIG)? else {
        crate::report(
            run.report(),
            "form",
            (CONFIG, None),
            CONFIG,
            format!("{CONFIG} is not tracked at the repository root"),
            "add .prettierrc.json with the standard settings: {\"printWidth\": 120, \"proseWrap\": \"preserve\", \"trailingComma\": \"all\"}",
        );
        return Ok(BTreeSet::new());
    };
    for (key, value) in PRETTIER {
        let found = config.member(key);
        if found.is_some_and(|member| member.value.render() == value) {
            continue;
        }
        let message = match found {
            Some(member) => format!(
                "{CONFIG} sets {key} to {}; the standard is {value}",
                member.value.render()
            ),
            None => format!("{CONFIG} does not set {key}; the standard is {value}"),
        };
        crate::report(
            run.report(),
            "settings",
            (CONFIG, found.map(|member| member.line)),
            key,
            message,
            "write the standard value; the formatting of every repository is the same",
        );
    }
    for member in config.members() {
        if member.key == "$schema" || PRETTIER.iter().any(|(key, _)| *key == member.key) {
            continue;
        }
        crate::report(
            run.report(),
            "settings",
            (CONFIG, Some(member.line)),
            &member.key,
            format!(
                "{CONFIG} sets {}, which the standard leaves at Prettier's default",
                member.key
            ),
            "remove the key; the standard sets only printWidth, proseWrap and trailingComma",
        );
    }
    Ok(config
        .members()
        .iter()
        .map(|member| member.key.clone())
        .collect())
}

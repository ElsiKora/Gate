//! `node/formatter`: one Prettier configuration, a root `.prettierrc.json` with the standard
//! settings, and no `.editorconfig` that changes what Prettier writes.

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
        settings(run, &tree)?;
        for path in tree.paths() {
            if file_name(&path) == ".editorconfig"
                && !path.split('/').any(|segment| segment == "node_modules")
            {
                editorconfig(run, &tree, &path)?;
            }
        }
        Ok(())
    }
}

/// The `.editorconfig` keys Prettier reads, and the value each has under the standard: Prettier's
/// default, or the standard's `printWidth`.
const EDITORCONFIG: [(&str, &str); 5] = [
    ("indent_style", "space"),
    ("indent_size", "2"),
    ("tab_width", "2"),
    ("end_of_line", "lf"),
    ("max_line_length", "120"),
];

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

/// `editorconfig`: Prettier reads `.editorconfig`, so a key it maps to its options, in a section
/// that covers files Prettier formats, has the standard's value or is absent. The Prettier
/// configuration stays the one source of the values.
fn editorconfig(run: &mut Run<'_, NoOptions>, tree: &Tree<'_>, path: &str) -> Result<()> {
    let Some(bytes) = tree.bytes(path) else {
        return Ok(());
    };
    let text = String::from_utf8(bytes)
        .map_err(|_| Error::new(format!("{path} is not UTF-8 text; save it as UTF-8")))?;
    let mut section: Option<(String, bool)> = None;
    let mut indent_size = None;
    let mut tab_widths = Vec::new();
    for (index, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(pattern) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            flush_tab_width(
                run,
                path,
                section.as_ref(),
                indent_size.take(),
                &mut tab_widths,
            );
            section = Some((pattern.to_owned(), formats(pattern)));
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Some((pattern, true)) = &section else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().to_ascii_lowercase();
        if value == "unset" {
            continue;
        }
        let Some((_, standard)) = EDITORCONFIG.iter().find(|(name, _)| *name == key) else {
            continue;
        };
        match key.as_str() {
            // `indent_size = tab` defers to `tab_width`.
            "indent_size" => indent_size = Some(value.clone()),
            // `tab_width` sets Prettier's width only where `indent_size` is absent or `tab`.
            "tab_width" => {
                if value != *standard {
                    tab_widths.push((index + 1, value));
                }
                continue;
            }
            _ => {}
        }
        if value == *standard || (key == "indent_size" && value == "tab") {
            continue;
        }
        disagrees(run, path, pattern, &key, &value, standard, index + 1);
    }
    flush_tab_width(run, path, section.as_ref(), indent_size, &mut tab_widths);
    Ok(())
}

/// Reports the `tab_width` lines of the section that ended, when the section's `indent_size` is
/// absent or `tab`, so that Prettier takes its width from them.
fn flush_tab_width(
    run: &mut Run<'_, NoOptions>,
    path: &str,
    section: Option<&(String, bool)>,
    indent_size: Option<String>,
    tab_widths: &mut Vec<(usize, String)>,
) {
    let found = std::mem::take(tab_widths);
    let Some((pattern, true)) = section else {
        return;
    };
    if indent_size.is_some_and(|size| size != "tab") {
        return;
    }
    for (line, value) in found {
        disagrees(run, path, pattern, "tab_width", &value, "2", line);
    }
}

fn disagrees(
    run: &mut Run<'_, NoOptions>,
    path: &str,
    pattern: &str,
    key: &str,
    value: &str,
    standard: &str,
    line: usize,
) {
    crate::report(
        run.report(),
        "editorconfig",
        (path, Some(line)),
        &format!("[{pattern}]{key}"),
        format!(
            "{path} sets {key} = {value} for [{pattern}], and Prettier reads it; the standard \
             formats with {standard}"
        ),
        &format!(
            "remove the key or write {key} = {standard}; .prettierrc.json is the one source of \
             Prettier's settings"
        ),
    );
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

/// `settings`: the root configuration holds exactly the standard.
fn settings(run: &mut Run<'_, NoOptions>, tree: &Tree<'_>) -> Result<()> {
    let Some(config) = repo::read_json(tree, CONFIG)? else {
        crate::report(
            run.report(),
            "form",
            (CONFIG, None),
            CONFIG,
            format!("{CONFIG} is not tracked at the repository root"),
            "add .prettierrc.json with the standard settings: {\"printWidth\": 120, \"proseWrap\": \"preserve\", \"trailingComma\": \"all\"}",
        );
        return Ok(());
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
    Ok(())
}

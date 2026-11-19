//! `ledger/manifest`: the ledger manifest is well formed, its files and identifier families exist,
//! and no tracked text file names a forbidden ledger path.

use std::collections::HashSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::serde_json::{self, Value};
use gate_sdk::{Rule, Run};

use crate::catalog::{At, Catalog, Findings};
use crate::input::{self, INPUT, Input};
use crate::settings::check_path;
use crate::text::{extension, join, multiline_pattern};

/// The rule.
#[derive(Debug, Default)]
pub struct Manifest;

/// The options of `ledger/manifest`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The manifest; while it is not tracked, the rule reports it as `#input`.
    pub manifest: String,
    /// Directory names the forbidden-reference scan skips at any depth.
    pub skip_folders: Vec<String>,
    /// The extensions, with the dot, of the files the forbidden-reference scan reads; files without
    /// an extension are read too.
    pub text_extensions: Vec<String>,
    /// File names the scan reads whatever their extension: the name itself, or the name followed
    /// by `.` and a suffix (`Dockerfile.dev`).
    pub text_file_prefixes: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            manifest: "docs/ledger/manifest.json".to_owned(),
            skip_folders: [
                ".git",
                ".next",
                ".turbo",
                "build",
                "cdk.out",
                "coverage",
                "dist",
                "node_modules",
            ]
            .map(str::to_owned)
            .to_vec(),
            text_extensions: [
                ".cjs", ".conf", ".env", ".hcl", ".inc", ".js", ".json", ".md", ".mdc", ".mjs",
                ".mk", ".py", ".rb", ".sh", ".sql", ".ts", ".tsx", ".toml", ".yaml", ".yml",
            ]
            .map(str::to_owned)
            .to_vec(),
            text_file_prefixes: vec!["Dockerfile".to_owned()],
        }
    }
}

impl Rule for Manifest {
    type Options = Options;

    const NAME: &'static str = "manifest";
    const TITLE: &'static str = "The ledger manifest is well formed, its files and identifier families exist, and no file names a forbidden ledger path";
    const ASPECTS: &'static [&'static str] = &[
        "forbidden-reference",
        "id-family",
        INPUT,
        "manifest-file",
        "manifest-shape",
        "record-unique",
    ];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let tree = run.tree();
        check_path("ledger/manifest option manifest", &run.options().manifest)?;
        let inputs = [Input::own("manifest", &run.options().manifest)];

        if !input::tracked(run.report(), &tree, Self::NAME, &inputs) {
            return Ok(());
        }

        let options = run.options();
        let path = options.manifest.as_str();

        let catalog = Catalog::new(tree);
        let mut findings = Findings::default();
        let manifest_text = catalog.read(path);
        let checker = Checker {
            catalog: &catalog,
            manifest: path,
        };

        match serde_json::from_str::<Value>(&manifest_text) {
            Err(error) => findings.emit_keyed(
                "manifest-shape",
                &At::file(path),
                "json".to_owned(),
                format!("{path} is not valid JSON: {error}"),
            ),
            Ok(manifest) => checker.check(&manifest, options, &mut findings),
        }

        findings.write_all(run.report());
        Ok(())
    }
}

struct Checker<'c, 'h> {
    catalog: &'c Catalog<'h>,
    manifest: &'c str,
}

impl Checker<'_, '_> {
    fn check(&self, manifest: &Value, options: &Options, findings: &mut Findings) {
        let path = self.manifest;
        let tree = self.catalog.tree();
        let Some(shape) = Shape::read(manifest) else {
            findings.emit_keyed(
                "manifest-shape",
                &At::file(path),
                "required-fields".to_owned(),
                format!("{path} contains malformed required ledger manifest fields"),
            );
            return;
        };

        if shape.files.iter().collect::<HashSet<_>>().len() != shape.files.len() {
            findings.emit_keyed(
                "manifest-shape",
                &At::file(path),
                "canonicalFiles#duplicate".to_owned(),
                format!("{path} contains duplicate canonicalFiles entries"),
            );
        }

        for file in &shape.files {
            let required = format!("{}/{file}", shape.root);

            if !tree.contains(&join(&[shape.root, file])) {
                findings.emit_keyed(
                    "manifest-file",
                    &self.location(&required),
                    format!("canonicalFiles/{file}"),
                    format!("{required} is required by the ledger manifest"),
                );
            }
        }

        for &(family, definition) in &shape.families {
            self.family(&shape, family, definition, findings);
        }

        for scanned in tree.paths() {
            if scanned == path || !is_text_file(&scanned, options) {
                continue;
            }

            let Some(bytes) = tree.bytes(&scanned) else {
                continue;
            };

            for (label, literal) in &shape.forbidden {
                if let Some(start) = find(&bytes, literal.as_bytes()) {
                    let line = bytes[..start].split(|byte| *byte == b'\n').count();
                    findings.emit_keyed(
                        "forbidden-reference",
                        &At {
                            path: scanned.clone(),
                            line: u32::try_from(line).ok(),
                        },
                        (*label).to_owned(),
                        format!("{scanned} references forbidden ledger path: {label}"),
                    );
                }
            }
        }
    }

    fn family(&self, shape: &Shape<'_>, family: &str, definition: &Value, findings: &mut Findings) {
        let manifest = self.manifest;
        let tree = self.catalog.tree();
        let pattern = definition.get("pattern").and_then(Value::as_str);
        let file = definition.get("file");
        let directory = definition.get("directory");
        // A definition names exactly one string `file` or `directory`; a `file` beside a non-string
        // `directory` is malformed.
        let target = match (
            pattern,
            file.and_then(Value::as_str),
            directory.and_then(Value::as_str),
        ) {
            (Some(_), Some(file), None) if directory.is_none() => {
                Some((join(&[shape.root, file]), false))
            }
            (Some(_), None, Some(directory)) => Some((directory.to_owned(), true)),
            _ => None,
        };
        let (Some(pattern), Some((path, is_directory))) = (pattern, target) else {
            findings.emit_keyed(
                "manifest-shape",
                &At::file(manifest),
                format!("{family}#malformed"),
                format!("{manifest} contains malformed {family}-family definition"),
            );
            return;
        };
        let location = self.location(&path);
        let exists = if is_directory {
            !tree.under(&join(&[&path])).is_empty()
        } else {
            tree.contains(&path)
        };

        if !exists {
            findings.emit_keyed(
                "manifest-file",
                &location,
                format!("{family}#missing"),
                format!("{path} is required by the {family}-family definition"),
            );
            return;
        }

        let regex = match multiline_pattern(pattern) {
            Ok(regex) => regex,
            Err(error) => {
                findings.emit_keyed(
                    "manifest-shape",
                    &At::file(manifest),
                    format!("{family}#pattern"),
                    format!("{manifest} contains invalid {family}-family pattern: {error}"),
                );
                return;
            }
        };
        let matches = if is_directory {
            self.catalog.directory_matches(&join(&[&path]), &regex)
        } else {
            self.catalog.matches(&path, &regex)
        };

        if matches.is_empty() {
            findings.emit_keyed(
                "id-family",
                &location,
                family.to_owned(),
                format!("{path} has no {family}-family ledger IDs"),
            );
        }

        let texts: Vec<&str> = matches.iter().map(|found| found.group(0)).collect();

        if definition.get("unique").is_some_and(truthy)
            && texts.iter().collect::<HashSet<_>>().len() != texts.len()
        {
            findings.emit_keyed(
                "record-unique",
                &location,
                family.to_owned(),
                format!("{path} contains duplicate {family} records"),
            );
        }
    }

    /// The finding location of a manifest-declared path: the path itself when it stays inside the
    /// tree, otherwise the manifest.
    fn location(&self, path: &str) -> At {
        let normalized = join(&[path]);
        let normalized = if normalized.is_empty() {
            ".".to_owned()
        } else {
            normalized
        };

        if path.starts_with('/') || normalized.split('/').any(|part| part == "..") {
            At::file(self.manifest)
        } else {
            At::file(&normalized)
        }
    }
}

/// The offset of the first occurrence of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }

    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// The required manifest fields.
struct Shape<'a> {
    root: &'a str,
    files: Vec<&'a str>,
    /// Label and joined path of each forbidden reference.
    forbidden: Vec<(&'a str, String)>,
    families: Vec<(&'a str, &'a Value)>,
}

impl<'a> Shape<'a> {
    fn read(manifest: &'a Value) -> Option<Self> {
        let root = manifest.get("canonicalRoot")?.as_str()?;
        let files = manifest
            .get("canonicalFiles")?
            .as_array()?
            .iter()
            .map(Value::as_str)
            .collect::<Option<Vec<_>>>()?;
        let forbidden = manifest
            .get("forbiddenReferences")?
            .as_array()?
            .iter()
            .map(|reference| {
                let parts = reference
                    .get("parts")?
                    .as_array()?
                    .iter()
                    .map(Value::as_str)
                    .collect::<Option<Vec<_>>>()?;
                // An empty path would be found in every file.
                if parts.is_empty() {
                    return None;
                }
                Some((reference.get("label")?.as_str()?, parts.join("/")))
            })
            .collect::<Option<Vec<_>>>()?;
        let families = manifest
            .get("idFamilies")?
            .as_object()?
            .iter()
            .map(|(family, definition)| (family.as_str(), definition))
            .collect();

        Some(Self {
            root,
            files,
            forbidden,
            families,
        })
    }
}

/// Whether the forbidden-reference scan reads `path`.
fn is_text_file(path: &str, options: &Options) -> bool {
    let mut components: Vec<&str> = path.split('/').collect();
    components.pop();

    if components
        .iter()
        .any(|component| options.skip_folders.iter().any(|skip| skip == component))
    {
        return false;
    }

    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = extension(path);

    extension.is_empty()
        || options.text_extensions.iter().any(|text| text == extension)
        || options.text_file_prefixes.iter().any(|prefix| {
            name.strip_prefix(prefix.as_str())
                .is_some_and(|rest| rest.is_empty() || (rest.len() > 1 && rest.starts_with('.')))
        })
}

/// Whether a JSON value is set: anything but `null`, `false`, zero and the empty string.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

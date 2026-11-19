//! `nestjs/package-pins`: each listed package is exact-pinned to its immutable registry tarball in
//! the manifest and the lockfile, with one root resolution and no alias or link.

use crate::shared::paths::{self, Form};
use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::serde_json::{self, Map, Value};
use gate_sdk::{Location, Rule, Run};

/// The rule.
#[derive(Debug, Default)]
pub struct PackagePins;

/// One package and the exact artifact it must resolve to.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Artifact {
    /// The package name.
    pub package: String,
    /// The exact version.
    pub version: String,
    /// The registry tarball URL the lockfile resolves it to.
    pub resolved: String,
    /// The tarball's integrity hash.
    pub integrity: String,
}

/// The options of `nestjs/package-pins`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The packages that must be exact-pinned.
    pub artifacts: Vec<Artifact>,
    /// The npm manifest, relative to the project root.
    pub manifest: String,
    /// The npm lockfile, relative to the project root.
    pub lockfile: String,
    /// The dependency sections of the manifest, and of the lockfile's root package, in which
    /// another name must not alias a listed package.
    pub alias_sections: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            artifacts: Vec::new(),
            manifest: "package.json".to_owned(),
            lockfile: "package-lock.json".to_owned(),
            alias_sections: [
                "dependencies",
                "devDependencies",
                "optionalDependencies",
                "peerDependencies",
            ]
            .map(str::to_owned)
            .to_vec(),
        }
    }
}

impl Options {
    /// Checks that every path option is a normalized path relative to the project root.
    fn check_paths(&self) -> gate_sdk::Result<()> {
        paths::check("manifest", &self.manifest, Form::File)?;
        paths::check("lockfile", &self.lockfile, Form::File)
    }
}

impl Rule for PackagePins {
    type Options = Options;

    const NAME: &'static str = "package-pins";
    const TITLE: &'static str = "Listed packages are exact-pinned to their registry tarballs";

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        run.options().check_paths()?;
        let options = run.options();
        let (manifest_path, lockfile_path) = (options.manifest.clone(), options.lockfile.clone());
        let artifacts = options.artifacts.clone();
        let sections = options.alias_sections.clone();
        let tree = run.tree();
        let (Some(manifest), Some(lockfile)) =
            (tree.text(&manifest_path), tree.text(&lockfile_path))
        else {
            let at = if tree.contains(&manifest_path) {
                &lockfile_path
            } else {
                &manifest_path
            };
            run.report()
                .finding(
                    "",
                    Location::file(at.as_str()),
                    format!(
                        "{manifest_path} and {lockfile_path} are required for exact safe-error \
                         package pins"
                    ),
                )
                .help("commit the npm manifest and its lockfile")
                .key("required");
            return Ok(());
        };
        let manifest = parse(&manifest_path, &manifest)?;
        let lockfile = parse(&lockfile_path, &lockfile)?;
        let files = Files {
            manifest_path: &manifest_path,
            lockfile_path: &lockfile_path,
            manifest: &manifest,
            lockfile: &lockfile,
            sections: &sections,
        };

        for artifact in &artifacts {
            files.pins(artifact, run);
        }

        Ok(())
    }
}

/// `text` as JSON, or an error naming `path`.
fn parse(path: &str, text: &str) -> gate_sdk::Result<Value> {
    serde_json::from_str(text).map_err(|error| {
        gate_sdk::Error::new(format!(
            "{path} is not valid JSON ({error}); fix the file so npm can read it"
        ))
    })
}

/// The entries of a JSON object or array, as JavaScript's `Object.entries` reads them.
fn entries(value: &Value) -> Vec<(String, &Value)> {
    match value {
        Value::Object(object) => object
            .iter()
            .map(|(key, value)| (key.clone(), value))
            .collect(),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, value)| (index.to_string(), value))
            .collect(),
        _ => Vec::new(),
    }
}

/// The parsed manifest and lockfile.
struct Files<'f> {
    manifest_path: &'f str,
    lockfile_path: &'f str,
    manifest: &'f Value,
    lockfile: &'f Value,
    sections: &'f [String],
}

impl Files<'_> {
    /// Reports each way `artifact` is not exact-pinned.
    fn pins(&self, artifact: &Artifact, run: &mut Run<'_, Options>) {
        let name = artifact.package.as_str();
        let version = artifact.version.as_str();
        let empty = Value::Object(Map::new());
        let packages = match &self.lockfile["packages"] {
            value @ (Value::Object(_) | Value::Array(_)) => value,
            _ => &empty,
        };
        let root = &packages[""];
        let canonical = format!("node_modules/{name}");
        let nested = format!("/{canonical}");
        let alias = format!("npm:{name}@");
        let resolutions: Vec<String> = entries(packages)
            .into_iter()
            .filter(|(path, candidate)| {
                if *path == canonical || path.ends_with(&nested) {
                    return true;
                }

                let resolved = &candidate["resolved"];

                candidate.is_object()
                    && (candidate["name"] == name
                        || resolved == artifact.resolved.as_str()
                        || resolved == canonical.as_str()
                        || resolved.as_str().is_some_and(|resolved| {
                            resolved.ends_with(&nested) || resolved.starts_with(&alias)
                        })
                        || candidate["version"]
                            .as_str()
                            .is_some_and(|version| version.starts_with(&alias)))
            })
            .map(|(path, _)| path)
            .collect();
        let aliased = self
            .sections
            .iter()
            .flat_map(|section| [&self.manifest[section.as_str()], &root[section.as_str()]])
            .flat_map(entries)
            .any(|(declaration, value)| {
                declaration != name
                    && value
                        .as_str()
                        .is_some_and(|value| value.starts_with(&alias))
            });
        let locked = &packages[canonical.as_str()];
        let mut report = |path: &str, check: &str, message: String| {
            run.report()
                .finding("", Location::file(path), message)
                .help(
                    "pin the exact version in the manifest and reinstall so the lockfile \
                     resolves it once, from the registry tarball",
                )
                .key(format!("{name}#{check}"));
        };

        if self.manifest["dependencies"][name] != version {
            report(
                self.manifest_path,
                "pin",
                format!(
                    "{name} must be exact-pinned to {version} in {}",
                    self.manifest_path
                ),
            );
        }

        if root["dependencies"][name] != version {
            report(
                self.lockfile_path,
                "pin",
                format!(
                    "{name} must be exact-pinned to {version} in {}",
                    self.lockfile_path
                ),
            );
        }

        if resolutions.as_slice() != [canonical.clone()] || aliased {
            report(
                self.lockfile_path,
                "resolution",
                format!("{name} must have exactly one root npm registry resolution"),
            );
        }

        if locked["version"] != version
            || locked["resolved"] != artifact.resolved.as_str()
            || locked["integrity"] != artifact.integrity.as_str()
            || locked["link"] == true
        {
            report(
                self.lockfile_path,
                "artifact",
                format!(
                    "{name} lock artifact must be the immutable {version} npm registry tarball \
                     with exact integrity and no link/alias"
                ),
            );
        }
    }
}
